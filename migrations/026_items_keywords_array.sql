-- ============================================================================
-- Migration 026: Change items.keywords to varchar[]
-- ============================================================================
-- Purpose: Store item keywords as an array of varchar instead of a single text
-- column, to better represent multiple distinct keywords.
-- ============================================================================

ALTER TABLE items
    ALTER COLUMN keywords TYPE varchar[]
    USING (
        CASE
            WHEN keywords IS NULL OR keywords = '' THEN NULL
            ELSE regexp_split_to_array(keywords, '\\s*,\\s*')::varchar[]
        END
    );

