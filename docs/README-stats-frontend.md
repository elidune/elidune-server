# Stats API — frontend integration

This document summarizes API changes relevant to the **stats query builder** (`POST /stats/query`) and **patron sex** values.

## `POST /stats/query` body (`StatsBuilderBody`)

All keys use **camelCase** (serde).

### New: `filterGroups`

- **Type:** `StatsFilter[][]` (array of arrays).
- **Default:** `[]` (omit or empty = no OR-groups).
- **Semantics:** Top-level `filters` are combined with **AND**. If `filterGroups` is non-empty, the overall predicate is:

  `( AND of filters ) AND ( OR of ( AND of each inner array ) )`

  Example: `filters: [{…}]` and `filterGroups: [[A,B], [C]]` yields:

  `filter1 AND ((A AND B) OR (C))`

- **Limits (server validation):** up to 16 outer groups, up to 32 filters per inner group, 128 total filter clauses across `filters` + all groups.

### `unionWith` (multi-root / `UNION ALL`)

- **Type:** `string[]`, default `[]`.
- **Purpose:** Combine **`entity`** with additional root tables that share the same projected columns. Currently supported: **`loans`** + **`loans_archives`** (exactly both).
- **SQL:** The root `FROM` becomes a subquery `UNION ALL` with a discriminator column `__union_source` (`loans` | `loans_archives`). Joins are still resolved from the canonical **`entity`** (e.g. `users`, `items`, `items.biblios`).
- **Field:** `loans.union_source` (computed) maps to the branch label; **only** use when `unionWith` is set, otherwise the request is rejected.
- **Discovery:** `GET /stats/schema` includes `entities.loans.unionWith: ["loans_archives"]` and root-level `unionWithSemantics`.

Example:

```json
{
  "entity": "loans",
  "unionWith": ["loans_archives"],
  "joins": ["users"],
  "aggregations": [{ "fn": "count", "field": "loans.id", "alias": "n" }],
  "timeBucket": { "field": "loans.date", "granularity": "year", "alias": "y" }
}
```

### Existing: `filters`

Unchanged: flat list of `StatsFilter` (`field`, `op`, `value`), AND’d together.

### Computed dimensions (whitelist)

Some entities expose **computed** fields in `GET /stats/schema` (discovery JSON). They are **not** physical columns; the server expands safe SQL templates (no client SQL).

- In discovery, a field may include `"computed": true` and a `type` / `label` like physical fields.
- **Restrictions:**
  - **Cannot** be used in `aggregations` or `timeBucket` (use a physical date/numeric column instead).
  - **Can** be used in `select`, `groupBy`, and `filters` / `filterGroups`.

#### `users` entity

| Field        | Notes |
|-------------|--------|
| `sex`       | Physical column: `'m'`, `'f'`, or SQL `NULL` (JSON `null`). Type in schema: `text`. |
| `sex_label` | Computed: `male` / `female` / `unknown`. |
| `age_band`  | Computed from `birthdate` (`0-17`, `18-29`, …, or `NULL` if missing/invalid). |
| `age_band_3` | Computed: `0-14`, `15-64`, `65+`, or `NULL` (reporting bands). |
| `active_membership_calendar_year` | Computed: `yes` / `no` — `yes` if `expiry_at` is null or `>=` start of current calendar year (UTC). |

#### `items` / `sources`

- `items.source_id` (physical) joins to **`sources`** (`items.sources` path from `loans` or root `items`).
- `sources.name` — label for the catalog source of a copy.

Saved shared templates (migration `008` / init scripts) include patron cross-tabs and loans by year × media × `biblios.audience_type` × item source.

Discovery also includes `filterGroupsSemantics` explaining OR-of-AND behavior.

## Patron sex (`users` API)

Legacy numeric codes (**70 / 77 / 85**) are **removed**. The database stores:

- `'m'` — male  
- `'f'` — female  
- `NULL` — unknown / not set  

JSON serialization uses lowercase strings `"m"` and `"f"`; omit the field or send `null` for unknown.

**Frontend action:** update forms, filters, and any hard-coded mappings from old integers to `m` / `f` / `null`.

## `GET /stats/schema`

Response is JSON suitable for building a query UI: `entities`, `aggregationFunctions`, `operators`, `timeGranularities`, plus `filterGroupsSemantics` and per-field `computed` flags where applicable.
