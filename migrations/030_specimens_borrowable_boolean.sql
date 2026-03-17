-- =============================================================================
-- Migration 030: Specimens borrowable boolean
-- =============================================================================
-- Purpose: Replace legacy borrow_status (98/110) with borrowable boolean.
-- Data migration: 98 => true, 110 => false (NULL keeps default true).
-- =============================================================================

ALTER TABLE specimens
    ADD COLUMN IF NOT EXISTS borrowable BOOLEAN NOT NULL DEFAULT TRUE;

UPDATE specimens
SET borrowable = CASE
    WHEN borrow_status = 98 THEN TRUE
    WHEN borrow_status = 110 THEN FALSE
    ELSE borrowable
END
WHERE borrow_status IS NOT NULL;

ALTER TABLE specimens
    DROP COLUMN IF EXISTS borrow_status;

