--
-- PostgreSQL database dump
--

-- Dumped from database version 17.0
-- Dumped by pg_dump version 17.0

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: public; Type: SCHEMA; Schema: -; Owner: elidune
--

-- *not* creating schema, since initdb creates it


ALTER SCHEMA public OWNER TO elidune;

--
-- Name: SCHEMA public; Type: COMMENT; Schema: -; Owner: elidune
--

COMMENT ON SCHEMA public IS 'Standard public schema';


--
-- Name: insert_author(integer, character varying, character varying, character varying, character varying); Type: FUNCTION; Schema: public; Owner: elidune
--

CREATE FUNCTION public.insert_author(integer, character varying, character varying, character varying, character varying) RETURNS integer
    LANGUAGE plpgsql
    AS $_$
DECLARE
my_id ALIAS FOR $1;
my_lastname ALIAS FOR $2;
my_firstname ALIAS FOR $3;
my_bio ALIAS FOR $4;
my_notes ALIAS FOR $5;

author_id DECIMAL;
in_lastname TEXT;
in_firstname TEXT;
in_bio TEXT;
in_notes TEXT;
                                                                                            
BEGIN
                                                                  
IF my_id<>0 THEN
UPDATE authors SET lastname=my_lastname, firstname=my_firstname, notes=my_notes, bio=my_bio WHERE id=my_id;
return my_id;
ELSE
SELECT id, lastname, firstname, bio, notes INTO author_id, in_lastname, in_firstname, in_bio, in_notes FROM authors WHERE lastname=my_lastname AND firstname=my_firstname ;
                                                                                            
        IF NOT FOUND THEN
                SELECT nextval('authors_id_seq') INTO author_id;
                INSERT INTO authors (id, lastname, firstname, bio, notes) VALUES (author_id, my_lastname, my_firstname, my_bio, my_notes);                 
                return author_id;
        ELSE
                IF in_notes!=my_notes OR in_bio!=my_bio  THEN
                        UPDATE authors SET notes=my_notes, bio=my_bio WHERE id=author_id;
                END IF;
                                                                                            
        END IF;

return author_id;

END IF;
                                                                                            
END;
$_$;


ALTER FUNCTION public.insert_author(integer, character varying, character varying, character varying, character varying) OWNER TO elidune;

--
-- Name: insert_collection(integer, character varying, character varying, character varying); Type: FUNCTION; Schema: public; Owner: elidune
--

CREATE FUNCTION public.insert_collection(integer, character varying, character varying, character varying) RETURNS integer
    LANGUAGE plpgsql
    AS $_$
DECLARE
my_id ALIAS FOR $1;
my_title1 ALIAS FOR $2;
my_title2 ALIAS FOR $3;
my_title3 ALIAS FOR $4;


collection_id DECIMAL;
in_title1 TEXT;
in_title2 TEXT;
in_title3 TEXT;
                                                                                            
BEGIN
                                                                  
IF my_id<>0 THEN
UPDATE collections SET title1=my_title1, title2=my_title2, title3=my_title3 WHERE id=my_id;
return my_id;
ELSE
SELECT id, title1, title2, title3 INTO collection_id, in_title1, in_title2, in_title3 FROM collections WHERE title1=my_title1;
                                                                                            
        IF NOT FOUND THEN
                SELECT nextval('collections_id_seq') INTO collection_id;
                INSERT INTO collections (id, title1, title2, title3) VALUES (collection_id, my_title1, my_title2, my_title3);                     return collection_id;
        ELSE
                IF in_title1!=my_title1 OR in_title2!=my_title2 OR in_title3!=my_title3  THEN
                        UPDATE collections SET title1=my_title1, title2=my_title2, title3=my_title3 WHERE id=collection_id;
                END IF;
                                                  
        END IF;

return collection_id;

END IF;
                                                                                            
END;
$_$;


ALTER FUNCTION public.insert_collection(integer, character varying, character varying, character varying) OWNER TO elidune;

--
-- Name: insert_edition(integer, character varying, character varying); Type: FUNCTION; Schema: public; Owner: elidune
--

CREATE FUNCTION public.insert_edition(integer, character varying, character varying) RETURNS integer
    LANGUAGE plpgsql
    AS $_$
DECLARE
my_id ALIAS FOR $1;
my_name ALIAS FOR $2;
my_place ALIAS FOR $3;

edition_id DECIMAL;
in_name TEXT;
in_place TEXT;
                                                                                            
BEGIN
                                                                  
IF my_id<>0 THEN
UPDATE editions SET name=my_name, place=my_place WHERE id=my_id;
return my_id;
ELSE
SELECT id, name, place INTO edition_id, in_name, in_place FROM editions WHERE name=my_name;
                                                                                            
        IF NOT FOUND THEN
                SELECT nextval('editions_id_seq') INTO edition_id;
                INSERT INTO editions (id, name, place) VALUES (edition_id, my_name, my_place);                 
                return edition_id;
        ELSE
                IF in_place!=my_place THEN
                        UPDATE editions SET place=my_place WHERE id=edition_id;
                END IF;
                                                                                            
        END IF;

return edition_id;

END IF;
                                                                                            
END;
$_$;


ALTER FUNCTION public.insert_edition(integer, character varying, character varying) OWNER TO elidune;

--
-- Name: insert_serie(integer, character varying); Type: FUNCTION; Schema: public; Owner: elidune
--

CREATE FUNCTION public.insert_serie(integer, character varying) RETURNS integer
    LANGUAGE plpgsql
    AS $_$
DECLARE
my_id ALIAS FOR $1;
my_name ALIAS FOR $2;


serie_id DECIMAL;
in_name TEXT;

                                                                                            
BEGIN
                                                                  
IF my_id<>0 THEN
UPDATE series SET name=my_name WHERE id=my_id;
return my_id;
ELSE
SELECT id, name INTO serie_id, in_name FROM series WHERE name=my_name;
                                                                                            
        IF NOT FOUND THEN
                SELECT nextval('series_id_seq') INTO serie_id;
                INSERT INTO series (id, name) VALUES (serie_id, my_name);                 
                return serie_id;                                   
        END IF;

return serie_id;

END IF;
                                                                                            
END;
$_$;


ALTER FUNCTION public.insert_serie(integer, character varying) OWNER TO elidune;

--
-- Name: insert_source(integer, character varying); Type: FUNCTION; Schema: public; Owner: elidune
--

CREATE FUNCTION public.insert_source(integer, character varying) RETURNS integer
    LANGUAGE plpgsql
    AS $_$
DECLARE
my_id ALIAS FOR $1;
my_name ALIAS FOR $2;


source_id DECIMAL;
in_name TEXT;

                                                                                            
BEGIN
                                                                  
IF my_id<>0 THEN
UPDATE sources SET name=my_name WHERE id=my_id;
return my_id;
ELSE
SELECT id, name INTO source_id, in_name FROM sources WHERE name=my_name;
                                                                                            
        IF NOT FOUND THEN
                SELECT nextval('sources_id_seq') INTO source_id;
                INSERT INTO sources (id, name) VALUES (source_id, my_name);                 
                return source_id;                                   
        END IF;

return source_id;

END IF;
                                                                                            
END;
$_$;


ALTER FUNCTION public.insert_source(integer, character varying) OWNER TO elidune;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: account_types; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.account_types (
    id integer NOT NULL,
    name character varying,
    items_rights bpchar DEFAULT 'n'::bpchar,
    users_rights bpchar DEFAULT 'n'::bpchar,
    loans_rights bpchar DEFAULT 'n'::bpchar,
    items_archive_rights bpchar DEFAULT 'n'::bpchar,
    books_rights bpchar DEFAULT 'n'::bpchar,
    audios_rights bpchar DEFAULT 'n'::bpchar,
    borrows_rights "char",
    settings_right bpchar,
    settings_rights bpchar
);


ALTER TABLE public.account_types OWNER TO elidune;

--
-- Name: account_types_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.account_types_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.account_types_id_seq OWNER TO elidune;

--
-- Name: account_types_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.account_types_id_seq OWNED BY public.account_types.id;


--
-- Name: audios; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.audios (
    id integer NOT NULL,
    label character varying,
    lang smallint,
    title1 character varying,
    title2 character varying,
    editor_name character varying,
    editor_place character varying,
    editor_date character varying,
    description character varying,
    addon character varying,
    barcode character varying,
    cote character varying,
    dewey character varying,
    author1 character varying,
    author2 character varying,
    author3 character varying,
    tracks character varying,
    keywords character varying,
    notes character varying,
    public_type smallint,
    audio_type smallint,
    nb_specimens smallint,
    state character varying,
    source_name character varying,
    source_country character varying,
    source_date character varying,
    source_norme character varying
);


ALTER TABLE public.audios OWNER TO elidune;

--
-- Name: audios_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.audios_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.audios_id_seq OWNER TO elidune;

--
-- Name: audios_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.audios_id_seq OWNED BY public.audios.id;


--
-- Name: authors; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.authors (
    id integer NOT NULL,
    key character varying,
    lastname character varying,
    firstname character varying,
    bio character varying,
    notes character varying
);


ALTER TABLE public.authors OWNER TO elidune;

--
-- Name: authors_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.authors_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.authors_id_seq OWNER TO elidune;

--
-- Name: authors_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.authors_id_seq OWNED BY public.authors.id;


--
-- Name: books; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.books (
    id integer NOT NULL,
    isbn character varying,
    isbn_dispo character varying,
    issn character varying,
    issn_dispo character varying,
    cote character varying,
    barcode character varying,
    dewey character varying,
    publication_date character varying,
    lang smallint,
    lang_orig smallint,
    title1 character varying,
    title2 character varying,
    title3 character varying,
    title4 character varying,
    author1 character varying,
    author2 character varying,
    author3 character varying,
    serie_name character varying,
    serie_vol_number smallint,
    public_type smallint,
    editor_name character varying,
    editor_place character varying,
    editor_date character varying,
    nb_pages character varying,
    format character varying,
    addon character varying,
    illus_type smallint,
    litterature_type smallint,
    collection_title1 character varying,
    collection_title2 character varying,
    collection_title3 character varying,
    collection_number_sub smallint,
    collection_issn character varying,
    collection_vol_number smallint,
    abstract character varying,
    keywords character varying,
    nb_specimens smallint,
    state character varying,
    source_name character varying,
    source_country character varying,
    source_date character varying,
    source_norme character varying
);


ALTER TABLE public.books OWNER TO elidune;

--
-- Name: books_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.books_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.books_id_seq OWNER TO elidune;

--
-- Name: books_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.books_id_seq OWNED BY public.books.id;


--
-- Name: borrows; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.borrows (
    id integer NOT NULL,
    user_id integer NOT NULL,
    specimen_id integer NOT NULL,
    date integer NOT NULL,
    renew_date integer,
    nb_renews smallint,
    issue_date integer,
    notes character varying,
    returned_date integer,
    item_id integer
);


ALTER TABLE public.borrows OWNER TO elidune;

--
-- Name: borrows_archives; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.borrows_archives (
    id integer NOT NULL,
    item_id integer NOT NULL,
    date integer NOT NULL,
    nb_renews smallint,
    issue_date integer,
    returned_date integer,
    notes character varying,
    specimen_id integer,
    borrower_public_type integer,
    occupation character varying,
    addr_city character varying,
    sex_id smallint,
    account_type_id smallint
);


ALTER TABLE public.borrows_archives OWNER TO elidune;

--
-- Name: borrows_archives_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.borrows_archives_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.borrows_archives_id_seq OWNER TO elidune;

--
-- Name: borrows_archives_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.borrows_archives_id_seq OWNED BY public.borrows_archives.id;


--
-- Name: borrows_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.borrows_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.borrows_id_seq OWNER TO elidune;

--
-- Name: borrows_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.borrows_id_seq OWNED BY public.borrows.id;


--
-- Name: borrows_settings; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.borrows_settings (
    id integer NOT NULL,
    media_type character varying,
    nb_max smallint,
    nb_renews smallint,
    duration smallint,
    notes character varying,
    account_type_id smallint
);


ALTER TABLE public.borrows_settings OWNER TO elidune;

--
-- Name: borrows_settings_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.borrows_settings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.borrows_settings_id_seq OWNER TO elidune;

--
-- Name: borrows_settings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.borrows_settings_id_seq OWNED BY public.borrows_settings.id;


--
-- Name: collections; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.collections (
    id integer NOT NULL,
    key character varying,
    title1 character varying,
    title2 character varying,
    title3 character varying,
    issn character varying
);


ALTER TABLE public.collections OWNER TO elidune;

--
-- Name: collections_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.collections_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.collections_id_seq OWNER TO elidune;

--
-- Name: collections_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.collections_id_seq OWNED BY public.collections.id;


--
-- Name: editions; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.editions (
    id integer NOT NULL,
    key character varying,
    name character varying,
    place character varying,
    notes character varying
);


ALTER TABLE public.editions OWNER TO elidune;

--
-- Name: editions_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.editions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.editions_id_seq OWNER TO elidune;

--
-- Name: editions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.editions_id_seq OWNED BY public.editions.id;


--
-- Name: fees; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.fees (
    id integer NOT NULL,
    "desc" character varying,
    amount integer DEFAULT 0
);


ALTER TABLE public.fees OWNER TO elidune;

--
-- Name: fees_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.fees_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.fees_id_seq OWNER TO elidune;

--
-- Name: fees_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.fees_id_seq OWNED BY public.fees.id;


--
-- Name: items; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.items (
    id integer NOT NULL,
    media_type character varying,
    identification character varying,
    price character varying,
    barcode character varying,
    dewey character varying,
    publication_date character varying,
    lang smallint,
    lang_orig smallint,
    title1 character varying,
    title2 character varying,
    title3 character varying,
    title4 character varying,
    author1_ids integer[],
    author1_functions character varying,
    author2_ids integer[],
    author2_functions character varying,
    author3_ids integer[],
    author3_functions character varying,
    serie_id integer,
    serie_vol_number smallint,
    collection_id integer,
    collection_number_sub smallint,
    collection_vol_number smallint,
    source_id integer,
    source_date character varying,
    source_norme character varying,
    genre smallint,
    subject character varying,
    public_type smallint,
    edition_id integer,
    edition_date character varying,
    nb_pages character varying,
    format character varying,
    content character varying,
    addon character varying,
    abstract character varying,
    notes character varying,
    keywords character varying,
    nb_specimens smallint,
    state character varying,
    is_archive smallint DEFAULT (0)::smallint,
    archived_timestamp integer,
    is_valid smallint DEFAULT (0)::smallint,
    crea_date integer,
    modif_date integer,
    temp smallint
);


ALTER TABLE public.items OWNER TO elidune;

--
-- Name: items_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.items_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.items_id_seq OWNER TO elidune;

--
-- Name: items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.items_id_seq OWNED BY public.items.id;


--
-- Name: remote_books; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.remote_books (
    id integer NOT NULL,
    isbn character varying,
    isbn_dispo character varying,
    issn character varying,
    issn_dispo character varying,
    cote character varying,
    barcode character varying,
    dewey character varying,
    publication_date character varying,
    lang smallint,
    lang_orig smallint,
    title1 character varying,
    title2 character varying,
    title3 character varying,
    title4 character varying,
    author1 character varying,
    author2 character varying,
    author3 character varying,
    serie_name character varying,
    serie_vol_number smallint,
    public_type smallint,
    editor_name character varying,
    editor_place character varying,
    editor_date character varying,
    nb_pages character varying,
    format character varying,
    addon character varying,
    illus_type smallint,
    litterature_type smallint,
    collection_title1 character varying,
    collection_title2 character varying,
    collection_title3 character varying,
    collection_number_sub smallint,
    collection_issn character varying,
    collection_vol_number smallint,
    abstract character varying,
    keywords character varying,
    nb_specimens smallint,
    state character varying,
    source_name character varying,
    source_country character varying,
    source_date character varying,
    source_norme character varying,
    age character varying
);


ALTER TABLE public.remote_books OWNER TO elidune;

--
-- Name: remote_books_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.remote_books_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.remote_books_id_seq OWNER TO elidune;

--
-- Name: remote_books_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.remote_books_id_seq OWNED BY public.remote_books.id;


--
-- Name: remote_items; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.remote_items (
    id integer NOT NULL,
    media_type character varying,
    identification character varying,
    price character varying,
    barcode character varying,
    dewey character varying,
    publication_date character varying,
    lang smallint,
    lang_orig smallint,
    title1 character varying,
    title2 character varying,
    title3 character varying,
    title4 character varying,
    author1_ids integer[],
    author1_functions character varying,
    author2_ids integer[],
    author2_functions character varying,
    author3_ids integer[],
    author3_functions character varying,
    serie_id integer,
    serie_vol_number smallint,
    collection_id integer,
    collection_number_sub smallint,
    collection_vol_number smallint,
    source_id integer,
    source_date character varying,
    source_norme character varying,
    genre smallint,
    subject character varying,
    public_type smallint,
    edition_id integer,
    edition_date character varying,
    nb_pages character varying,
    format character varying,
    content character varying,
    addon character varying,
    abstract character varying,
    notes character varying,
    keywords character varying,
    nb_specimens smallint,
    state character varying,
    is_archive smallint DEFAULT (0)::smallint,
    archived_timestamp integer,
    is_valid smallint DEFAULT (0)::smallint,
    crea_date integer,
    modif_date integer
);


ALTER TABLE public.remote_items OWNER TO elidune;

--
-- Name: remote_items_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.remote_items_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.remote_items_id_seq OWNER TO elidune;

--
-- Name: remote_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.remote_items_id_seq OWNED BY public.remote_items.id;


--
-- Name: remote_specimens; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.remote_specimens (
    id integer NOT NULL,
    id_item integer,
    source_id integer,
    identification character varying,
    cote character varying,
    media_type character varying,
    place smallint,
    status smallint,
    codestat smallint,
    notes character varying,
    price character varying,
    creation_date integer,
    modif_date integer
);


ALTER TABLE public.remote_specimens OWNER TO elidune;

--
-- Name: remote_specimens_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.remote_specimens_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.remote_specimens_id_seq OWNER TO elidune;

--
-- Name: remote_specimens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.remote_specimens_id_seq OWNED BY public.remote_specimens.id;


--
-- Name: sal_emp; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.sal_emp (
    name text,
    pay_by_quarter integer[],
    schedule text[]
);


ALTER TABLE public.sal_emp OWNER TO elidune;

--
-- Name: serie_id; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.serie_id (
    nextval bigint
);


ALTER TABLE public.serie_id OWNER TO elidune;

--
-- Name: series; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.series (
    id integer NOT NULL,
    key character varying,
    name character varying
);


ALTER TABLE public.series OWNER TO elidune;

--
-- Name: series_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.series_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.series_id_seq OWNER TO elidune;

--
-- Name: series_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.series_id_seq OWNED BY public.series.id;


--
-- Name: sources; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.sources (
    id integer NOT NULL,
    key character varying,
    name character varying
);


ALTER TABLE public.sources OWNER TO elidune;

--
-- Name: sources_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.sources_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.sources_id_seq OWNER TO elidune;

--
-- Name: sources_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.sources_id_seq OWNED BY public.sources.id;


--
-- Name: specimens; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.specimens (
    id integer NOT NULL,
    id_item integer,
    source_id integer,
    identification character varying,
    cote character varying,
    place smallint,
    status smallint,
    codestat smallint,
    notes character varying,
    price character varying,
    modif_date integer,
    is_archive integer DEFAULT (0)::smallint,
    archive_date integer DEFAULT (0)::smallint,
    crea_date integer
);


ALTER TABLE public.specimens OWNER TO elidune;

--
-- Name: specimens_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.specimens_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.specimens_id_seq OWNER TO elidune;

--
-- Name: specimens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.specimens_id_seq OWNED BY public.specimens.id;


--
-- Name: users; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.users (
    id integer NOT NULL,
    login character varying,
    password character varying,
    firstname character varying,
    lastname character varying,
    email character varying,
    addr_street character varying,
    addr_zip_code integer,
    addr_city character varying,
    phone character varying,
    sex_id smallint,
    account_type_id smallint,
    subscription_type_id smallint,
    fee_id smallint,
    last_payement_date timestamp without time zone DEFAULT ('now'::text)::timestamp(6) with time zone,
    group_id integer,
    barcode character varying,
    notes character varying,
    occupation character varying,
    crea_date integer,
    modif_date integer,
    issue_date integer,
    profession smallint,
    birthdate character varying,
    archived_date integer DEFAULT (0)::smallint,
    public_type integer
);


ALTER TABLE public.users OWNER TO elidune;

--
-- Name: users_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.users_id_seq OWNER TO elidune;

--
-- Name: users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.users_id_seq OWNED BY public.users.id;


--
-- Name: z3950servers; Type: TABLE; Schema: public; Owner: elidune
--

CREATE TABLE public.z3950servers (
    id integer NOT NULL,
    address character varying,
    port integer DEFAULT 2200 NOT NULL,
    name character varying,
    description character varying,
    activated integer,
    login character varying,
    password character varying,
    database character varying,
    format character varying
);


ALTER TABLE public.z3950servers OWNER TO elidune;

--
-- Name: z3950servers_id_seq; Type: SEQUENCE; Schema: public; Owner: elidune
--

CREATE SEQUENCE public.z3950servers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER SEQUENCE public.z3950servers_id_seq OWNER TO elidune;

--
-- Name: z3950servers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: elidune
--

ALTER SEQUENCE public.z3950servers_id_seq OWNED BY public.z3950servers.id;


--
-- Name: account_types id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.account_types ALTER COLUMN id SET DEFAULT nextval('public.account_types_id_seq'::regclass);


--
-- Name: audios id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.audios ALTER COLUMN id SET DEFAULT nextval('public.audios_id_seq'::regclass);


--
-- Name: authors id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.authors ALTER COLUMN id SET DEFAULT nextval('public.authors_id_seq'::regclass);


--
-- Name: books id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.books ALTER COLUMN id SET DEFAULT nextval('public.books_id_seq'::regclass);


--
-- Name: borrows id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.borrows ALTER COLUMN id SET DEFAULT nextval('public.borrows_id_seq'::regclass);


--
-- Name: borrows_archives id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.borrows_archives ALTER COLUMN id SET DEFAULT nextval('public.borrows_archives_id_seq'::regclass);


--
-- Name: borrows_settings id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.borrows_settings ALTER COLUMN id SET DEFAULT nextval('public.borrows_settings_id_seq'::regclass);


--
-- Name: collections id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.collections ALTER COLUMN id SET DEFAULT nextval('public.collections_id_seq'::regclass);


--
-- Name: editions id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.editions ALTER COLUMN id SET DEFAULT nextval('public.editions_id_seq'::regclass);


--
-- Name: fees id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.fees ALTER COLUMN id SET DEFAULT nextval('public.fees_id_seq'::regclass);


--
-- Name: items id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.items ALTER COLUMN id SET DEFAULT nextval('public.items_id_seq'::regclass);


--
-- Name: remote_books id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.remote_books ALTER COLUMN id SET DEFAULT nextval('public.remote_books_id_seq'::regclass);


--
-- Name: remote_items id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.remote_items ALTER COLUMN id SET DEFAULT nextval('public.remote_items_id_seq'::regclass);


--
-- Name: remote_specimens id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.remote_specimens ALTER COLUMN id SET DEFAULT nextval('public.remote_specimens_id_seq'::regclass);


--
-- Name: series id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.series ALTER COLUMN id SET DEFAULT nextval('public.series_id_seq'::regclass);


--
-- Name: sources id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.sources ALTER COLUMN id SET DEFAULT nextval('public.sources_id_seq'::regclass);


--
-- Name: specimens id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.specimens ALTER COLUMN id SET DEFAULT nextval('public.specimens_id_seq'::regclass);


--
-- Name: users id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.users ALTER COLUMN id SET DEFAULT nextval('public.users_id_seq'::regclass);


--
-- Name: z3950servers id; Type: DEFAULT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.z3950servers ALTER COLUMN id SET DEFAULT nextval('public.z3950servers_id_seq'::regclass);


--
-- Name: account_types account_types_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.account_types
    ADD CONSTRAINT account_types_pkey PRIMARY KEY (id);


--
-- Name: audios audios_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.audios
    ADD CONSTRAINT audios_pkey PRIMARY KEY (id);


--
-- Name: authors authors_key_key; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.authors
    ADD CONSTRAINT authors_key_key UNIQUE (key);


--
-- Name: authors authors_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.authors
    ADD CONSTRAINT authors_pkey PRIMARY KEY (id);


--
-- Name: books books_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.books
    ADD CONSTRAINT books_pkey PRIMARY KEY (id);


--
-- Name: borrows_archives borrows_archives_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.borrows_archives
    ADD CONSTRAINT borrows_archives_pkey PRIMARY KEY (id);


--
-- Name: borrows borrows_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.borrows
    ADD CONSTRAINT borrows_pkey PRIMARY KEY (id);


--
-- Name: borrows_settings borrows_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.borrows_settings
    ADD CONSTRAINT borrows_settings_pkey PRIMARY KEY (id);


--
-- Name: collections collections_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.collections
    ADD CONSTRAINT collections_pkey PRIMARY KEY (id);


--
-- Name: editions editions_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.editions
    ADD CONSTRAINT editions_pkey PRIMARY KEY (id);


--
-- Name: items items_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.items
    ADD CONSTRAINT items_pkey PRIMARY KEY (id);


--
-- Name: remote_books remote_books_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.remote_books
    ADD CONSTRAINT remote_books_pkey PRIMARY KEY (id);


--
-- Name: remote_items remote_items_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.remote_items
    ADD CONSTRAINT remote_items_pkey PRIMARY KEY (id);


--
-- Name: remote_specimens remote_specimens_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.remote_specimens
    ADD CONSTRAINT remote_specimens_pkey PRIMARY KEY (id);


--
-- Name: series series_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.series
    ADD CONSTRAINT series_pkey PRIMARY KEY (id);


--
-- Name: sources sources_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.sources
    ADD CONSTRAINT sources_pkey PRIMARY KEY (id);


--
-- Name: specimens specimens_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.specimens
    ADD CONSTRAINT specimens_pkey PRIMARY KEY (id);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: elidune
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: account_types_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX account_types_id_key ON public.users USING btree (id);


--
-- Name: authors_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX authors_id_key ON public.authors USING btree (id);


--
-- Name: authors_lastname_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX authors_lastname_key ON public.authors USING btree (lastname);


--
-- Name: borrows_archives_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX borrows_archives_id_key ON public.borrows_archives USING btree (id);


--
-- Name: borrows_archives_item_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX borrows_archives_item_id_key ON public.borrows_archives USING btree (item_id);


--
-- Name: borrows_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX borrows_id_key ON public.borrows USING btree (id);


--
-- Name: borrows_settings_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX borrows_settings_id_key ON public.borrows_settings USING btree (id);


--
-- Name: borrows_settings_media_type_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX borrows_settings_media_type_key ON public.borrows_settings USING btree (media_type);


--
-- Name: borrows_specimen_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX borrows_specimen_id_key ON public.borrows USING btree (specimen_id);


--
-- Name: borrows_user_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX borrows_user_id_key ON public.borrows USING btree (user_id);


--
-- Name: collections_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX collections_id_key ON public.collections USING btree (id);


--
-- Name: editions_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX editions_id_key ON public.editions USING btree (id);


--
-- Name: editions_name_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX editions_name_key ON public.editions USING btree (name);


--
-- Name: items_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX items_id_key ON public.items USING btree (id);


--
-- Name: items_identification_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX items_identification_key ON public.items USING btree (identification);


--
-- Name: items_title1_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX items_title1_key ON public.items USING btree (title1);


--
-- Name: remote_items_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX remote_items_id_key ON public.remote_items USING btree (id);


--
-- Name: remote_items_identification_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX remote_items_identification_key ON public.remote_items USING btree (identification);


--
-- Name: remote_items_title1_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX remote_items_title1_key ON public.remote_items USING btree (title1);


--
-- Name: remote_specimens_id_item_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX remote_specimens_id_item_key ON public.remote_specimens USING btree (id_item);


--
-- Name: remote_specimens_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX remote_specimens_id_key ON public.remote_specimens USING btree (id);


--
-- Name: remote_specimens_identification_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX remote_specimens_identification_key ON public.remote_specimens USING btree (identification);


--
-- Name: remote_specimens_source_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX remote_specimens_source_id_key ON public.remote_specimens USING btree (source_id);


--
-- Name: series_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX series_id_key ON public.series USING btree (id);


--
-- Name: series_name_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX series_name_key ON public.series USING btree (name);


--
-- Name: sources_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX sources_id_key ON public.sources USING btree (id);


--
-- Name: sources_name_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX sources_name_key ON public.sources USING btree (name);


--
-- Name: specimens_id_item_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX specimens_id_item_key ON public.specimens USING btree (id_item);


--
-- Name: specimens_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX specimens_id_key ON public.specimens USING btree (id);


--
-- Name: specimens_identification_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX specimens_identification_key ON public.specimens USING btree (identification);


--
-- Name: specimens_source_id_key; Type: INDEX; Schema: public; Owner: elidune
--

CREATE INDEX specimens_source_id_key ON public.specimens USING btree (source_id);


--
-- Name: SCHEMA public; Type: ACL; Schema: -; Owner: elidune
--

REVOKE USAGE ON SCHEMA public FROM PUBLIC;
GRANT ALL ON SCHEMA public TO postgres;
GRANT ALL ON SCHEMA public TO PUBLIC;


--
-- PostgreSQL database dump complete
--

