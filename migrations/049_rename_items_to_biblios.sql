-- =============================================================================
-- Migration 049: Rename domain concepts
--   - bibliographic records: items → biblios
--   - physical copies: specimens → items
--   - junction tables: item_authors → biblio_authors, item_series → biblio_series
-- =============================================================================

-- Step 1: rename bibliographic records table
ALTER TABLE items RENAME TO biblios;

-- Step 2: rename specimens table to items (physical copies)
ALTER TABLE specimens RENAME TO items;

-- Step 3: rename FK column on physical items (was specimens.item_id → items.biblio_id)
ALTER TABLE items RENAME COLUMN item_id TO biblio_id;

-- Step 4: rename FK column on loans (was loans.specimen_id → loans.item_id)
ALTER TABLE loans RENAME COLUMN specimen_id TO item_id;

-- Step 5: rename junction tables
ALTER TABLE item_authors RENAME TO biblio_authors;
ALTER TABLE item_series  RENAME TO biblio_series;

-- Step 6: rename FK column on biblio_authors (was item_id → biblio_id)
ALTER TABLE biblio_authors RENAME COLUMN item_id TO biblio_id;

-- Step 7: rename FK column on biblio_series (was item_id → biblio_id)
ALTER TABLE biblio_series RENAME COLUMN item_id TO biblio_id;

-- Step 8: fix reservations — previously pointed to bibliographic records (biblios),
--         now must point to physical copies (items).
--         Truncate because the reservations table was newly created (migration 045)
--         and no production data exists yet.
ALTER TABLE reservations DROP CONSTRAINT IF EXISTS reservations_item_id_fkey;
TRUNCATE TABLE reservations;
ALTER TABLE reservations ADD CONSTRAINT reservations_item_id_fkey
    FOREIGN KEY (item_id) REFERENCES items(id) ON DELETE CASCADE;

-- Step 9: rename inventory_scans.specimen_id → item_id (now references items table)
ALTER TABLE inventory_scans DROP CONSTRAINT IF EXISTS inventory_scans_specimen_id_fkey;
ALTER TABLE inventory_scans RENAME COLUMN specimen_id TO item_id;
ALTER TABLE inventory_scans ADD CONSTRAINT inventory_scans_item_id_fkey
    FOREIGN KEY (item_id) REFERENCES items(id) ON DELETE SET NULL;

-- Step 10: rename or recreate indexes to reflect new table/column names
ALTER INDEX IF EXISTS idx_reservations_item RENAME TO idx_reservations_item_id;
