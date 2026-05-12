-- Audit log: record success/failure and HTTP/API error metadata for diagnostics.

ALTER TABLE audit_log
    ADD COLUMN IF NOT EXISTS outcome TEXT NOT NULL DEFAULT 'success',
    ADD COLUMN IF NOT EXISTS http_status SMALLINT,
    ADD COLUMN IF NOT EXISTS error_code TEXT,
    ADD COLUMN IF NOT EXISTS error_message TEXT;

ALTER TABLE audit_log DROP CONSTRAINT IF EXISTS audit_log_outcome_check;
ALTER TABLE audit_log
    ADD CONSTRAINT audit_log_outcome_check CHECK (outcome IN ('success', 'failure'));

CREATE INDEX IF NOT EXISTS audit_log_outcome_created_at_idx ON audit_log (outcome, created_at DESC);
