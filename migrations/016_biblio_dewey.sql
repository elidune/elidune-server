-- Add Dewey Decimal Classification to bibliographic records.
ALTER TABLE biblios ADD COLUMN IF NOT EXISTS dewey VARCHAR(100);

-- Backfill from stored MARC JSON (camelCase: scheme = "dewey").
UPDATE biblios b
SET dewey = sub.number
FROM (
    SELECT DISTINCT ON (b2.id)
        b2.id,
        trim(cls->>'number') AS number
    FROM biblios b2
    CROSS JOIN LATERAL jsonb_array_elements(
        COALESCE(b2.marc_record->'indexing'->'classifications', '[]'::jsonb)
    ) WITH ORDINALITY AS t(cls, ordinality)
    WHERE b2.marc_record IS NOT NULL
      AND lower(cls->>'scheme') = 'dewey'
      AND nullif(trim(cls->>'number'), '') IS NOT NULL
    ORDER BY b2.id, ordinality
) sub
WHERE b.id = sub.id
  AND b.dewey IS NULL;

CREATE INDEX IF NOT EXISTS idx_biblio_dewey ON biblios(dewey) WHERE dewey IS NOT NULL;
