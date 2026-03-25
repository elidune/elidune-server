-- =============================================================================
-- Elidune Database Initialization Script
-- =============================================================================
-- This file represents the canonical target schema after all migrations
-- (001 through 044). Use it to create a fresh database for migration or testing.
--
-- Run order: extensions → lookup tables → core domain → junction/relation tables
--            → operational tables → stats/audit/config tables
-- =============================================================================

-- =============================================================================
-- EXTENSIONS
-- =============================================================================

CREATE EXTENSION IF NOT EXISTS unaccent;

-- =============================================================================
-- LOOKUP TABLES
-- =============================================================================

CREATE TABLE IF NOT EXISTS account_types (
    code                VARCHAR(50)  PRIMARY KEY,
    name                VARCHAR(100),
    items_rights        VARCHAR(1),
    users_rights        VARCHAR(1),
    loans_rights        VARCHAR(1),
    items_archive_rights VARCHAR(1),
    borrows_rights      VARCHAR(1),
    settings_rights     VARCHAR(1)
);

INSERT INTO account_types (code, name, items_rights, users_rights, loans_rights, items_archive_rights, borrows_rights, settings_rights) VALUES
    ('guest', 'Guest', 'r', 'r', 'n', 'n', 'n', 'r'),
    ('reader', 'Reader', 'r', 'r', 'r', 'r', 'r', 'r'),
    ('librarian', 'Librarian', 'w', 'w', 'w', 'w', 'w', 'r'),
    ('admin', 'Administrator', 'w', 'w', 'w', 'w', 'w', 'w'),
    ('group', 'Group', 'r', 'r', 'r', 'r', 'r', 'r');



CREATE TABLE IF NOT EXISTS fees (
    code    VARCHAR(50)  PRIMARY KEY,
    name    VARCHAR(100),
    amount  INTEGER DEFAULT 0
);

-- insert some basic fees
INSERT INTO fees (code, name, amount) VALUES
    ('free', 'Free', 0),
    ('local', 'Local', 0),
    ('foreigner', 'Foreigner', 0);

CREATE TABLE IF NOT EXISTS public_types (
    id                          BIGSERIAL   PRIMARY KEY,
    name                        VARCHAR(50) NOT NULL UNIQUE,
    label                       VARCHAR(100) NOT NULL,
    subscription_duration_days  INTEGER     DEFAULT 365,
    age_min                     SMALLINT,
    age_max                     SMALLINT,
    subscription_price          INTEGER     DEFAULT 0,
    max_loans                   SMALLINT,
    loan_duration_days          SMALLINT
);

INSERT INTO public_types (name, label, subscription_duration_days, age_min, age_max, subscription_price, max_loans, loan_duration_days) VALUES
    ('child',  'Child',    365, 0,    12, 0,    10,   21),
    ('adult',  'Adult',    365, 18,   99, 1500, 5,    21),
    ('school', 'School',     365, NULL, NULL, 0,  50,   60),
    ('staff',  'Staff', 365, NULL, NULL, NULL, NULL, NULL),
    ('senior', 'Senior',    365, 60,   99, 0,    5,    21)
ON CONFLICT (name) DO NOTHING;



CREATE TABLE IF NOT EXISTS public_type_loan_settings (
    id              BIGSERIAL   PRIMARY KEY,
    public_type_id  BIGINT      NOT NULL REFERENCES public_types(id) ON DELETE CASCADE,
    media_type      VARCHAR(50) NOT NULL,
    duration        SMALLINT,
    nb_max          SMALLINT,
    nb_renews       SMALLINT,
    UNIQUE(public_type_id, media_type)
);

CREATE INDEX IF NOT EXISTS idx_public_type_loan_settings_public_type
    ON public_type_loan_settings(public_type_id);



-- =============================================================================
-- AUTHORS
-- =============================================================================

CREATE TABLE IF NOT EXISTS authors (
    id          BIGSERIAL   PRIMARY KEY,
    key         VARCHAR(255),
    lastname    VARCHAR(255),
    firstname   VARCHAR(255),
    bio         TEXT,
    notes       TEXT,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    update_at   TIMESTAMPTZ
);

-- =============================================================================
-- EDITIONS
-- =============================================================================

CREATE TABLE IF NOT EXISTS editions (
    id                    BIGSERIAL   PRIMARY KEY,
    key                   VARCHAR(255),
    publisher_name        VARCHAR(255),
    place_of_publication  VARCHAR(255),
    notes                 TEXT,
    date                  VARCHAR(20),
    created_at            TIMESTAMPTZ DEFAULT NOW(),
    updated_at            TIMESTAMPTZ DEFAULT NOW()
);

-- =============================================================================
-- SERIES
-- =============================================================================

CREATE TABLE IF NOT EXISTS series (
    id          BIGSERIAL   PRIMARY KEY,
    key         VARCHAR(255),
    name        VARCHAR(255) NOT NULL,
    issn        VARCHAR(30),
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_series_key_unique
    ON series (key) WHERE key IS NOT NULL;

-- =============================================================================
-- COLLECTIONS
-- =============================================================================

CREATE TABLE IF NOT EXISTS collections (
    id              BIGSERIAL   PRIMARY KEY,
    key             VARCHAR(255),
    name            VARCHAR(255) NOT NULL,
    secondary_title VARCHAR(255),
    tertiary_title  VARCHAR(255),
    issn            VARCHAR(30),
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_collections_key_unique
    ON collections (key) WHERE key IS NOT NULL;

-- =============================================================================
-- SOURCES
-- =============================================================================

CREATE TABLE IF NOT EXISTS sources (
    id          BIGSERIAL   PRIMARY KEY,
    key         VARCHAR(255),
    name        VARCHAR(255),
    is_archive  SMALLINT    DEFAULT 0,
    archived_at TIMESTAMPTZ,
    "default"   BOOLEAN     DEFAULT FALSE
);

CREATE UNIQUE INDEX IF NOT EXISTS sources_default_unique
    ON sources ("default") WHERE "default" = TRUE;

INSERT INTO sources (name, is_archive, "default") VALUES ('MyLibrary', 0, TRUE);

-- =============================================================================
-- USERS
-- =============================================================================

CREATE TABLE IF NOT EXISTS users (
    id                  BIGSERIAL   PRIMARY KEY,
    login               VARCHAR(255) UNIQUE,
    password            VARCHAR(255),
    firstname           VARCHAR(255),
    lastname            VARCHAR(255),
    email               VARCHAR(255),
    addr_street         VARCHAR(255),
    addr_zip_code       INTEGER,
    addr_city           VARCHAR(255),
    phone               VARCHAR(50),
    account_type        VARCHAR(50)  NOT NULL DEFAULT 'guest'
                            REFERENCES account_types(code),
    fee                 VARCHAR(50),
    group_id            BIGINT,
    barcode             VARCHAR(100) UNIQUE,
    notes               TEXT,
    public_type         BIGINT       REFERENCES public_types(id),
    status              SMALLINT     DEFAULT 0,
    birthdate           VARCHAR(20),
    created_at          TIMESTAMPTZ  DEFAULT NOW(),
    update_at           TIMESTAMPTZ,
    issue_at            TIMESTAMPTZ,
    archived_at         TIMESTAMPTZ,
    language            VARCHAR(32)  DEFAULT 'french',
    sex                 SMALLINT     DEFAULT 85,
    staff_type          SMALLINT,
    hours_per_week      REAL,
    staff_start_date    DATE,
    staff_end_date      DATE,
    receive_reminders   BOOLEAN      NOT NULL DEFAULT TRUE,
    two_factor_enabled  BOOLEAN      DEFAULT FALSE,
    two_factor_method   VARCHAR(20),
    totp_secret         TEXT,
    recovery_codes      TEXT,
    recovery_codes_used TEXT,
    must_change_password BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_users_account_type ON users(account_type);
CREATE INDEX IF NOT EXISTS idx_users_public_type  ON users(public_type);

-- =============================================================================
-- ITEMS
-- =============================================================================

CREATE TABLE IF NOT EXISTS biblios (
    id                          BIGSERIAL   PRIMARY KEY,
    media_type                  VARCHAR(30) NOT NULL DEFAULT 'unknown',
    isbn                        VARCHAR(30),
    title                       VARCHAR(500),
    subject                     TEXT,
    audience_type               VARCHAR(30),
    lang                        VARCHAR(32),
    lang_orig                   VARCHAR(32),
    publication_date            VARCHAR(20),
    source_id                   BIGINT      REFERENCES sources(id) ON DELETE SET NULL,
    edition_id                  BIGINT      REFERENCES editions(id) ON DELETE SET NULL,
    page_extent                 TEXT,
    format                      TEXT,
    table_of_contents           TEXT,
    accompanying_material       TEXT,
    abstract                    TEXT,
    notes                       TEXT,
    keywords                    VARCHAR[],
    is_valid                    SMALLINT    DEFAULT 1,
    marc_record                 JSONB,
    created_at                  TIMESTAMPTZ DEFAULT NOW(),
    updated_at                  TIMESTAMPTZ DEFAULT NOW(),
    archived_at                 TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_biblios_isbn       ON biblios(isbn) WHERE isbn IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_biblios_media_type ON biblios(media_type);
CREATE INDEX IF NOT EXISTS idx_biblios_active     ON biblios(archived_at) WHERE archived_at IS NULL;

-- =============================================================================
-- BIBLIO_AUTHORS (N:M junction)
-- =============================================================================

CREATE TABLE IF NOT EXISTS biblio_authors (
    id          BIGSERIAL   PRIMARY KEY,
    biblio_id   BIGINT      NOT NULL REFERENCES biblios(id) ON DELETE CASCADE,
    author_id   BIGINT      NOT NULL REFERENCES authors(id) ON DELETE CASCADE,
    function    VARCHAR(50),
    author_type SMALLINT    NOT NULL DEFAULT 0,
    position    SMALLINT    NOT NULL DEFAULT 1,
    UNIQUE(biblio_id, author_id, function)
);

CREATE INDEX IF NOT EXISTS idx_biblio_authors_biblio  ON biblio_authors(biblio_id);
CREATE INDEX IF NOT EXISTS idx_biblio_authors_author  ON biblio_authors(author_id);

-- =============================================================================
-- BIBLIO_SERIES (N:M junction)
-- =============================================================================

CREATE TABLE IF NOT EXISTS biblio_series (
    id              BIGSERIAL   PRIMARY KEY,
    biblio_id       BIGINT      NOT NULL REFERENCES biblios(id) ON DELETE CASCADE,
    series_id       BIGINT      NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    position        SMALLINT    NOT NULL DEFAULT 1,
    volume_number   SMALLINT,
    UNIQUE (biblio_id, series_id)
);

CREATE INDEX IF NOT EXISTS idx_biblio_series_biblio  ON biblio_series(biblio_id);
CREATE INDEX IF NOT EXISTS idx_biblio_series_series  ON biblio_series(series_id);

-- =============================================================================
-- BIBLIO_COLLECTIONS (N:M junction)
-- =============================================================================

CREATE TABLE IF NOT EXISTS biblio_collections (
    id              BIGSERIAL   PRIMARY KEY,
    biblio_id       BIGINT      NOT NULL REFERENCES biblios(id)     ON DELETE CASCADE,
    collection_id   BIGINT      NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    position        SMALLINT    NOT NULL DEFAULT 1,
    volume_number   SMALLINT,
    UNIQUE (biblio_id, collection_id)
);

CREATE INDEX IF NOT EXISTS idx_biblio_collections_biblio     ON biblio_collections(biblio_id);
CREATE INDEX IF NOT EXISTS idx_biblio_collections_collection ON biblio_collections(collection_id);

-- =============================================================================
-- ITEMS (physical copies, formerly specimens)
-- =============================================================================

CREATE TABLE IF NOT EXISTS items (
    id                  BIGSERIAL   PRIMARY KEY,
    biblio_id           BIGINT      REFERENCES biblios(id) ON DELETE CASCADE,
    source_id           BIGINT      REFERENCES sources(id) ON DELETE SET NULL,
    barcode             VARCHAR(100),
    call_number         VARCHAR(100),
    volume_designation  TEXT,
    place               SMALLINT,
    borrowable          BOOLEAN     NOT NULL DEFAULT TRUE,
    circulation_status  SMALLINT,
    notes               TEXT,
    price               TEXT,
    created_at          TIMESTAMPTZ DEFAULT NOW(),
    updated_at          TIMESTAMPTZ,
    archived_at         TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_items_barcode_unique
    ON items (barcode) WHERE barcode IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_items_biblio  ON items(biblio_id);
CREATE INDEX IF NOT EXISTS idx_items_active  ON items(archived_at) WHERE archived_at IS NULL;

-- =============================================================================
-- LOANS
-- =============================================================================

CREATE TABLE IF NOT EXISTS loans (
    id                      BIGSERIAL   PRIMARY KEY,
    user_id                 BIGINT      NOT NULL,
    item_id                 BIGINT      REFERENCES items(id),
    date                    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    renew_at                TIMESTAMPTZ,
    nb_renews               SMALLINT    DEFAULT 0,
    issue_at                TIMESTAMPTZ,
    notes                   TEXT,
    returned_at             TIMESTAMPTZ,
    last_reminder_sent_at   TIMESTAMPTZ,
    reminder_count          INTEGER     NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_loans_user_id    ON loans(user_id);
CREATE INDEX IF NOT EXISTS idx_loans_item_id    ON loans(item_id);
CREATE INDEX IF NOT EXISTS idx_loans_active     ON loans(returned_at) WHERE returned_at IS NULL;

-- =============================================================================
-- LOANS_ARCHIVES
-- =============================================================================

CREATE TABLE IF NOT EXISTS loans_archives (
    id                      BIGSERIAL   PRIMARY KEY,
    user_id                 BIGINT,
    item_id                 BIGINT      REFERENCES items(id) ON DELETE SET NULL,
    date                    TIMESTAMPTZ,
    nb_renews               SMALLINT    DEFAULT 0,
    issue_at                TIMESTAMPTZ,
    returned_at             TIMESTAMPTZ,
    notes                   TEXT,
    borrower_public_type    BIGINT      REFERENCES public_types(id) ON DELETE SET NULL,
    addr_city               VARCHAR(255),
    account_type            VARCHAR(50)
);

CREATE INDEX IF NOT EXISTS idx_loans_archives_item_id ON loans_archives(item_id);
CREATE INDEX IF NOT EXISTS idx_loans_archives_user     ON loans_archives(user_id);

-- =============================================================================
-- LOANS_SETTINGS
-- =============================================================================

CREATE TABLE IF NOT EXISTS loans_settings (
    id          BIGSERIAL   PRIMARY KEY,
    media_type  VARCHAR(30) UNIQUE,
    nb_max      SMALLINT,
    nb_renews   SMALLINT,
    duration    SMALLINT,
    notes       TEXT,
    account_type VARCHAR(50)
);

INSERT INTO loans_settings (media_type, nb_max, nb_renews, duration, notes, account_type) VALUES
    ('audio', 2, 2, 14, '', ''),
    ('printedText', 3, 2, 21, '', ''),
    ('periodic', 2, 2, 14, '', '');

-- =============================================================================
-- Z3950 SERVERS
-- =============================================================================

CREATE TABLE IF NOT EXISTS z3950servers (
    id          BIGSERIAL   PRIMARY KEY,
    address     VARCHAR(255),
    port        INTEGER,
    name        VARCHAR(255),
    description TEXT,
    activated   BOOLEAN     DEFAULT TRUE,
    login       VARCHAR(255),
    password    VARCHAR(255),
    database    VARCHAR(255),
    format      VARCHAR(50),
    encoding    VARCHAR(20) DEFAULT 'utf-8'
);


INSERT INTO z3950servers (address, port, name, description, activated, login, password, database, format, encoding) VALUES
    ('z3950.bnf.fr', 2211, 'BNF', 'Bibliothèque nationale de France', TRUE, 'Z3950', 'Z3950_BNF', 'TOUT-UTF8', 'UNIMARC', 'utf-8'),
    ('z3950.loc.gov', 7090, 'Library of Congress', 'Library of Congress Z39.50 Server', TRUE, '', '', 'VOYAGER', 'MARC21', 'utf-8'),
    ('opac.sudoc.abes.fr', 2200, 'SUDOC', 'Système Universitaire de Documentation', TRUE, '', '', 'abes', 'UNIMARC', 'utf-8');


-- =============================================================================
-- VISITOR COUNTS
-- =============================================================================

CREATE TABLE IF NOT EXISTS visitor_counts (
    id          BIGSERIAL   PRIMARY KEY,
    count_date  DATE        NOT NULL,
    count       INTEGER     NOT NULL DEFAULT 0,
    source      VARCHAR(50) DEFAULT 'manual',
    notes       VARCHAR,
    created_at  TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_visitor_counts_date ON visitor_counts(count_date);

-- =============================================================================
-- SCHEDULE
-- =============================================================================

CREATE TABLE IF NOT EXISTS schedule_periods (
    id          BIGSERIAL   PRIMARY KEY,
    name        VARCHAR(100) NOT NULL,
    start_date  DATE        NOT NULL,
    end_date    DATE        NOT NULL,
    notes       VARCHAR,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    update_at   TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS schedule_slots (
    id          BIGSERIAL   PRIMARY KEY,
    period_id   BIGINT      NOT NULL REFERENCES schedule_periods(id) ON DELETE CASCADE,
    day_of_week SMALLINT    NOT NULL CHECK (day_of_week BETWEEN 0 AND 6),
    open_time   TIME        NOT NULL,
    close_time  TIME        NOT NULL,
    created_at  TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_schedule_slots_period  ON schedule_slots(period_id);

CREATE TABLE IF NOT EXISTS schedule_closures (
    id              BIGSERIAL   PRIMARY KEY,
    closure_date    DATE        NOT NULL,
    reason          VARCHAR,
    created_at      TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_schedule_closures_date ON schedule_closures(closure_date);

-- =============================================================================
-- EQUIPMENT
-- =============================================================================

CREATE TABLE IF NOT EXISTS equipment (
    id              BIGSERIAL   PRIMARY KEY,
    name            VARCHAR(255) NOT NULL,
    equipment_type  SMALLINT    NOT NULL DEFAULT 0,
    has_internet    BOOLEAN     DEFAULT FALSE,
    is_public       BOOLEAN     DEFAULT TRUE,
    quantity        INTEGER     DEFAULT 1,
    status          SMALLINT    DEFAULT 0,
    notes           VARCHAR,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    update_at       TIMESTAMPTZ
);

-- =============================================================================
-- EVENTS
-- =============================================================================

CREATE TABLE IF NOT EXISTS events (
    id                      BIGSERIAL   PRIMARY KEY,
    name                    VARCHAR(255) NOT NULL,
    event_type              SMALLINT    NOT NULL DEFAULT 0,
    event_date              DATE        NOT NULL,
    start_time              TIME,
    end_time                TIME,
    attendees_count         INTEGER     DEFAULT 0,
    target_public           SMALLINT,
    school_name             VARCHAR(255),
    class_name              VARCHAR(255),
    students_count          INTEGER,
    partner_name            VARCHAR(255),
    description             VARCHAR,
    notes                   VARCHAR,
    created_at              TIMESTAMPTZ DEFAULT NOW(),
    update_at               TIMESTAMPTZ,
    announcement_sent_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_events_date ON events(event_date);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);

-- =============================================================================
-- SETTINGS (admin-overridable runtime config)
-- =============================================================================

CREATE TABLE IF NOT EXISTS settings (
    key         VARCHAR(100) PRIMARY KEY,
    value       JSONB        NOT NULL,
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- =============================================================================
-- AUDIT LOG
-- =============================================================================

CREATE TABLE IF NOT EXISTS audit_log (
    id          BIGSERIAL   PRIMARY KEY,
    event_type  TEXT        NOT NULL,
    user_id     BIGINT,
    entity_type TEXT,
    entity_id   BIGINT,
    ip_address  TEXT,
    payload     JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS audit_log_event_type_idx ON audit_log (event_type);
CREATE INDEX IF NOT EXISTS audit_log_entity_idx     ON audit_log (entity_type, entity_id);
CREATE INDEX IF NOT EXISTS audit_log_user_id_idx    ON audit_log (user_id);
CREATE INDEX IF NOT EXISTS audit_log_created_at_idx ON audit_log (created_at DESC);

-- =============================================================================
-- LIBRARY INFO
-- =============================================================================

CREATE TABLE IF NOT EXISTS library_info (
    id              SMALLINT    PRIMARY KEY DEFAULT 1,
    name            TEXT,
    addr_line1      VARCHAR(100),
    addr_line2      VARCHAR(100),
    addr_postcode   VARCHAR(10),
    addr_city       VARCHAR(100),
    addr_country    VARCHAR(50),
    phones          JSONB       NOT NULL DEFAULT '[]'::jsonb,
    email           TEXT,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT library_info_single_row CHECK (id = 1)
);

INSERT INTO library_info (id) VALUES (1) ON CONFLICT DO NOTHING;
