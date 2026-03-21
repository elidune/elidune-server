-- Migration 041: Drop PostgreSQL FTS artifacts added by migration 040.
-- Meilisearch now handles full-text catalog search; the tsvector column,
-- trigger, SQL function, GIN index, and FTS configuration are no longer needed.

-- Drop trigger (fires on item UPDATE to rebuild search_vector)
DROP TRIGGER IF EXISTS items_search_vector_trigger ON items;

-- Drop trigger function
DROP FUNCTION IF EXISTS items_search_vector_trigger_fn() CASCADE;

-- Drop the SQL rebuild function used by the trigger and by explicit Rust calls
DROP FUNCTION IF EXISTS items_rebuild_search_vector(BIGINT) CASCADE;

-- Drop GIN index on search_vector
DROP INDEX IF EXISTS idx_items_search_vector;

-- Drop the search_vector column itself
ALTER TABLE items DROP COLUMN IF EXISTS search_vector;

-- Drop the custom FTS configuration (used by to_tsquery / tsvector)
DROP TEXT SEARCH CONFIGURATION IF EXISTS simple_unaccent;
