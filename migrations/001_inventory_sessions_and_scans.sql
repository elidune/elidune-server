-- Stocktaking sessions and per-session barcode scans (see repository/inventory.rs)

CREATE TABLE inventory_sessions (
    id                BIGINT       PRIMARY KEY,
    name              TEXT         NOT NULL,
    started_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    closed_at         TIMESTAMPTZ,
    status            VARCHAR(16)  NOT NULL DEFAULT 'open',
    location_filter   TEXT,
    notes             TEXT,
    created_by        BIGINT       REFERENCES users(id) ON DELETE SET NULL,
    CONSTRAINT inventory_sessions_status_chk CHECK (status IN ('open', 'closed'))
);

CREATE INDEX idx_inventory_sessions_started_at ON inventory_sessions (started_at DESC);
CREATE INDEX idx_inventory_sessions_status ON inventory_sessions (status);

CREATE TABLE inventory_scans (
    id          BIGSERIAL    PRIMARY KEY,
    session_id  BIGINT       NOT NULL REFERENCES inventory_sessions(id) ON DELETE CASCADE,
    item_id     BIGINT       REFERENCES items(id) ON DELETE SET NULL,
    barcode     VARCHAR(100) NOT NULL,
    scanned_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    result      VARCHAR(32)  NOT NULL,
    CONSTRAINT inventory_scans_result_chk CHECK (result IN ('found', 'unknown_barcode'))
);

CREATE INDEX idx_inventory_scans_session ON inventory_scans (session_id);
CREATE INDEX idx_inventory_scans_session_item ON inventory_scans (session_id, item_id);
