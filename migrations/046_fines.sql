-- =============================================================================
-- Migration 046: Fine / Penalty management
-- =============================================================================
-- Fines are accrued automatically for overdue loans and can be paid manually.
-- fine_rules stores per-media-type daily rates.

CREATE TABLE IF NOT EXISTS fine_rules (
    id              BIGSERIAL PRIMARY KEY,
    media_type      VARCHAR(50),        -- NULL = default for all types
    daily_rate      NUMERIC(10,2) NOT NULL DEFAULT 0.10,
    max_amount      NUMERIC(10,2),      -- cap per loan (NULL = no cap)
    grace_days      INT NOT NULL DEFAULT 0,
    notes           TEXT
);

-- Default fine rule (applies to all media types unless overridden)
INSERT INTO fine_rules (media_type, daily_rate, max_amount, grace_days)
VALUES (NULL, 0.10, 10.00, 0)
ON CONFLICT DO NOTHING;

CREATE TABLE IF NOT EXISTS fines (
    id              BIGINT PRIMARY KEY,
    loan_id         BIGINT NOT NULL REFERENCES loans(id) ON DELETE CASCADE,
    user_id         BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    amount          NUMERIC(10,2) NOT NULL,
    paid_amount     NUMERIC(10,2) NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    paid_at         TIMESTAMPTZ,
    -- pending | partial | paid | waived
    status          VARCHAR(20) NOT NULL DEFAULT 'pending',
    notes           TEXT
);

CREATE INDEX IF NOT EXISTS idx_fines_user ON fines(user_id);
CREATE INDEX IF NOT EXISTS idx_fines_loan ON fines(loan_id);
CREATE INDEX IF NOT EXISTS idx_fines_status ON fines(status);
