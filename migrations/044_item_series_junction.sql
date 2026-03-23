-- =============================================================================
-- Migration 044: item_series junction table (N:M items <-> series)
-- =============================================================================
-- Replaces items.series_id + items.series_volume_number with a many-to-many
-- link so one item can belong to several series. Volume is stored per link.

CREATE TABLE IF NOT EXISTS item_series (
    id              BIGSERIAL PRIMARY KEY,
    item_id         BIGINT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    series_id       BIGINT NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    position        SMALLINT NOT NULL DEFAULT 1,
    volume_number   SMALLINT,
    UNIQUE (item_id, series_id)
);

CREATE INDEX IF NOT EXISTS idx_item_series_item ON item_series(item_id);
CREATE INDEX IF NOT EXISTS idx_item_series_series ON item_series(series_id);

-- Backfill from legacy single-series columns
INSERT INTO item_series (item_id, series_id, position, volume_number)
SELECT id, series_id, 1, series_volume_number
FROM items
WHERE series_id IS NOT NULL;

DROP INDEX IF EXISTS idx_items_series_vol;

ALTER TABLE items DROP COLUMN IF EXISTS series_id;
ALTER TABLE items DROP COLUMN IF EXISTS series_volume_number;
