-- Track catalog consolidation after a closed inventory session.

ALTER TABLE inventory_sessions
    ADD COLUMN IF NOT EXISTS consolidated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS consolidated_by BIGINT REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_inventory_sessions_consolidated_at
    ON inventory_sessions (consolidated_at)
    WHERE consolidated_at IS NOT NULL;
