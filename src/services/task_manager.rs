//! Background task manager.
//!
//! Tracks long-running server-side operations (MARC batch import, maintenance, …).
//! Active tasks are held in an in-memory map; completed/failed outcomes are
//! persisted to Redis so the frontend can retrieve them after a page refresh or
//! session reconnect.
//!
//! ## Redis keys
//! | Key                       | Value                              | TTL  |
//! |---------------------------|------------------------------------|------|
//! | `task:{id}`               | JSON-serialised `BackgroundTask`   | 24 h |
//! | `task:user:{user_id}`     | Redis Set of task-id strings       | 24 h |

use std::{collections::HashMap, future::Future, sync::Arc};

use chrono::Utc;
use redis::AsyncCommands;
use tokio::sync::RwLock;

use crate::{
    models::task::{BackgroundTask, TaskKind, TaskProgress, TaskStatus},
    services::redis::RedisService,
};

const TASK_TTL_SECS: u64 = 24 * 60 * 60;
/// Time to keep a completed task in memory before evicting it.
const MEMORY_GRACE_SECS: u64 = 5 * 60;

type TaskMap = Arc<std::sync::RwLock<HashMap<i64, Arc<RwLock<BackgroundTask>>>>>;

// ── TaskHandle ────────────────────────────────────────────────────────────────

/// Handle passed to a spawned task closure; used to report progress and outcome.
///
/// Cloning is cheap — all fields are `Arc`-backed.
#[derive(Clone)]
pub struct TaskHandle {
    pub id: i64,
    task: Arc<RwLock<BackgroundTask>>,
    redis: RedisService,
}

impl TaskHandle {
    /// Update running progress (in-memory only; fast, no I/O).
    ///
    /// `message` can be any serialisable value — a plain string, a structured
    /// object, etc.  Pass `None` to omit the message field.
    pub async fn set_progress(
        &self,
        current: usize,
        total: usize,
        message: Option<serde_json::Value>,
    ) {
        let mut task = self.task.write().await;
        task.progress = Some(TaskProgress { current, total, message });
    }

    /// Mark the task as completed and persist the result to Redis.
    ///
    /// Must be called exactly once before the spawned closure returns.
    pub async fn complete(&self, result: serde_json::Value) {
        {
            let mut task = self.task.write().await;
            task.status = TaskStatus::Completed;
            task.result = Some(result);
            task.completed_at = Some(Utc::now());
            task.progress = None;
        }
        self.persist().await;
    }

    /// Mark the task as failed and persist the error to Redis.
    ///
    /// Must be called exactly once before the spawned closure returns.
    pub async fn fail(&self, error: String) {
        {
            let mut task = self.task.write().await;
            task.status = TaskStatus::Failed;
            task.error = Some(error);
            task.completed_at = Some(Utc::now());
            task.progress = None;
        }
        self.persist().await;
    }

    /// Return a snapshot of the current task state.
    pub async fn snapshot(&self) -> BackgroundTask {
        self.task.read().await.clone()
    }

    /// Write the current state to Redis asynchronously (fire-and-forget).
    async fn persist(&self) {
        let snapshot = self.task.read().await.clone();
        let redis = self.redis.clone();
        let user_id = snapshot.user_id;
        let task_id = snapshot.id;

        tokio::spawn(async move {
            let Ok(json) = serde_json::to_string(&snapshot) else { return };
            let Ok(mut conn) = redis.get_connection().await else { return };

            let task_key = format!("task:{task_id}");
            let user_key = format!("task:user:{user_id}");

            let _: Result<(), _> = redis::cmd("SETEX")
                .arg(&task_key)
                .arg(TASK_TTL_SECS)
                .arg(&json)
                .query_async(&mut conn)
                .await;

            let _: Result<(), _> = conn.sadd(&user_key, task_id.to_string()).await;

            let _: Result<(), _> = redis::cmd("EXPIRE")
                .arg(&user_key)
                .arg(TASK_TTL_SECS)
                .query_async(&mut conn)
                .await;
        });
    }
}

// ── TaskManager ───────────────────────────────────────────────────────────────

/// Registry of background tasks for all users.
///
/// Clone-friendly; the internal map and Redis client are `Arc`-backed.
#[derive(Clone)]
pub struct TaskManager {
    active: TaskMap,
    redis: RedisService,
}

impl TaskManager {
    pub fn new(redis: RedisService) -> Self {
        Self {
            active: Arc::new(std::sync::RwLock::new(HashMap::new())),
            redis,
        }
    }

    /// Spawn a background task and return its ID immediately (non-blocking).
    ///
    /// The closure receives a [`TaskHandle`] and **must** call either
    /// `handle.complete(result)` or `handle.fail(reason)` before returning.
    ///
    /// The spawned Tokio task transitions the status from `Pending` → `Running`
    /// before calling the closure, and removes the task from memory
    /// [`MEMORY_GRACE_SECS`] after it finishes so that the last in-memory read
    /// can still succeed.
    pub fn spawn_task<F, Fut>(&self, kind: TaskKind, user_id: i64, f: F) -> i64
    where
        F: FnOnce(TaskHandle) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let task_id: i64 = snowflaked::Generator::new(1).generate();

        let task = BackgroundTask {
            id: task_id,
            kind,
            status: TaskStatus::Pending,
            progress: None,
            result: None,
            error: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            user_id,
        };

        let task_arc = Arc::new(RwLock::new(task));
        self.active
            .write()
            .unwrap()
            .insert(task_id, task_arc.clone());

        let handle = TaskHandle {
            id: task_id,
            task: task_arc,
            redis: self.redis.clone(),
        };

        let active_clone = Arc::clone(&self.active);
        let redis_for_index = self.redis.clone();

        tokio::spawn(async move {
            // Transition to Running and register user index in Redis
            {
                let mut t = handle.task.write().await;
                t.status = TaskStatus::Running;
                t.started_at = Some(Utc::now());
            }
            if let Ok(mut conn) = redis_for_index.get_connection().await {
                let user_key = format!("task:user:{user_id}");
                let _: Result<(), _> = conn.sadd(&user_key, task_id.to_string()).await;
                let _: Result<(), _> = redis::cmd("EXPIRE")
                    .arg(&user_key)
                    .arg(TASK_TTL_SECS)
                    .query_async(&mut conn)
                    .await;
            }

            f(handle).await;

            // Grace period: keep in memory so any in-flight GET /tasks/:id
            // can still read the completed state from memory before eviction.
            tokio::time::sleep(tokio::time::Duration::from_secs(MEMORY_GRACE_SECS)).await;
            active_clone.write().unwrap().remove(&task_id);
        });

        task_id
    }

    /// Get a task by ID.
    ///
    /// Checks the in-memory map first (active / recently completed), then falls
    /// back to Redis for tasks that finished more than [`MEMORY_GRACE_SECS`] ago.
    pub async fn get_task(&self, task_id: i64) -> Option<BackgroundTask> {
        // Acquire and release the sync guard before any await point.
        let arc_opt = {
            let guard = self.active.read().unwrap();
            guard.get(&task_id).cloned()
        };
        if let Some(arc) = arc_opt {
            return Some(arc.read().await.clone());
        }
        self.load_from_redis(task_id).await
    }

    /// List tasks visible to a user, sorted newest-first.
    ///
    /// - Regular users see only their own tasks.
    /// - Admins see **all** in-memory active tasks plus their own completed tasks
    ///   from Redis.
    pub async fn list_tasks(&self, user_id: i64, is_admin: bool) -> Vec<BackgroundTask> {
        // Collect Arc handles without holding the sync lock across awaits.
        let arcs: Vec<Arc<RwLock<BackgroundTask>>> = {
            let guard = self.active.read().unwrap();
            guard.values().cloned().collect()
        };

        let mut result: Vec<BackgroundTask> = Vec::new();
        for arc in arcs {
            let snap = arc.read().await.clone();
            if is_admin || snap.user_id == user_id {
                result.push(snap);
            }
        }

        // Merge completed tasks from Redis (own tasks only — no cross-user scan).
        let redis_tasks = self.load_user_tasks_from_redis(user_id).await;
        let active_ids: std::collections::HashSet<i64> = result.iter().map(|t| t.id).collect();
        for t in redis_tasks {
            if !active_ids.contains(&t.id) {
                result.push(t);
            }
        }

        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        result
    }

    async fn load_from_redis(&self, task_id: i64) -> Option<BackgroundTask> {
        let mut conn = self.redis.get_connection().await.ok()?;
        let key = format!("task:{task_id}");
        let json: String = conn.get(&key).await.ok()?;
        serde_json::from_str(&json).ok()
    }

    async fn load_user_tasks_from_redis(&self, user_id: i64) -> Vec<BackgroundTask> {
        let Ok(mut conn) = self.redis.get_connection().await else {
            return vec![];
        };
        let user_key = format!("task:user:{user_id}");
        let ids: Vec<String> = match conn.smembers(&user_key).await {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let mut tasks = Vec::with_capacity(ids.len());
        for id_str in &ids {
            let Ok(task_id) = id_str.parse::<i64>() else { continue };
            let key = format!("task:{task_id}");
            let Ok(json): Result<String, _> = conn.get(&key).await else { continue };
            if let Ok(t) = serde_json::from_str::<BackgroundTask>(&json) {
                tasks.push(t);
            }
        }
        tasks
    }
}
