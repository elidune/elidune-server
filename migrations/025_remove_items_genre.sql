-- ============================================================================
-- Migration 025: Remove genre column from items
-- ============================================================================
-- Purpose: Drop the genre column which is no longer used by the Rust Item
-- model or repository layer.
-- ============================================================================

ALTER TABLE items
    DROP COLUMN IF EXISTS genre;

