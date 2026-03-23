-- Remove biblio↔series links where the series has no display name (orphaned / invalid rows).
DELETE FROM biblio_series
WHERE series_id IN (SELECT id FROM series WHERE name IS NULL);
