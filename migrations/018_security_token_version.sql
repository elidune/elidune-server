-- Phase 2 security: invalidate outstanding JWTs when credentials or role change.
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS token_version BIGINT NOT NULL DEFAULT 0;
