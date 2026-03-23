-- =============================================================================
-- Migration 045: Reservation / Hold system
-- =============================================================================
-- Patrons can place a hold on an item when all specimens are borrowed.
-- Position in queue, notification tracking, and expiry are managed here.

CREATE TABLE IF NOT EXISTS reservations (
    id              BIGINT PRIMARY KEY,
    user_id         BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id         BIGINT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notified_at     TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ,
    -- pending | ready | fulfilled | cancelled | expired
    status          VARCHAR(20) NOT NULL DEFAULT 'pending',
    position        INT NOT NULL DEFAULT 1,
    notes           TEXT
);

CREATE INDEX IF NOT EXISTS idx_reservations_user ON reservations(user_id);
CREATE INDEX IF NOT EXISTS idx_reservations_item ON reservations(item_id);
CREATE INDEX IF NOT EXISTS idx_reservations_status ON reservations(status);
