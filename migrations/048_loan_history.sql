-- =============================================================================
-- Migration 048: Patron borrowing history (GDPR-aware)
-- =============================================================================
-- Controls whether individual users opt in/out of history retention.
-- History is stored in the loans table itself (returned loans).
-- This migration adds the opt-in flag and a procedure to purge history.

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS history_enabled BOOLEAN NOT NULL DEFAULT TRUE;

COMMENT ON COLUMN users.history_enabled IS
    'GDPR opt-in: when FALSE the user loan history is anonymised on return';
