# Elidune Server — AI Agent Guide

## Project Overview

**Elidune** is a library management system (LMS) REST API server written in Rust.  
Stack: Axum · SQLx (PostgreSQL) · Redis · Meilisearch · Z39.50 · JWT auth · utoipa (OpenAPI).

**Entry point:** `src/main.rs` — loads config, initialises DB pool, Redis, services, and Axum router.  
**Crate root:** `src/lib.rs` — exports `AppState`, `AppConfig`, `DynamicConfig`, `AppError`, `AppResult`.

---

## Global
- always refer to the library management best practice
- for every move propose some enhancement

## Architecture

```
src/
  api/          # Axum handlers (HTTP layer only — no business logic)
  services/     # Business logic, one service struct per domain
  repository/   # SQL queries via sqlx (raw SQL, no ORM)
  models/       # Serde/SQLx data types (structs & enums)
  marc/         # MARC21 record translator (z3950-rs → internal models)
  config.rs     # Static config loaded from file (AppConfig)
  dynamic_config.rs  # Runtime-overridable settings from DB
  error.rs      # AppError enum + ErrorCode + IntoResponse
```

**Request flow:** `api/` handler → `services/` business logic → `repository/` SQL → PostgreSQL.

---

## Key Domain Concepts

| Concept | Description |
|---|---|
| **Item** | Bibliographic record (book, DVD, etc.) with ISBN, authors, series, etc. |
| **Specimen** | Physical copy of an item (barcode, call number, borrowable flag) |
| **Loan** | Borrowing of a specimen by a user |
| **Source** | Z39.50 or external catalog source for importing records |
| **PublicType** | Audience classification for items (e.g. youth, adult) |
| **MARC** | `src/marc/translator.rs` converts `z3950_rs::MarcRecord` → `Item + Vec<Specimen>` |

---

## Error Handling

All errors go through `AppError` in `src/error.rs`. Match the right variant:

```rust
AppError::NotFound("item not found".into())      // → 404
AppError::Validation("isbn invalid".into())       // → 400
AppError::Conflict("duplicate entry".into())      // → 409
AppError::Authorization("admin only".into())      // → 403
AppError::Internal("unexpected state".into())     // → 500
AppError::BusinessRule("max loans reached".into())// → 422
```

Special variants for UI confirmation flows:
- `AppError::DuplicateNeedsConfirmation` — returns 409 with `DuplicateConfirmationRequired` body
- `AppError::DuplicateBarcodeNeedsConfirmation` — same pattern for specimen barcodes

---

## Coding Conventions

- All code and comments in **English**.
- `serde(rename_all = "camelCase")` on all public-facing structs/enums.
- Enums that map to DB strings implement `sqlx::Type`, `sqlx::Encode`, `sqlx::Decode` manually (see `Language` in `src/models/mod.rs` as the canonical example).
- IDs are `i64` (Snowflake-generated via `snowflaked` crate). Primary/foreign keys are `BIGINT` in the DB.
- Use `AppResult<T>` (`Result<T, AppError>`) as function return type.
- Avoid `.unwrap()` — use `?` or explicit error mapping.
- OpenAPI annotations via `utoipa`: `#[utoipa::path(...)]` on handlers, `#[derive(ToSchema)]` on models.
- Use traits for behaviour boundaries. Prefer generics for hot paths, `dyn Trait` for heterogeneous/runtime dispatch.
- Derive `Default` when all fields have sensible defaults.
- Use concrete types (`struct`/`enum`) over `serde_json::Value` wherever shape is known.
- **Match on types, never strings.** Only convert to strings at serialization/display boundaries.
- Prefer `From`/`Into`/`TryFrom`/`TryInto` over manual conversions. Ask before adding manual conversion paths.
- Prefer streaming over non-streaming API calls.
- Run independent async work concurrently (`tokio::join!`, `futures::join_all`).
- Never use `block_on` inside async context.
- **Forbidden:** `Mutex<()>` / `Arc<Mutex<()>>` — mutex must guard actual state.
- Use `anyhow::Result` for app errors, `thiserror` for library errors. Propagate with `?`.
- **Never `.unwrap()`/`.expect()` in production.** Workspace lints deny these. Use `?`, `ok_or_else`, `unwrap_or_default`, `unwrap_or_else(|e| e.into_inner())` for locks.
- Use `time` crate (workspace dep) for date/time — no manual epoch math or magic constants like `86400`.
- Prefer `chrono` only if already imported in the crate; default to `time` for new code.
- Prefer crates over subprocesses (`std::process::Command`). Use subprocesses only when no mature crate exists.
- Prefer guard clauses (early returns) over nested `if` blocks.
- Prefer iterators/combinators over manual loops. Use `Cow<'_, str>` when allocation is conditional.
- Keep public API surfaces small. Use `#[must_use]` where return values matter.
  
---

## Database & Migrations

- Migrations live in `migrations/` as numbered SQL files (`NNN_description.sql`).
- Run via SQLx CLI: `sqlx migrate run`.
- Never modify an existing migration; always add a new numbered file.
- always update the `scripts/migrate_data.py` script when adding db migration
- keep the `scripts/init_database.py` up to date
---

## Configuration

Config loaded from a TOML/YAML file passed as `--config <path>`.  
Key sections in `AppConfig`: `server`, `database`, `users` (JWT), `logging`, `email`, `redis`, `meilisearch`.

Dynamic (DB-overridable) settings are in `DynamicConfig` / `dynamic_config.rs`.

---

## Services

`Services` struct in `src/services/mod.rs` holds all service instances, created once at startup and shared via `Arc<Services>` in `AppState`.

| Service | Responsibility |
|---|---|
| `catalog` | Item/specimen CRUD, search, import (with optional Meilisearch) |
| `loans` | Borrow/return flow, loan rules |
| `users` | Auth (JWT + TOTP), user management |
| `marc` | Z39.50 import pipeline (fetch → translate → catalog) |
| `z3950` | Z39.50 protocol client with Redis caching |
| `search` | Meilisearch index sync (optional; falls back to PostgreSQL FTS) |
| `stats` | Reporting and statistics queries |
| `reminders` | Scheduled loan reminder emails |
| `audit` | Audit trail for sensitive operations |
| `settings` | Borrowing rules and library settings |
| `scheduler` | Background task scheduler (woken via `AppState::scheduler_notify`) |

---

## Development Commands

```bash
# Build
rtk cargo build

# Check (fast, no codegen)
rtk cargo check

# Run tests
rtk cargo test

# Clippy
rtk cargo clippy

# Apply migrations (requires DATABASE_URL)
sqlx migrate run
```

---

## File Naming Patterns

| Pattern | Purpose |
|---|---|
| `src/api/<domain>.rs` | HTTP handlers for a domain |
| `src/services/<domain>.rs` | Business logic for a domain |
| `src/repository/<domain>.rs` | SQL queries for a domain |
| `src/models/<domain>.rs` | Data types for a domain |
| `migrations/NNN_description.sql` | Numbered sequential migrations |
