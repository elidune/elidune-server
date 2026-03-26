# Background Tasks — Frontend Integration Guide

Long-running server operations (MARC batch imports, maintenance runs) now execute
asynchronously.  Instead of waiting for an HTTP response that could take minutes,
the server returns a task ID immediately and the client polls for progress.

---

## Quick Reference

| Action | Endpoint | Method | Auth |
|--------|----------|--------|------|
| Start MARC import | `/api/v1/biblios/import-marc-batch` | POST | Staff |
| Start maintenance | `/api/v1/maintenance` | POST | Admin |
| List my tasks | `/api/v1/tasks` | GET | Any |
| Poll a task | `/api/v1/tasks/:id` | GET | Any |

---

## 1. Starting a Long-Running Operation

### MARC Batch Import

```http
POST /api/v1/biblios/import-marc-batch
  ?batch_id=927364819265437696
  &source_id=123456789
Authorization: Bearer <token>
```

Response — `202 Accepted`:
```json
{
  "taskId": "927364819265437697"
}
```

Store `taskId` and start polling.

### Maintenance Run

```http
POST /api/v1/maintenance
Authorization: Bearer <token>
Content-Type: application/json

{
  "actions": ["cleanupSeries", "mergeDuplicateSeries", "cleanupOrphanAuthors"]
}
```

Response — `202 Accepted`:
```json
{
  "taskId": "927364819265437698"
}
```

---

## 2. Polling a Task

```http
GET /api/v1/tasks/927364819265437697
Authorization: Bearer <token>
```

### Response Shape

```json
{
  "id": "927364819265437697",
  "kind": "marcBatchImport",
  "status": "running",
  "progress": {
    "current": 42,
    "total": 150,
    "message": "Importing record 42/150"
  },
  "result": null,
  "error": null,
  "createdAt": "2026-03-24T10:00:00Z",
  "startedAt": "2026-03-24T10:00:01Z",
  "completedAt": null,
  "userId": "818273645564928000"
}
```

### Status Values

| `status` | Meaning |
|----------|---------|
| `pending` | Task created, not yet started |
| `running` | Task is executing; `progress` is populated |
| `completed` | Task finished successfully; `result` is populated |
| `failed` | Task encountered a fatal error; `error` is populated |

### `result` shape by `kind`

#### `marcBatchImport`

```json
{
  "batchId": "927364819265437696",
  "imported": ["1", "2", "5"],
  "failed": [
    {
      "key": "3",
      "error": "Duplicate ISBN",
      "existingId": "42"
    }
  ]
}
```

#### `maintenance`

```json
{
  "reports": [
    {
      "action": "cleanupSeries",
      "success": true,
      "details": { "deleted": 12, "quotesStripped": 8 }
    },
    {
      "action": "mergeDuplicateSeries",
      "success": true,
      "details": { "merged": 3 }
    }
  ]
}
```

---

## 3. Recommended Polling Strategy

Poll with exponential back-off to avoid hammering the server:

```typescript
async function pollTask(
  taskId: string,
  token: string,
  onProgress?: (task: BackgroundTask) => void,
): Promise<BackgroundTask> {
  const BASE_MS = 500;
  const MAX_MS  = 5_000;
  let delay = BASE_MS;

  while (true) {
    const res  = await fetch(`/api/v1/tasks/${taskId}`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);

    const task: BackgroundTask = await res.json();

    onProgress?.(task);

    if (task.status === 'completed' || task.status === 'failed') {
      return task;
    }

    await sleep(delay);
    delay = Math.min(delay * 1.5, MAX_MS);
  }
}

function sleep(ms: number) {
  return new Promise(resolve => setTimeout(resolve, ms));
}
```

Typical call:

```typescript
const { taskId } = await startImport(batchId, sourceId);

const task = await pollTask(taskId, token, (t) => {
  if (t.progress) {
    updateProgressBar(t.progress.current, t.progress.total, t.progress.message);
  }
});

if (task.status === 'completed') {
  const report = task.result as MarcBatchImportReport;
  showImportSummary(report.imported, report.failed);
} else {
  showError(task.error ?? 'Unknown error');
}
```

---

## 4. Recovering Tasks After Reconnect / Page Refresh

Completed and failed task results are persisted in Redis for **24 hours**.
When the user logs back in, call `GET /tasks` to retrieve their history:

```http
GET /api/v1/tasks
Authorization: Bearer <token>
```

Response — an array of `BackgroundTask` objects, sorted newest-first:

```json
[
  {
    "id": "927364819265437697",
    "kind": "marcBatchImport",
    "status": "completed",
    "result": { "batchId": "...", "imported": ["1","2","3"], "failed": [] },
    ...
  },
  {
    "id": "927364819265437696",
    "kind": "maintenance",
    "status": "failed",
    "error": "PostgreSQL connection lost",
    ...
  }
]
```

### Reconnect Pattern

```typescript
// On app startup / after login
async function restoreTaskState(token: string) {
  const res   = await fetch('/api/v1/tasks', {
    headers: { Authorization: `Bearer ${token}` },
  });
  const tasks: BackgroundTask[] = await res.json();

  for (const task of tasks) {
    if (task.status === 'running' || task.status === 'pending') {
      // Resume polling for tasks that were in progress
      pollTask(task.id, token, onProgress).then(onComplete);
    } else {
      // Hydrate UI with already-finished results
      displayTaskResult(task);
    }
  }
}
```

> **Note**: If the server restarted while a task was running, the task will not
> appear in the list at all (it was never persisted). The frontend should handle
> this gracefully — e.g. by showing a "Task not found" message for any stored
> `taskId` that returns 404.

---

## 5. TypeScript Types

```typescript
export type TaskKind   = 'marcBatchImport' | 'maintenance';
export type TaskStatus = 'pending' | 'running' | 'completed' | 'failed';

export interface TaskProgress {
  current: number;
  total:   number;
  /** Free-form structured value; may be a string or an object with action details */
  message?: unknown;
}

// result shapes —  only present when status === 'completed'
export interface MarcBatchImportReport {
  batchId:  string;
  /** Array of successfully imported record keys (e.g. ["1", "2", "5"]) */
  imported: string[];
  failed: Array<{
    key:        string;
    error:      string;
    existingId?: string;
  }>;
}

export interface MaintenanceResponse {
  reports: Array<{
    action:  string;
    success: boolean;
    details: Record<string, number>;
    error?:  string;
  }>;
}

export interface BackgroundTask {
  id:           string;
  kind:         TaskKind;
  status:       TaskStatus;
  progress?:    TaskProgress;
  result?:      MarcBatchImportReport | MaintenanceResponse | null;
  error?:       string | null;
  createdAt:    string;   // ISO 8601
  startedAt?:   string | null;
  completedAt?: string | null;
  userId:       string;
}
```

---

## 6. Visibility Rules

| User role | Sees in `GET /tasks` |
|-----------|----------------------|
| Regular user / librarian | Only their own tasks |
| Admin | All **active** tasks from all users + their own completed tasks from Redis |

A task can only be fetched via `GET /tasks/:id` by the user who created it (or
any admin).  A 403 is returned for cross-user access.

---

## 7. Data Retention

| State | In-memory | Redis |
|-------|-----------|-------|
| `pending` / `running` | Yes | No (only user-index entry) |
| `completed` / `failed` | 5 min after completion | 24 h from completion |
| After 5 min completion | Evicted | Still available in Redis |

If a task completes and the frontend hasn't polled within 5 minutes, the
`GET /tasks/:id` endpoint will transparently fall back to Redis.

After 24 hours, the task data is permanently deleted.  Design the UI
accordingly (e.g. offer a "download report" button while the task is fresh).

---

## 8. Global API Convention — All JSON Keys Are camelCase

All JSON request bodies and responses across the entire API now use **camelCase**
keys consistently.  This is a breaking change from any prior snake_case usage.

### Key renames that affect common endpoints

| Endpoint | Old key | New key |
|----------|---------|---------|
| `POST /loans`, `GET /loans` | `user_id`, `item_id`, `issue_at`, `nb_renews`, `is_overdue` | `userId`, `itemId`, `issueAt`, `nbRenews`, `isOverdue` |
| `GET /users`, `POST /users` | `addr_street`, `addr_zip_code`, `addr_city`, `account_type`, `staff_type`, `hours_per_week`, `staff_start_date`, `staff_end_date`, `two_factor_enabled`, `two_factor_method`, `receive_reminders`, `must_change_password` | `addrStreet`, `addrZipCode`, `addrCity`, `accountType`, `staffType`, `hoursPerWeek`, `staffStartDate`, `staffEndDate`, `twoFactorEnabled`, `twoFactorMethod`, `receiveReminders`, `mustChangePassword` |
| `POST /auth/login` | `token_type`, `expires_in`, `requires_2fa`, `two_factor_method`, `device_id`, `must_change_password` | `tokenType`, `expiresIn`, `requires2fa`, `twoFactorMethod`, `deviceId`, `mustChangePassword` |
| `POST /auth/verify-2fa` | `user_id`, `trust_device` | `userId`, `trustDevice` |
| `GET /stats` | `start_date`, `end_date`, `public_type`, `media_type` (query params) | `startDate`, `endDate`, `publicType`, `mediaType` |
| `GET /stats/loans` | `start_date`, `end_date`, `public_type`, `media_type`, `user_id`, `total_loans`, `total_returns`, `time_series`, `by_media_type` | `startDate`, `endDate`, `publicType`, `mediaType`, `userId`, `totalLoans`, `totalReturns`, `timeSeries`, `byMediaType` |
| `POST /maintenance` | `actions` values: `cleanup_series`, `merge_duplicate_series`, etc. | `cleanupSeries`, `mergeDuplicateSeries`, etc. |
| `GET /biblios/import-report` | `existing_id`, `import_report` | `existingId`, `importReport` |
| `ImportAction` enum values | `created`, `merged_bibliographic`, `replaced_archived`, `replaced_confirmed` | `created`, `mergedBibliographic`, `replacedArchived`, `replacedConfirmed` |
| `GET /events/stats` | `total_events`, `total_attendees`, `school_visits`, `distinct_classes`, `total_students`, `by_type`, `event_type` | `totalEvents`, `totalAttendees`, `schoolVisits`, `distinctClasses`, `totalStudents`, `byType`, `eventType` |

### Enum values that stay lowercase (single-word variants)

These status enums already produced lowercase values and are **unchanged**:

- `FineStatus`: `pending`, `partial`, `paid`, `waived`
- `InventoryStatus`: `open`, `closed`
- `ReservationStatus`: `pending`, `ready`, `fulfilled`, `cancelled`, `expired`
- `AccountTypeSlug` (in user objects): `admin`, `librarian`, `reader`, `guest`

### Request body migration example

```diff
// POST /loans — Create a loan
{
-  "user_id": "123456789",
-  "item_id": "987654321",
-  "item_identification": null,
+  "userId": "123456789",
+  "itemId": "987654321",
+  "itemIdentification": null,
   "force": false
}
```

```diff
// POST /maintenance
{
-  "actions": ["cleanup_series", "merge_duplicate_series"]
+  "actions": ["cleanupSeries", "mergeDuplicateSeries"]
}
```
