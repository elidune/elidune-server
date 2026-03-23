-- Add must_change_password flag to users table.
-- When true, the user must change their password on next login.
-- Used for first-login enforcement (e.g. auto-created admin user).
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS must_change_password BOOLEAN NOT NULL DEFAULT FALSE;
