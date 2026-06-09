//! Email outbox integration tests: repository helpers, outbox worker, and API flows.

mod common;

use std::sync::atomic::{AtomicU32, Ordering};

use axum::http::StatusCode;
use chrono::Utc;
use common::{fixtures, TestApp};
use elidune_server::{
    repository::Repository,
    services::{email_outbox, operational_metrics},
    EmailService,
};
use http_body_util::BodyExt;
use once_cell::sync::Lazy;
use serde_json::json;
use tokio::sync::Mutex;

/// Serialise DB mutations: snowflake IDs and outbox worker batches are order-sensitive.
static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static SNOWFLAKE_WORKER: AtomicU32 = AtomicU32::new(1);

async fn test_guard() -> tokio::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().await
}

// ---------------------------------------------------------------------------
// Repository + EmailService
// ---------------------------------------------------------------------------

#[tokio::test]
async fn overdue_reminder_enqueue_reserves_loans_until_sent() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let repo = app.state.services.repository.as_ref().clone();
    let email = email_service(&app);

    let (loan_ids, patron_email) = seed_overdue_loan(&repo).await;

    let outbox_id = email
        .enqueue_overdue_reminder(
            &patron_email,
            "Overdue reminder",
            "plain body",
            "<p>html body</p>",
            &loan_ids,
        )
        .await
        .expect("enqueue overdue reminder");

    let reserved = repo
        .email_outbox_reminder_loan_ids(outbox_id)
        .await
        .expect("reserved loan ids");
    assert_eq!(reserved, loan_ids);

    let eligible = repo
        .loans_get_overdue_for_reminders(7)
        .await
        .expect("overdue query");
    assert!(
        eligible.iter().all(|row| !loan_ids.contains(&row.loan_id)),
        "reserved loans must be excluded while outbox is pending"
    );

    repo.loans_update_reminder_sent(&loan_ids)
        .await
        .expect("mark reminded after SMTP");
    repo.email_outbox_release_reminder_loans(outbox_id)
        .await
        .expect("release reservation");

    sqlx::query("UPDATE email_outbox SET status = 'sent', sent_at = $2 WHERE id = $1")
        .bind(outbox_id)
        .bind(Utc::now())
        .execute(repo.pool())
        .await
        .expect("mark outbox sent");

    let reminder_count: i32 = sqlx::query_scalar(
        "SELECT reminder_count FROM loans WHERE id = $1",
    )
    .bind(loan_ids[0])
    .fetch_one(repo.pool())
    .await
    .expect("reminder count");
    assert_eq!(reminder_count, 1);

    let eligible_after = repo
        .loans_get_overdue_for_reminders(7)
        .await
        .expect("overdue query after sent");
    assert!(
        eligible_after.iter().all(|row| !loan_ids.contains(&row.loan_id)),
        "loans stay excluded until frequency window elapses after reminder tracking update"
    );
}

#[tokio::test]
async fn generic_enqueue_creates_pending_outbox_row() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let repo = app.state.services.repository.as_ref().clone();
    let email = email_service(&app);

    let outbox_id = email
        .enqueue("generic@test.local", "Subject", "plain", "<p>html</p>")
        .await
        .expect("enqueue generic email");

    assert_eq!(outbox_status(&repo, outbox_id).await, "pending");

    let linked_event = repo
        .email_outbox_event_id_for_outbox(outbox_id)
        .await
        .expect("event lookup");
    assert!(linked_event.is_none());

    let reserved = repo
        .email_outbox_reminder_loan_ids(outbox_id)
        .await
        .expect("reminder loans");
    assert!(reserved.is_empty());
}

#[tokio::test]
async fn event_announcement_pending_count_tracks_outbox_rows() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let repo = app.state.services.repository.as_ref().clone();
    let email = email_service(&app);

    let event_id = seed_event(&repo, "Outbox test event").await;

    let outbox_a = email
        .enqueue_event_announcement(
            "a@test.local",
            "Event",
            "plain",
            "<p>html</p>",
            event_id,
        )
        .await
        .expect("enqueue a");
    let outbox_b = email
        .enqueue_event_announcement(
            "b@test.local",
            "Event",
            "plain",
            "<p>html</p>",
            event_id,
        )
        .await
        .expect("enqueue b");

    assert_eq!(
        repo.email_outbox_event_id_for_outbox(outbox_a)
            .await
            .expect("event id a"),
        Some(event_id)
    );
    assert_eq!(
        repo.email_outbox_event_id_for_outbox(outbox_b)
            .await
            .expect("event id b"),
        Some(event_id)
    );

    assert_eq!(
        repo.email_outbox_pending_event_announcement_count(event_id)
            .await
            .expect("pending count"),
        2
    );

    sqlx::query("UPDATE email_outbox SET status = 'sent', sent_at = NOW() WHERE id = $1")
        .bind(outbox_a)
        .execute(repo.pool())
        .await
        .expect("sent a");
    repo.email_outbox_release_event_announcement(outbox_a)
        .await
        .expect("release a");

    assert_eq!(
        repo.email_outbox_pending_event_announcement_count(event_id)
            .await
            .expect("pending after one sent"),
        1
    );

    sqlx::query("UPDATE email_outbox SET status = 'sent', sent_at = NOW() WHERE id = $1")
        .bind(outbox_b)
        .execute(repo.pool())
        .await
        .expect("sent b");
    repo.email_outbox_release_event_announcement(outbox_b)
        .await
        .expect("release b");

    assert_eq!(
        repo.email_outbox_pending_event_announcement_count(event_id)
            .await
            .expect("pending after all sent"),
        0
    );

    repo.events_set_announcement_sent_at(event_id)
        .await
        .expect("mark announcement sent");
    let sent_at: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
        "SELECT announcement_sent_at FROM events WHERE id = $1",
    )
    .bind(event_id)
    .fetch_one(repo.pool())
    .await
    .expect("announcement_sent_at");
    assert!(sent_at.is_some());
}

#[tokio::test]
async fn metrics_snapshot_reflects_pending_outbox_rows() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let repo = app.state.services.repository.as_ref().clone();
    let email = email_service(&app);

    let before = repo.metrics_snapshot().await.expect("snapshot before");
    let _ = email
        .enqueue("metrics@test.local", "Metrics", "plain", "<p>x</p>")
        .await
        .expect("enqueue");

    let after = repo.metrics_snapshot().await.expect("snapshot after");
    assert_eq!(
        after.outbox_pending_count,
        before.outbox_pending_count + 1
    );
    assert!(after.outbox_oldest_pending_seconds >= 0);
}

// ---------------------------------------------------------------------------
// Outbox worker (scheduler path)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn process_outbox_batch_marks_invalid_body_failed_and_releases_loans() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let repo = app.state.services.repository.as_ref().clone();
    let email = email_service(&app);
    let audit = app.state.services.audit.clone();

    let (loan_ids, _) = seed_overdue_loan(&repo).await;
    let outbox_id = insert_raw_outbox(&repo, "bad-body@test.local", "not-json", 0).await;
    link_reminder_loans(&repo, outbox_id, &loan_ids).await;

    let report = email_outbox::process_outbox_batch(&email, &repo, &audit, Some(10))
        .await
        .expect("process batch");

    assert!(report.processed >= 1);
    assert!(report.failed >= 1);
    assert_eq!(outbox_status(&repo, outbox_id).await, "failed");
    assert!(
        repo.email_outbox_reminder_loan_ids(outbox_id)
            .await
            .expect("released loans")
            .is_empty()
    );

    let eligible = repo
        .loans_get_overdue_for_reminders(7)
        .await
        .expect("eligible after failure");
    assert!(
        eligible.iter().any(|row| loan_ids.contains(&row.loan_id)),
        "failed delivery must release loan reservations"
    );
}

#[tokio::test]
async fn process_outbox_batch_defers_on_smtp_failure_and_keeps_reservations() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let repo = app.state.services.repository.as_ref().clone();
    let email = email_service(&app);
    let audit = app.state.services.audit.clone();

    let (loan_ids, _) = seed_overdue_loan(&repo).await;
    let body = json!({"plain": "hello", "html": "<p>hello</p>"}).to_string();
    let outbox_id = insert_raw_outbox(&repo, "defer@test.local", &body, 0).await;
    link_reminder_loans(&repo, outbox_id, &loan_ids).await;

    let report = email_outbox::process_outbox_batch(&email, &repo, &audit, Some(10))
        .await
        .expect("process batch");

    assert!(report.processed >= 1);
    assert!(report.deferred >= 1);
    assert_eq!(outbox_status(&repo, outbox_id).await, "pending");

    let attempts: i32 = sqlx::query_scalar("SELECT attempts FROM email_outbox WHERE id = $1")
        .bind(outbox_id)
        .fetch_one(repo.pool())
        .await
        .expect("attempts");
    assert_eq!(attempts, 1);

    assert_eq!(
        repo.email_outbox_reminder_loan_ids(outbox_id)
            .await
            .expect("still reserved"),
        loan_ids
    );
}

#[tokio::test]
async fn process_outbox_batch_permanent_failure_releases_reservations() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let repo = app.state.services.repository.as_ref().clone();
    let email = email_service(&app);
    let audit = app.state.services.audit.clone();

    let (loan_ids, _) = seed_overdue_loan(&repo).await;
    let body = json!({"plain": "hello", "html": "<p>hello</p>"}).to_string();
    let outbox_id = insert_raw_outbox(&repo, "fail@test.local", &body, 4).await;
    link_reminder_loans(&repo, outbox_id, &loan_ids).await;

    let report = email_outbox::process_outbox_batch(&email, &repo, &audit, Some(10))
        .await
        .expect("process batch");

    assert!(report.processed >= 1);
    assert!(report.failed >= 1);
    assert_eq!(outbox_status(&repo, outbox_id).await, "failed");
    assert!(
        repo.email_outbox_reminder_loan_ids(outbox_id)
            .await
            .expect("released")
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// HTTP API flows
// ---------------------------------------------------------------------------

#[tokio::test]
async fn api_get_overdue_loans_lists_seeded_loan() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let admin_token = admin_token(&app).await;
    let repo = app.state.services.repository.as_ref().clone();
    let (loan_ids, _) = seed_overdue_loan(&repo).await;

    let (status, body) = app
        .get_json_with_auth("/api/v1/loans/overdue", &admin_token)
        .await;
    assert_eq!(status, StatusCode::OK, "overdue list: {body}");

    let ids: Vec<i64> = body["loans"]
        .as_array()
        .expect("loans array")
        .iter()
        .map(|loan| {
            loan["loanId"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| loan["loanId"].as_i64())
                .expect("loanId in overdue response")
        })
        .collect();
    assert!(ids.contains(&loan_ids[0]));
    assert!(body["total"].as_i64().unwrap_or(0) >= 1);
}

#[tokio::test]
async fn api_send_overdue_reminders_dry_run_does_not_enqueue() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let admin_token = admin_token(&app).await;
    let repo = app.state.services.repository.as_ref().clone();
    let _ = seed_overdue_loan(&repo).await;

    let before = pending_outbox_count(&repo).await;
    let (status, body) = app
        .post_empty(
            "/api/v1/loans/send-overdue-reminders?dryRun=true",
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "dry run: {body}");
    assert!(body["emailsSent"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(body["dryRun"], true);
    assert_eq!(pending_outbox_count(&repo).await, before);
}

#[tokio::test]
async fn api_send_overdue_reminders_enqueues_and_reserves_loans() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let admin_token = admin_token(&app).await;
    let repo = app.state.services.repository.as_ref().clone();
    let (loan_ids, _) = seed_overdue_loan(&repo).await;

    let (status, body) = app
        .post_empty("/api/v1/loans/send-overdue-reminders", Some(&admin_token))
        .await;
    assert_eq!(status, StatusCode::OK, "send reminders: {body}");
    assert!(body["emailsSent"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(body["dryRun"], false);

    let eligible = repo
        .loans_get_overdue_for_reminders(7)
        .await
        .expect("eligible after enqueue");
    assert!(
        eligible.iter().all(|row| !loan_ids.contains(&row.loan_id)),
        "API enqueue must reserve loans in outbox"
    );
}

#[tokio::test]
async fn api_send_event_announcement_enqueues_outbox_rows() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    let admin_token = admin_token(&app).await;
    let repo = app.state.services.repository.as_ref().clone();
    let event_id = seed_event(&repo, "API announcement event").await;

    let payload = json!({
        "subject": "Library event",
        "bodyPlain": "Join us tomorrow.",
        "bodyHtml": "<p>Join us tomorrow.</p>"
    });

    let (status, body) = app
        .post_json(
            &format!("/api/v1/events/{event_id}/send-announcement"),
            &payload,
            Some(&admin_token),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "send announcement: {body}");
    assert!(body["emailsSent"].as_u64().unwrap_or(0) >= 1);

    assert!(
        repo.email_outbox_pending_event_announcement_count(event_id)
            .await
            .expect("pending count") >= 1
    );
}

#[tokio::test]
async fn api_metrics_exposes_outbox_gauges() {
    let _guard = test_guard().await;
    let Some(app) = TestApp::spawn().await else {
        return;
    };

    operational_metrics::init_prometheus_recorder().expect("prometheus recorder");

    let repo = app.state.services.repository.as_ref().clone();
    let email = email_service(&app);
    let _ = email
        .enqueue("prom@test.local", "Prom", "plain", "<p>x</p>")
        .await
        .expect("enqueue");

    let snapshot = repo.metrics_snapshot().await.expect("snapshot");
    assert!(snapshot.outbox_pending_count >= 1);

    let response = app
        .request(
            axum::http::Request::builder()
                .method("GET")
                .uri("/metrics")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("elidune_outbox_pending_count"));
    assert!(text.contains("elidune_active_loans"));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn email_service(app: &TestApp) -> EmailService {
    let repo = app.state.services.repository.as_ref();
    EmailService::new(app.state.dynamic_config.clone(), repo.pool().clone())
}

fn snowflake() -> i64 {
    let worker = SNOWFLAKE_WORKER.fetch_add(1, Ordering::Relaxed) % 1022 + 1;
    snowflaked::Generator::new(worker as u16).generate::<i64>()
}

async fn admin_token(app: &TestApp) -> String {
    let (_, health) = app.get_json("/api/v1/health").await;
    if health["setup"]["needFirstSetup"].as_bool() == Some(true) {
        return fixtures::ensure_first_setup(app).await;
    }

    let repo = app.state.services.repository.as_ref();
    let admin_id = ensure_admin_user(repo).await;
    let user = app
        .state
        .services
        .users
        .get_by_id(admin_id)
        .await
        .expect("load admin");
    app.state
        .services
        .users
        .issue_access_token(&user)
        .await
        .expect("issue admin token")
}

async fn ensure_admin_user(repo: &Repository) -> i64 {
    if let Some(id) = sqlx::query_scalar(
        r#"
        SELECT id FROM users
        WHERE account_type = 'admin'
          AND (status IS NULL OR status <> 'deleted')
        ORDER BY created_at
        LIMIT 1
        "#,
    )
    .fetch_optional(repo.pool())
    .await
    .expect("lookup admin user")
    {
        return id;
    }

    let suffix = Utc::now().timestamp_micros();
    sqlx::query_scalar(
        r#"
        INSERT INTO users (
            id, login, password, firstname, lastname, email, account_type,
            sex, birthdate, language, receive_reminders, token_version, created_at, update_at
        )
        VALUES ($1, $2, 'hash', 'Outbox', 'Admin', $3, 'admin',
                'm', '1980-01-01', 'french', FALSE, 0, NOW(), NOW())
        RETURNING id
        "#,
    )
    .bind(snowflake())
    .bind(format!("outbox_admin_{suffix}"))
    .bind(format!("outbox-admin-{suffix}@test.local"))
    .fetch_one(repo.pool())
    .await
    .expect("seed admin user")
}

async fn seed_overdue_loan(repo: &Repository) -> (Vec<i64>, String) {
    let suffix = Utc::now().timestamp_micros();
    let patron_email = format!("patron_{suffix}@test.local");
    let user_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO users (
            id, login, password, firstname, lastname, email, account_type,
            sex, birthdate, language, receive_reminders, token_version, created_at, update_at
        )
        VALUES ($1, $2, 'hash', 'Patron', 'Test', $3, 'reader',
                'm', '1990-01-01', 'french', TRUE, 0, NOW(), NOW())
        RETURNING id
        "#,
    )
    .bind(snowflake())
    .bind(format!("outbox_user_{suffix}"))
    .bind(&patron_email)
    .fetch_one(repo.pool())
    .await
    .expect("insert user");

    let biblio_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO biblios (id, title, media_type, lang)
        VALUES ($1, 'Outbox overdue title', 'printedText', 'fre')
        RETURNING id
        "#,
    )
    .bind(snowflake())
    .fetch_one(repo.pool())
    .await
    .expect("insert biblio");

    let item_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO items (id, biblio_id, barcode, borrowable)
        VALUES ($1, $2, $3, TRUE)
        RETURNING id
        "#,
    )
    .bind(snowflake())
    .bind(biblio_id)
    .bind(format!("OUTBOX-{}", Utc::now().timestamp_micros()))
    .fetch_one(repo.pool())
    .await
    .expect("insert item");

    let loan_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO loans (id, user_id, item_id, date, expiry_at)
        VALUES ($1, $2, $3, NOW() - INTERVAL '30 days', NOW() - INTERVAL '7 days')
        RETURNING id
        "#,
    )
    .bind(snowflake())
    .bind(user_id)
    .bind(item_id)
    .fetch_one(repo.pool())
    .await
    .expect("insert overdue loan");

    (vec![loan_id], patron_email)
}

async fn seed_event(repo: &Repository, name: &str) -> i64 {
    sqlx::query_scalar(
        r#"
        INSERT INTO events (id, name, event_type, event_date, created_at, update_at)
        VALUES ($1, $2, 6, CURRENT_DATE, NOW(), NOW())
        RETURNING id
        "#,
    )
    .bind(snowflake())
    .bind(name)
    .fetch_one(repo.pool())
    .await
    .expect("insert event")
}

async fn insert_raw_outbox(repo: &Repository, to: &str, body: &str, attempts: i32) -> i64 {
    let id = snowflake();
    sqlx::query(
        r#"
        INSERT INTO email_outbox (id, to_addr, subject, body, status, attempts, created_at)
        VALUES ($1, $2, 'Test subject', $3, 'pending', $4, NOW() - INTERVAL '1 year')
        "#,
    )
    .bind(id)
    .bind(to)
    .bind(body)
    .bind(attempts)
    .execute(repo.pool())
    .await
    .expect("insert outbox row");
    id
}

async fn link_reminder_loans(repo: &Repository, outbox_id: i64, loan_ids: &[i64]) {
    for loan_id in loan_ids {
        sqlx::query(
            "INSERT INTO email_outbox_reminder_loans (outbox_id, loan_id) VALUES ($1, $2)",
        )
        .bind(outbox_id)
        .bind(loan_id)
        .execute(repo.pool())
        .await
        .expect("link reminder loan");
    }
}

async fn pending_outbox_count(repo: &Repository) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM email_outbox WHERE status = 'pending'")
        .fetch_one(repo.pool())
        .await
        .expect("pending count")
}

async fn outbox_status(repo: &Repository, outbox_id: i64) -> String {
    sqlx::query_scalar("SELECT status FROM email_outbox WHERE id = $1")
        .bind(outbox_id)
        .fetch_one(repo.pool())
        .await
        .expect("outbox status")
}
