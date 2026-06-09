-- Links pending overdue-reminder outbox rows to loan IDs so:
-- 1) loans are excluded from the next reminder batch while delivery is pending;
-- 2) reminder tracking columns update only after SMTP success (status = 'sent').

CREATE TABLE IF NOT EXISTS email_outbox_reminder_loans (
    outbox_id BIGINT NOT NULL REFERENCES email_outbox (id) ON DELETE CASCADE,
    loan_id   BIGINT NOT NULL REFERENCES loans (id) ON DELETE CASCADE,
    PRIMARY KEY (loan_id)
);

CREATE INDEX IF NOT EXISTS email_outbox_reminder_loans_outbox_idx
    ON email_outbox_reminder_loans (outbox_id);

COMMENT ON TABLE email_outbox_reminder_loans IS
    'Loans reserved by a pending overdue-reminder outbox row; released on sent or permanent failure.';
