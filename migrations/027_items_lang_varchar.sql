-- =============================================================================
-- Migration 027: Convert items.lang and items.lang_orig to VARCHAR
-- =============================================================================
-- Purpose:
--   - Migrate legacy integer language codes to ISO-like 3-letter codes.
--   - Keep column names `lang` and `lang_orig`, but change their type to VARCHAR(3).
--   - Mapping based on `language_code_to_id` in `src/marc/translator.rs`:
--       1 -> 'fre'
--       2 -> 'eng'
--       3 -> 'ger'   -- (also accepted input: 'deu')
--       4 -> 'jpn'
--       5 -> 'spa'
--       6 -> 'por'
--     All legacy 0 codes are mapped to 'fre' by default.
-- =============================================================================

-- Convert items.lang from integer code to VARCHAR(3)
ALTER TABLE items
    ALTER COLUMN lang TYPE VARCHAR(3)
    USING (
        CASE
            WHEN lang = 1 THEN 'fre'
            WHEN lang = 2 THEN 'eng'
            WHEN lang = 3 THEN 'ger'
            WHEN lang = 4 THEN 'jpn'
            WHEN lang = 5 THEN 'spa'
            WHEN lang = 6 THEN 'por'
            WHEN lang = 0 THEN 'fre'  -- default for legacy 0
            ELSE NULL
        END
    );

-- Convert items.lang_orig from integer code to VARCHAR(3)
ALTER TABLE items
    ALTER COLUMN lang_orig TYPE VARCHAR(3)
    USING (
        CASE
            WHEN lang_orig = 1 THEN 'fre'
            WHEN lang_orig = 2 THEN 'eng'
            WHEN lang_orig = 3 THEN 'ger'
            WHEN lang_orig = 4 THEN 'jpn'
            WHEN lang_orig = 5 THEN 'spa'
            WHEN lang_orig = 6 THEN 'por'
            WHEN lang_orig = 0 THEN 'fre'  -- default for legacy 0
            ELSE NULL
        END
    );

