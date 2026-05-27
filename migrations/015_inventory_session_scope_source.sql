-- Inventory session scope by catalog source (physical copy provenance on items.source_id).
-- Also allow scan result for copies found outside session scope.

ALTER TABLE inventory_sessions
    ADD COLUMN IF NOT EXISTS scope_source_id BIGINT REFERENCES sources(id) ON DELETE SET NULL;

COMMENT ON COLUMN inventory_sessions.scope_source_id IS
    'When set, expected counts, missing list, scans, and consolidation only include active items with this source_id.';

-- biblios.source_id is legacy / unused by application code; provenance lives on items.source_id.
COMMENT ON COLUMN biblios.source_id IS
    'Legacy column — not used by the application. Catalog source is stored on items.source_id per physical copy.';

ALTER TABLE inventory_scans
    DROP CONSTRAINT IF EXISTS inventory_scans_result_chk;

ALTER TABLE inventory_scans
    ADD CONSTRAINT inventory_scans_result_chk CHECK (
        result IN ('found', 'unknown_barcode', 'found_archived', 'found_out_of_scope')
    );

CREATE INDEX IF NOT EXISTS idx_items_source_active
    ON items (source_id)
    WHERE archived_at IS NULL;

-- Legacy data may have several open sessions with the same scope (e.g. multiple global sessions).
-- Keep the newest open session per scope; close the rest before adding the unique index.
WITH ranked AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY COALESCE(scope_source_id, -1::bigint),
                            COALESCE(scope_place, -1::smallint)
               ORDER BY started_at DESC, id DESC
           ) AS rn
    FROM inventory_sessions
    WHERE status = 'open'
)
UPDATE inventory_sessions inv
SET status = 'closed',
    closed_at = COALESCE(inv.closed_at, NOW())
FROM ranked r
WHERE inv.id = r.id
  AND r.rn > 1;

-- Prevent two open inventory sessions on the same scope (source + place).
-- NULL scope dimensions are normalized so only one global open session is allowed.
CREATE UNIQUE INDEX IF NOT EXISTS idx_inventory_sessions_open_scope
    ON inventory_sessions (
        COALESCE(scope_source_id, -1::bigint),
        COALESCE(scope_place, -1::smallint)
    )
    WHERE status = 'open';
