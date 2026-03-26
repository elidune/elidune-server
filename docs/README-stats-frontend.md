# Frontend — Statistics query builder and saved queries

This document describes what the web app must implement so staff can **design**, **save**, **list**, and **run** flexible statistics (library KPIs without new backend endpoints).

## Audience and permissions

- All endpoints below require a **Bearer JWT** (`Authorization: Bearer <token>`).
- Callers must be **staff** (`librarian` or `admin`). Non-staff receive **403**.
- **Saved queries**: non-admin users see **their own** rows plus rows with `isShared: true`. **Admins** see **all** saved queries in the list.

## Base URL

All paths are under `/api/v1` (e.g. `GET /api/v1/stats/schema`).

## 1) Discovery — drive the UI from the server

### `GET /stats/schema`

Returns a JSON document the UI must use to:

- Build entity / join pickers (only allowed relations are listed).
- Build field pickers (types + human labels).
- Know valid **aggregation functions**, **filter operators**, and **time granularities**.

Important fields (camelCase):

| Field | Purpose |
|----------|---------|
| `entities` | Map of entity key → `{ label, fields, relations }` |
| `fields` | Map field name → `{ type, label }` |
| `relations` | Map relation name → `{ join: [left, right], label }` |
| `aggregationFunctions` | e.g. `count`, `countDistinct`, … |
| `operators` | e.g. `eq`, `gte`, `in`, `isNull`, … |
| `timeGranularities` | `day`, `week`, `month`, `quarter`, `year` |

**Frontend responsibility:** treat this as the **only** source of truth for allowed fields and joins. Do not hardcode table names; when the backend evolves, the UI updates automatically after refresh.

## 2) Query builder — produce a JSON body

The user composes a declarative query in memory. The shape matches **`StatsBuilderBody`** in OpenAPI (see Swagger `/api/docs`).

### Core concepts

- **`entity`**: root table (e.g. `loans`, `visitor_counts`).
- **`joins`**: array of **dot paths** from the root, e.g. `users`, `items.biblios`, `users.public_types`. The server resolves joins and assigns unique SQL aliases (e.g. `users__public_types`).
- **`select`**: non-aggregated columns to return (each item: `field`, optional `alias`).
- **`filters`**: `WHERE` predicates (`field`, `op`, `value`).
- **`aggregations`**: aggregate expressions (`fn`, `field`, `alias`). Use `alias` in `orderBy` / `having`.
- **`groupBy`**: dimensions for `GROUP BY` (required when mixing raw dimensions and aggregates).
- **`having`**: filters on **aggregation aliases** (not raw columns).
- **`timeBucket`**: `DATE_TRUNC` on a timestamp/date field (optional).
- **`orderBy`**: sort by output column **alias** (quoted identifiers in SQL).
- **`limit` / `offset`**: pagination (server caps `limit` at 10 000).

### JSON naming (camelCase)

Use camelCase property names in JSON: `groupBy`, `timeBucket`, `orderBy`, `isShared`, etc.

### Aggregation `fn` values

JSON key is **`fn`** (e.g. `"fn": "count"`). Allowed values align with `aggregationFunctions` from the schema.

### Example: minimal table (no aggregates)

```json
{
  "entity": "visitor_counts",
  "joins": [],
  "select": [
    { "field": "visitor_counts.count_date", "alias": "day" },
    { "field": "visitor_counts.count", "alias": "visitors" }
  ],
  "filters": [
    { "field": "visitor_counts.count_date", "op": "gte", "value": "2025-01-01" }
  ],
  "aggregations": [],
  "groupBy": [],
  "having": [],
  "orderBy": [{ "field": "day", "dir": "asc" }],
  "limit": 100,
  "offset": 0
}
```

### Example: loans by audience + biblio (joins)

```json
{
  "entity": "loans",
  "joins": ["users.public_types", "items.biblios"],
  "select": [
    { "field": "public_types.label", "alias": "audience" },
    { "field": "biblios.media_type", "alias": "media" }
  ],
  "filters": [
    { "field": "loans.date", "op": "gte", "value": "2025-01-01T00:00:00Z" }
  ],
  "aggregations": [
    { "fn": "count", "field": "loans.id", "alias": "totalLoans" }
  ],
  "groupBy": [
    { "field": "public_types.label", "alias": "audience" },
    { "field": "biblios.media_type", "alias": "media" }
  ],
  "having": [],
  "orderBy": [{ "field": "totalLoans", "dir": "desc" }],
  "limit": 50,
  "offset": 0
}
```

**UX guidance**

1. **Step 1 — Entity:** pick `entity` from `schema.entities`.
2. **Step 2 — Joins:** multi-select paths; only offer relations from `schema.entities[entity].relations` and chained relations from the schema.
3. **Step 3 — Dimensions / measures:** add `select` or `aggregations` (or `timeBucket` for time series).
4. **Step 4 — Filters:** bind `field` to allowed fields, `op` to `operators`, `value` type according to field type (arrays for `in` / `notIn`).
5. **Step 5 — Grouping:** if there are aggregates, require `groupBy` for every non-aggregated selected dimension (mirror SQL rules).
6. **Step 6 — Sort & page:** `orderBy.field` must match a **output** alias; use `limit`/`offset` and display `totalRows` from the response.

## 3) Execute — `POST /stats/query`

- **Request body:** the same object as above (`StatsBuilderBody`).
- **Response:** `StatsTableResponse` (camelCase):

| Field | Meaning |
|--------|---------|
| `columns` | `{ name, label, dataType }` per column |
| `rows` | Array of objects (dynamic keys = column aliases / names) |
| `totalRows` | Total rows matching the query **before** `limit`/`offset` |
| `limit` / `offset` | Echo of effective pagination |

**Frontend responsibility:** render a **data grid** (sort indicators can be client-only or align with `orderBy`). Optionally add charts (categories from dimension columns, measures from numeric columns).

**Caching:** identical bodies may hit a **Redis cache** (5 minutes TTL); users may see slightly stale counts for heavy dashboards.

**Rate limiting:** these routes share a **stricter** limit than generic API traffic; avoid polling `POST /stats/query` in a tight loop.

## 4) Saved queries — persist and reuse

### List — `GET /stats/saved`

Returns `SavedStatsQuery[]`:

- `id`, `name`, `description`, `query` (full `StatsBuilderBody`), `userId`, `isShared`, `createdAt`, `updatedAt`.

### Create — `POST /stats/saved`

Body (`SavedStatsQueryWrite`):

```json
{
  "name": "Loans by audience 2025",
  "description": "Optional",
  "query": { "...": "same shape as POST /stats/query" },
  "isShared": false
}
```

### Update — `PUT /stats/saved/{id}`

Same body as create. Owner or admin only.

### Delete — `DELETE /stats/saved/{id}`

Returns `{ "ok": true }`. Owner or admin only.

### Run — `GET /stats/saved/{id}/run`

Executes the stored `query` and returns the same **`StatsTableResponse`** as `POST /stats/query`. Caller must be allowed to read the saved row (owner, or `isShared`, or admin).

## 5) Suggested UI structure (Statistics area)

1. **“Explorer” tab**  
   - Load schema → wizard / advanced form to build `StatsBuilderBody`.  
   - `POST /stats/query` → preview table + optional chart.  
   - Actions: **Save** (opens modal → `POST /stats/saved`), **Export CSV** (client-side from `rows`).

2. **“Saved reports” tab**  
   - `GET /stats/saved` → list (name, description, owner, shared flag, updated date).  
   - Row actions: **Run** (`GET .../run`), **Edit** (`PUT`), **Delete** (`DELETE`).  
   - “Edit” should reopen the builder with `query` pre-filled.

3. **Empty / error states**  
   - Show server messages for validation (`400`) and permission (`403`).  
   - On timeout (`500` / internal), suggest narrowing date range or filters.

## 6) Testing checklist

- [ ] `GET /stats/schema` populates all controls without hardcoded table names.  
- [ ] Builder produces valid JSON (camelCase) and matches OpenAPI.  
- [ ] Saved list shows shared queries to all staff; private only to owner.  
- [ ] Run saved query matches running the same body via `POST /stats/query`.  
- [ ] Pagination: `totalRows` vs current page; `offset` increases by `limit`.  
- [ ] `orderBy` references aliases returned in `columns`.

## 7) Default templates (migrations 002–003)

After `sqlx migrate run`, the DB may contain **shared** (`isShared: true`) saved queries with English names such as **Registrations — …** / **Loans — …** (by audience, month, city, account type; loans by month, media/audience cross-tab, unique borrowers, archived loans, average renewals). Inserts use the first `users.id` and only run if `users` is non-empty.

Reference ODT/PDF exports are often **screenshots only** (no extractable text in `content.xml`); templates are heuristic matches to common LMS indicators, not pixel-perfect copies.

## 8) Reference

- Design: `docs/elidune-stats-api-design.md`  
- OpenAPI: `/api/docs` (Swagger UI) — types `StatsBuilderBody`, `StatsTableResponse`, `SavedStatsQuery`.
