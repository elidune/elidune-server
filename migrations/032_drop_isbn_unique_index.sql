-- =============================================================================
-- Migration 032: Drop partial unique index on items.isbn
-- =============================================================================
-- Allows duplicate ISBNs among active items when explicitly forced by the user.
-- ISBN uniqueness is still enforced at the application level (409 confirmation),
-- but can be bypassed with the allow_duplicate_isbn flag.

DROP INDEX IF EXISTS idx_items_isbn_active_unique;
