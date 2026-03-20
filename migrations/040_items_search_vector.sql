-- =============================================================================
-- Migration 040: Full-Text Search vector for items
-- =============================================================================
-- Adds an accent-insensitive FTS configuration, a materialized search_vector
-- column on items (tsvector), a rebuild function that aggregates text from all
-- related tables (authors, editions, specimens, series, collections), and a
-- BEFORE INSERT/UPDATE trigger to keep it current.
-- =============================================================================

-- =============================================================================
-- 1. FTS CONFIGURATION: simple_unaccent
-- =============================================================================
-- Uses the 'simple' dictionary (no stemming, safe for multilingual catalogs)
-- combined with the 'unaccent' filter so accented characters are normalised.

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_ts_config WHERE cfgname = 'simple_unaccent') THEN
        CREATE TEXT SEARCH CONFIGURATION simple_unaccent (COPY = simple);
        ALTER TEXT SEARCH CONFIGURATION simple_unaccent
            ALTER MAPPING FOR hword, hword_part, word
            WITH unaccent, simple;
    END IF;
END $$;

-- =============================================================================
-- 2. ADD search_vector COLUMN
-- =============================================================================

ALTER TABLE items ADD COLUMN IF NOT EXISTS search_vector tsvector;

-- =============================================================================
-- 3. REBUILD FUNCTION
-- =============================================================================
-- Weights:
--   A (1.0) : title, author lastname/firstname
--   B (0.4) : subject, keywords (array), isbn
--   C (0.2) : publisher_name, series name, collection primary_title, barcode, call_number
--   D (0.1) : abstract, notes, table_of_contents, accompanying_material

CREATE OR REPLACE FUNCTION items_rebuild_search_vector(p_item_id BIGINT)
RETURNS tsvector
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    v_title          TEXT;
    v_subject        TEXT;
    v_keywords       TEXT;
    v_isbn           TEXT;
    v_abstract       TEXT;
    v_notes          TEXT;
    v_toc            TEXT;
    v_accompanying   TEXT;
    v_authors        TEXT;
    v_publisher      TEXT;
    v_series_name    TEXT;
    v_coll_title     TEXT;
    v_barcodes       TEXT;
    v_call_numbers   TEXT;
    v_vec            tsvector := ''::tsvector;
BEGIN
    -- Fields from items itself
    SELECT
        coalesce(title, ''),
        coalesce(subject, ''),
        coalesce(array_to_string(keywords, ' '), ''),
        coalesce(isbn, ''),
        coalesce(abstract, ''),
        coalesce(notes, ''),
        coalesce(table_of_contents, ''),
        coalesce(accompanying_material, '')
    INTO
        v_title, v_subject, v_keywords, v_isbn,
        v_abstract, v_notes, v_toc, v_accompanying
    FROM items WHERE id = p_item_id;

    IF NOT FOUND THEN
        RETURN ''::tsvector;
    END IF;

    -- Authors (all linked authors concatenated)
    SELECT string_agg(
               coalesce(a.lastname, '') || ' ' || coalesce(a.firstname, ''),
               ' '
           )
    INTO v_authors
    FROM item_authors ia
    JOIN authors a ON a.id = ia.author_id
    WHERE ia.item_id = p_item_id;

    -- Publisher (edition)
    SELECT coalesce(e.publisher_name, '')
    INTO v_publisher
    FROM items i
    LEFT JOIN editions e ON e.id = i.edition_id
    WHERE i.id = p_item_id;

    -- Series name
    SELECT coalesce(s.name, '')
    INTO v_series_name
    FROM items i
    LEFT JOIN series s ON s.id = i.series_id
    WHERE i.id = p_item_id;

    -- Collection primary title
    SELECT coalesce(c.primary_title, '')
    INTO v_coll_title
    FROM items i
    LEFT JOIN collections c ON c.id = i.collection_id
    WHERE i.id = p_item_id;

    -- Specimen barcodes and call numbers (active only)
    SELECT
        coalesce(string_agg(DISTINCT sp.barcode, ' '), ''),
        coalesce(string_agg(DISTINCT sp.call_number, ' '), '')
    INTO v_barcodes, v_call_numbers
    FROM specimens sp
    WHERE sp.item_id = p_item_id AND sp.archived_at IS NULL;

    -- Build weighted vector
    -- A: title + authors
    IF v_title <> '' OR coalesce(v_authors, '') <> '' THEN
        v_vec := v_vec ||
            setweight(to_tsvector('simple_unaccent',
                trim(coalesce(v_title, '') || ' ' || coalesce(v_authors, ''))
            ), 'A');
    END IF;

    -- B: subject + keywords + isbn
    IF v_subject <> '' OR v_keywords <> '' OR v_isbn <> '' THEN
        v_vec := v_vec ||
            setweight(to_tsvector('simple_unaccent',
                trim(v_subject || ' ' || v_keywords || ' ' || v_isbn)
            ), 'B');
    END IF;

    -- C: publisher + series + collection + barcode + call_number
    DECLARE
        v_c TEXT := trim(
            coalesce(v_publisher, '') || ' ' ||
            coalesce(v_series_name, '') || ' ' ||
            coalesce(v_coll_title, '') || ' ' ||
            coalesce(v_barcodes, '') || ' ' ||
            coalesce(v_call_numbers, '')
        );
    BEGIN
        IF v_c <> '' THEN
            v_vec := v_vec || setweight(to_tsvector('simple_unaccent', v_c), 'C');
        END IF;
    END;

    -- D: abstract + notes + toc + accompanying
    DECLARE
        v_d TEXT := trim(v_abstract || ' ' || v_notes || ' ' || v_toc || ' ' || v_accompanying);
    BEGIN
        IF v_d <> '' THEN
            v_vec := v_vec || setweight(to_tsvector('simple_unaccent', v_d), 'D');
        END IF;
    END;

    RETURN v_vec;
END;
$$;

-- =============================================================================
-- 4. TRIGGER: keep search_vector current on UPDATE of text columns
-- =============================================================================
-- Not on INSERT: the row has no linked authors/specimens yet during INSERT;
-- the Rust code explicitly rebuilds the vector after creating relations.

CREATE OR REPLACE FUNCTION items_search_vector_trigger_fn()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.search_vector := items_rebuild_search_vector(NEW.id);
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS items_search_vector_trigger ON items;

CREATE TRIGGER items_search_vector_trigger
    BEFORE UPDATE OF
        title, subject, keywords, isbn, abstract,
        notes, table_of_contents, accompanying_material,
        edition_id, series_id, collection_id
    ON items
    FOR EACH ROW
    EXECUTE FUNCTION items_search_vector_trigger_fn();

-- =============================================================================
-- 5. GIN INDEX
-- =============================================================================

CREATE INDEX IF NOT EXISTS idx_items_search_vector ON items USING GIN (search_vector);

-- =============================================================================
-- 6. BACKFILL ALL EXISTING ROWS
-- =============================================================================

UPDATE items SET search_vector = items_rebuild_search_vector(id);
 