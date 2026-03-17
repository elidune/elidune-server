-- =============================================================================
-- Migration 031: Partial unique index on items.isbn for active items
-- =============================================================================
-- Ensures no two active (non-archived) items share the same ISBN.
-- Archived items may keep their ISBN for historical reference.

-- First, archive older duplicates among active items (keep the one with most specimens, then newest id)
DO $$
DECLARE
  dup_count INT;
BEGIN
  SELECT COUNT(*) INTO dup_count
  FROM (
    SELECT isbn
    FROM items
    WHERE isbn IS NOT NULL AND archived_at IS NULL
    GROUP BY isbn
    HAVING COUNT(*) > 1
  ) dups;

  IF dup_count > 0 THEN
    RAISE NOTICE 'Found % duplicate active ISBNs — archiving older duplicates.', dup_count;

    WITH ranked AS (
      SELECT i.id,
             ROW_NUMBER() OVER (
               PARTITION BY i.isbn
               ORDER BY (SELECT COUNT(*) FROM specimens s WHERE s.item_id = i.id AND s.archived_at IS NULL) DESC,
                        i.id DESC
             ) AS rn
      FROM items i
      WHERE i.isbn IS NOT NULL AND i.archived_at IS NULL
    )
    UPDATE items SET archived_at = NOW(), updated_at = NOW()
    WHERE id IN (SELECT id FROM ranked WHERE rn > 1);
  END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_items_isbn_active_unique
  ON items (isbn) WHERE archived_at IS NULL AND isbn IS NOT NULL;
