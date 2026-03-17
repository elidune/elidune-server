-- =============================================================================
-- Migration 024: Remove legacy columns from items
-- =============================================================================
-- Purpose: Drop obsolete columns no longer used by the Rust model:
-- - barcode
-- - call_number
-- - price
-- - nb_specimens
-- - state
-- - marc_format
-- - archived_timestamp
-- - status
-- =============================================================================

ALTER TABLE items
    DROP COLUMN IF EXISTS barcode,
    DROP COLUMN IF EXISTS call_number,
    DROP COLUMN IF EXISTS price,
    DROP COLUMN IF EXISTS nb_specimens,
    DROP COLUMN IF EXISTS state,
    DROP COLUMN IF EXISTS marc_format,
    DROP COLUMN IF EXISTS archived_timestamp,
    DROP COLUMN IF EXISTS status;

