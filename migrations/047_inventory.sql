-- =============================================================================
-- Migration 047: Inventory / Stocktaking sessions
-- =============================================================================
-- An inventory session captures which specimens were physically scanned.
-- Discrepancy reports compare scanned vs expected.

CREATE TABLE IF NOT EXISTS inventory_sessions (
    id              BIGINT PRIMARY KEY,
    name            VARCHAR(255) NOT NULL,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    closed_at       TIMESTAMPTZ,
    -- open | closed
    status          VARCHAR(20) NOT NULL DEFAULT 'open',
    location_filter VARCHAR(255),       -- optional: scope to a shelf/location
    notes           TEXT,
    created_by      BIGINT REFERENCES users(id)
);

CREATE TABLE IF NOT EXISTS inventory_scans (
    id              BIGSERIAL PRIMARY KEY,
    session_id      BIGINT NOT NULL REFERENCES inventory_sessions(id) ON DELETE CASCADE,
    specimen_id     BIGINT REFERENCES specimens(id) ON DELETE SET NULL,
    barcode         VARCHAR(100) NOT NULL,
    scanned_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- found | unknown_barcode
    result          VARCHAR(30) NOT NULL DEFAULT 'found'
);

CREATE INDEX IF NOT EXISTS idx_inventory_scans_session ON inventory_scans(session_id);
CREATE INDEX IF NOT EXISTS idx_inventory_scans_barcode ON inventory_scans(barcode);
