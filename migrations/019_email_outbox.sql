-- Reliable email delivery queue. Producers call EmailService::enqueue; the scheduler
-- drains pending rows via services::email_outbox::process_outbox_batch.

CREATE TABLE IF NOT EXISTS email_outbox (
    id          BIGINT       PRIMARY KEY,
    to_addr     TEXT         NOT NULL,
    subject     TEXT         NOT NULL,
    body        TEXT         NOT NULL,
    status      VARCHAR(16)  NOT NULL DEFAULT 'pending',
    attempts    INTEGER      NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    sent_at     TIMESTAMPTZ,
    CONSTRAINT email_outbox_status_chk CHECK (status IN ('pending', 'sent', 'failed'))
);

CREATE INDEX IF NOT EXISTS email_outbox_pending_created_idx
    ON email_outbox (created_at ASC)
    WHERE status = 'pending';

COMMENT ON TABLE  email_outbox           IS 'Outbound email queue processed by the background scheduler.';
COMMENT ON COLUMN email_outbox.body      IS 'JSON payload: {"plain":"...","html":"..."} for multipart send.';
COMMENT ON COLUMN email_outbox.status    IS 'pending → sent, or failed after max delivery attempts.';
COMMENT ON COLUMN email_outbox.attempts  IS 'Number of send attempts (incremented on transient failure).';
