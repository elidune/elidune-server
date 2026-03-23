-- 1. Enforce NOT NULL on series.name (orphaned rows were removed in migration 044)
ALTER TABLE series ALTER COLUMN name SET NOT NULL;

-- 2. Rename primary_title → name in collections (mirrors series.name as the main display field)
ALTER TABLE collections RENAME COLUMN primary_title TO name;

-- Fill any remaining NULLs before adding the constraint
UPDATE collections
SET name = COALESCE(key, 'Collection ' || id::text)
WHERE name IS NULL;

ALTER TABLE collections ALTER COLUMN name SET NOT NULL;

-- 3. N:M junction between biblios and collections (mirrors biblio_series)
CREATE TABLE biblio_collections (
    id              BIGSERIAL   PRIMARY KEY,
    biblio_id       BIGINT      NOT NULL REFERENCES biblios(id)     ON DELETE CASCADE,
    collection_id   BIGINT      NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    position        SMALLINT    NOT NULL DEFAULT 1,
    volume_number   SMALLINT,
    UNIQUE (biblio_id, collection_id)
);

CREATE INDEX idx_biblio_collections_biblio     ON biblio_collections(biblio_id);
CREATE INDEX idx_biblio_collections_collection ON biblio_collections(collection_id);

-- 4. Back-fill from the existing 1:1 column on biblios
INSERT INTO biblio_collections (biblio_id, collection_id, position, volume_number)
SELECT id, collection_id, 1, collection_volume_number
FROM   biblios
WHERE  collection_id IS NOT NULL;

-- 5. Drop the now-redundant columns from biblios
ALTER TABLE biblios DROP COLUMN collection_id;
ALTER TABLE biblios DROP COLUMN collection_sequence_number;
ALTER TABLE biblios DROP COLUMN collection_volume_number;
