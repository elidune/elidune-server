#!/usr/bin/env python3
"""
Data migration script for Elidune
Migrates data from the legacy C/XML-RPC PostgreSQL schema to the new Rust schema.

Source schema: legacy database (see elidune-pgdump.sql)
Target schema: new database (see init_database.sql)

Usage:
    python migrate_data.py --source-db <old_db_url> --target-db <new_db_url>
    python migrate_data.py --source-db <old_db_url> --target-db <new_db_url> --reset
    python migrate_data.py --source-db <old_db_url> --target-db <new_db_url> --skip-items --skip-users

Key transformations:
    - account_types: id (int) -> code (slug)
    - fees: id (int) -> code (slug), "desc" -> "name"
    - users: account_type_id -> account_type (slug), fee_id -> fee (slug),
             crea_date/modif_date/issue_date/archived_date (int) -> timestamptz,
             passwords -> argon2 hash,
             public_type int (97/106/117) -> FK to public_types table,
             sex_id -> sex (preserved as SMALLINT)
             removed: subscription_type_id, occupation, profession
             added: status (VARCHAR camelCase: active/blocked/deleted), language, must_change_password, 2FA fields
    - items -> biblios: complete schema refactor (see migrate_items)
      - identification->isbn, title1->title, author columns -> biblio_authors table,
        serie_id -> biblio_series junction, collection_id -> biblio_collections junction,
        media_type/lang/audience_type now camelCase strings,
        crea_date/modif_date (int) -> created_at/updated_at (timestamptz)
      - FK columns that were 0 in legacy (no link) -> NULL (source_id, edition_id, etc.)
      - rows with both empty ISBN (identification) and no title (title1-4) are skipped;
        linked specimens and loans on those rows are skipped for FK consistency
      - biblios.title uses first non-empty among title1, title2, title3, title4
    - specimens -> items: physical copies (see migrate_specimens)
      - identification->barcode, cote->call_number, codestat->circulation_status,
        status(98/110)->borrowable bool, id_item->biblio_id,
        modif_date/archive_date/crea_date (int) -> updated_at/archived_at/created_at (timestamptz)
    - borrows -> loans (active) + loans_archives (returned)
      - specimen_id -> item_id (legacy_fk_id: 0/NULL skipped), renew_date/issue_date/returned_date (int) -> timestamptz
    - borrows_archives -> loans_archives
      - account_type_id -> account_type slug, specimen_id -> item_id (legacy_fk_id: 0 skipped),
        issue_date/returned_date (int) -> timestamptz
    - borrows_settings -> loans_settings (account_type_id -> account_type slug)
    - z3950servers: activated int -> boolean, added encoding (default utf-8)

Requirements:
    pip install psycopg2-binary argon2-cffi
"""

import argparse
import re
import sys
from datetime import date, datetime, timezone
from pathlib import Path

import psycopg2

try:
    from argon2 import PasswordHasher
    ARGON2_AVAILABLE = True
except ImportError:
    ARGON2_AVAILABLE = False
    print("Warning: argon2-cffi not installed. Install with: pip install argon2-cffi")


# =============================================================================
# CONSTANTS - Source to target mappings
# =============================================================================

ACCOUNT_TYPE_ID_TO_CODE = {
    1: 'guest',
    2: 'reader',
    3: 'librarian',
    4: 'admin',
    8: 'group',
}

FEE_ID_TO_CODE = {
    1: 'free',
    2: 'local',
    3: 'foreigner',
}

# Legacy integer public_type -> public_types.name (seeded in migration 033)
# child=1, adult=2, school=3, staff=4, senior=5 (by insertion order)
PUBLIC_TYPE_INT_TO_NAME = {
    97:  'adult',
    106: 'child',
    117: 'senior',
}

# Legacy media_type codes -> camelCase DB strings (migration 028)
MEDIA_TYPE_CODE_TO_DB = {
    '':    'all',
    'u':   'unknown',
    'b':   'printedText',
    'm':   'multimedia',
    'bc':  'comics',
    'p':   'periodic',
    'v':   'video',
    'vt':  'videoTape',
    'vd':  'videoDvd',
    'a':   'audio',
    'am':  'audioMusic',
    'amt': 'audioMusicTape',
    'amc': 'audioMusicCd',
    'an':  'audioNonMusic',
    'ant': 'audioNonMusicTape',
    'anc': 'audioNonMusicCd',
    'c':   'cdRom',
    'i':   'images',
}

# Legacy integer lang codes -> camelCase DB strings (migration 029)
LANG_INT_TO_DB = {
    0: 'unknown',
    1: 'french',
    2: 'english',
    3: 'german',
    4: 'japanese',
    5: 'spanish',
    6: 'portuguese',
}

# Legacy audience_type integers -> camelCase strings (migration 042)
AUDIENCE_TYPE_INT_TO_DB = {
    97:  'general',
    106: 'juvenile',
    117: 'unknown',
}

# Author function codes -> camelCase DB strings (migration 043)
# Integer codes from AuthorFunction enum + MARC relator codes
AUTHOR_FUNCTION_TO_DB = {
    # Integer codes
    '70':  'author',
    '440': 'illustrator',
    '730': 'translator',
    '695': 'scientificAdvisor',
    '340': 'scientificAdvisor',
    '80':  'prefaceWriter',
    '600': 'photographer',
    '651': 'publishingDirector',
    '650': 'publishingDirector',
    '230': 'composer',
    # MARC relator codes
    'aut': 'author',
    'ill': 'illustrator',
    'trl': 'translator',
    'edt': 'scientificAdvisor',
    'aui': 'prefaceWriter',
    'pht': 'photographer',
    'pbd': 'publishingDirector',
    'cmp': 'composer',
    # Already-canonical camelCase (idempotent)
    'author':             'author',
    'illustrator':        'illustrator',
    'translator':         'translator',
    'scientificAdvisor':  'scientificAdvisor',
    'prefaceWriter':      'prefaceWriter',
    'photographer':       'photographer',
    'publishingDirector': 'publishingDirector',
    'composer':           'composer',
}

TABLES_DROP_ORDER = [
    'audit_log',
    'loans_archives', 'loans', 'loans_settings',
    'biblio_authors',
    'biblio_series',
    'biblio_collections',
    'items',     # physical copies (formerly specimens)
    'biblios',   # bibliographic records (formerly items)
    'z3950servers', 'fees', 'users',
    'authors', 'editions', 'collections', 'series', 'sources',
    'public_type_loan_settings', 'public_types',
    'account_types',
    'visitor_counts',
    'schedule_slots', 'schedule_closures', 'schedule_periods',
    'equipment', 'events',
    'settings', 'library_info',
]


# =============================================================================
# HELPERS
# =============================================================================

def connect_db(url):
    """Connect to PostgreSQL database."""
    return psycopg2.connect(url)


def ts_to_datetime(value):
    """Convert a Unix timestamp (int) to a UTC datetime, or None."""
    if value is None or value == 0:
        return None
    if isinstance(value, datetime):
        return value
    try:
        numeric = int(value) if isinstance(value, str) else float(value)
        return datetime.fromtimestamp(numeric, tz=timezone.utc)
    except (ValueError, TypeError, OSError, OverflowError):
        return None


def hash_password(plain, hasher):
    """Hash a password with Argon2. Returns None for empty passwords."""
    if not plain:
        return None
    if plain.startswith('$argon2'):
        return plain  # already hashed
    return hasher.hash(plain)


def slug_from_name(name, fallback_id):
    """Generate a slug from a name string."""
    if not name:
        return f'item_{fallback_id}'
    slug = re.sub(r'[^a-z0-9]+', '_', name.lower().strip()).strip('_')
    return slug or f'item_{fallback_id}'


def batch_insert(cursor, conn, sql, rows, batch_size=500):
    """Execute INSERT statements in batches and commit."""
    for i in range(0, len(rows), batch_size):
        for row in rows[i:i + batch_size]:
            cursor.execute(sql, row)
        conn.commit()


def map_media_type(code):
    """Map legacy media_type code to camelCase DB string."""
    if code is None:
        return 'unknown'
    return MEDIA_TYPE_CODE_TO_DB.get(code, code if code else 'unknown')


def map_lang(value):
    """Map legacy integer or ISO-3 lang code to camelCase DB string."""
    if value is None:
        return None
    if isinstance(value, int):
        return LANG_INT_TO_DB.get(value)
    s = str(value).strip()
    if s.lstrip('-').isdigit():
        return LANG_INT_TO_DB.get(int(s))
    iso_map = {
        'fre': 'french', 'fra': 'french', 'eng': 'english',
        'ger': 'german', 'deu': 'german', 'jpn': 'japanese',
        'spa': 'spanish', 'por': 'portuguese',
    }
    return iso_map.get(s.lower(), s if s else None)


def map_audience_type(value):
    """Map legacy integer audience_type to camelCase DB string."""
    if value is None:
        return None
    if isinstance(value, int):
        return AUDIENCE_TYPE_INT_TO_DB.get(value, 'unknown')
    s = str(value).strip()
    if s.lstrip('-').isdigit():
        return AUDIENCE_TYPE_INT_TO_DB.get(int(s), 'unknown')
    return s if s else None


def map_author_function(value):
    """Map legacy author function code (int string or MARC relator) to camelCase DB string.

    Returns None for unrecognised values (migration 043 ELSE NULL behaviour).
    """
    if not value:
        return None
    s = str(value).strip()
    return AUTHOR_FUNCTION_TO_DB.get(s)


def legacy_fk_id(value):
    """Legacy DB used 0 as 'no reference'; PostgreSQL FKs must use NULL instead."""
    if value is None:
        return None
    try:
        v = int(value)
    except (TypeError, ValueError):
        return None
    return None if v <= 0 else v


def legacy_biblio_primary_title(title1, title2=None, title3=None, title4=None):
    """First non-empty title among legacy title1..title4 (strip whitespace)."""
    for t in (title1, title2, title3, title4):
        if t is None:
            continue
        s = t.strip() if isinstance(t, str) else str(t).strip()
        if s:
            return s
    return None


def legacy_biblio_missing_isbn_and_title(identification, title1, title2=None, title3=None, title4=None):
    """True when no usable title (title1-4) and no ISBN — skip biblio migration."""
    if legacy_biblio_primary_title(title1, title2, title3, title4) is not None:
        return False
    if identification is None:
        return True
    s = str(identification).strip()
    if not s:
        return True
    cleaned = re.sub(r'[^0-9X]', '', s, flags=re.IGNORECASE).upper()
    return not cleaned


# =============================================================================
# SCHEMA MANAGEMENT
# =============================================================================

def reset_target_database(conn):
    """Drop all tables and recreate schema from init_database.sql."""
    print("Resetting target database...")
    cur = conn.cursor()

    for table in TABLES_DROP_ORDER:
        cur.execute(f"DROP TABLE IF EXISTS {table} CASCADE")

    # Drop legacy FTS artifacts (safe no-ops if not present)
    cur.execute("DROP TRIGGER IF EXISTS items_search_vector_trigger ON items")
    cur.execute("DROP FUNCTION IF EXISTS items_search_vector_update() CASCADE")
    cur.execute("DROP FUNCTION IF EXISTS items_search_vector_trigger_fn() CASCADE")
    cur.execute("DROP FUNCTION IF EXISTS items_rebuild_search_vector(BIGINT) CASCADE")
    conn.commit()
    print("  Tables dropped")

    init_sql_path = Path(__file__).parent / 'init_database.sql'
    if not init_sql_path.exists():
        print(f"  ERROR: {init_sql_path} not found")
        sys.exit(1)

    sql = init_sql_path.read_text(encoding='utf-8')
    cur.execute(sql)
    conn.commit()
    print("  Schema recreated from init_database.sql")


# =============================================================================
# DATA MIGRATION FUNCTIONS
# =============================================================================

def migrate_account_types(src, dst):
    """Migrate account_types: id (int PK) -> code (slug PK)."""
    print("Migrating account_types...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    src_cur.execute("""
        SELECT id, name, items_rights, users_rights, loans_rights,
               items_archive_rights, borrows_rights, settings_rights
        FROM account_types
    """)

    count = 0
    for row in src_cur.fetchall():
        aid, name, ir, ur, lr, iar, br, sr = row
        code = ACCOUNT_TYPE_ID_TO_CODE.get(aid, f'type_{aid}')
        dst_cur.execute("""
            INSERT INTO account_types (code, name, items_rights, users_rights, loans_rights,
                                       items_archive_rights, borrows_rights, settings_rights)
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (code) DO UPDATE SET
                name = EXCLUDED.name, items_rights = EXCLUDED.items_rights,
                users_rights = EXCLUDED.users_rights, loans_rights = EXCLUDED.loans_rights,
                items_archive_rights = EXCLUDED.items_archive_rights,
                borrows_rights = EXCLUDED.borrows_rights, settings_rights = EXCLUDED.settings_rights
        """, (code, name, ir, ur, lr, iar, br, sr))
        count += 1

    dst.commit()
    print(f"  {count} account types migrated")


def migrate_fees(src, dst):
    """Migrate fees: id (int PK) + desc -> code (slug PK) + name."""
    print("Migrating fees...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    src_cur.execute('SELECT id, "desc", amount FROM fees')

    count = 0
    for fee_id, desc, amount in src_cur.fetchall():
        code = FEE_ID_TO_CODE.get(fee_id, slug_from_name(desc, fee_id))
        dst_cur.execute("""
            INSERT INTO fees (code, name, amount)
            VALUES (%s, %s, %s)
            ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name, amount = EXCLUDED.amount
        """, (code, desc, amount))
        count += 1

    dst.commit()
    print(f"  {count} fees migrated")


def normalize_birthdate(val):
    """Map legacy birthdate to datetime.date or None (aligned with migration 009 rules)."""
    if val is None:
        return None
    if isinstance(val, date) and not isinstance(val, datetime):
        return val
    if isinstance(val, datetime):
        return val.date()
    s = str(val).strip()
    if not s:
        return None
    if len(s) == 10 and s[4] == "-" and s[7] == "-":
        try:
            return datetime.strptime(s, "%Y-%m-%d").date()
        except ValueError:
            return None
    if s.isdigit():
        if len(s) == 8:
            try:
                return datetime.strptime(s, "%Y%m%d").date()
            except ValueError:
                return None
        if len(s) == 6:
            yy = int(s[0:2])
            mm = int(s[2:4])
            dd = int(s[4:6])
            year = 2000 + yy if yy <= 30 else 1900 + yy
            try:
                return date(year, mm, dd)
            except ValueError:
                return None
    return None


def migrate_users(src, dst, hash_passwords=True):
    """Migrate users with password hashing and schema transformations.

    Source columns: id, login, password, firstname, lastname, email,
        addr_street, addr_zip_code, addr_city, phone,
        sex_id -> sex (SMALLINT preserved),
        account_type_id -> account_type (slug),
        subscription_type_id (dropped), fee_id -> fee (slug),
        last_payement_date (dropped), group_id, barcode, notes,
        occupation (dropped), crea_date/modif_date/issue_date (int) -> timestamptz,
        profession (dropped), birthdate -> DATE (normalized), archived_date (int) -> timestamptz,
        public_type (int 97/106/117) -> FK to public_types

    Target adds: status, language, 2FA fields (defaults), must_change_password
    """
    print("Migrating users...")

    hasher = None
    if hash_passwords:
        if not ARGON2_AVAILABLE:
            print("  ERROR: argon2-cffi required. Install with: pip install argon2-cffi")
            sys.exit(1)
        hasher = PasswordHasher()
        print("  Password hashing enabled (Argon2)")

    src_cur = src.cursor()
    dst_cur = dst.cursor()

    # Load public_types id mapping from target DB (name -> id)
    dst_cur.execute("SELECT id, name FROM public_types")
    pt_name_to_id = {name: pid for pid, name in dst_cur.fetchall()}

    src_cur.execute("""
        SELECT id, login, password, firstname, lastname, email,
               addr_street, addr_zip_code, addr_city, phone,
               account_type_id, fee_id, group_id, barcode,
               notes, crea_date, modif_date, issue_date,
               birthdate, archived_date, public_type, sex_id
        FROM users
    """)
    rows = src_cur.fetchall()

    # Build unique login map
    login_counts = {}
    for row in rows:
        login = (row[1] or '').strip() or f'user_{row[0]}'
        key = login.lower()
        login_counts.setdefault(key, []).append((row[0], login))

    unique_logins = {}
    for key, entries in login_counts.items():
        if len(entries) == 1:
            unique_logins[entries[0][0]] = entries[0][1]
        else:
            for uid, login in entries:
                unique_logins[uid] = f'{login}_{uid}'

    hashed_count = 0
    migrated = 0

    for row in rows:
        (uid, _, raw_pw, firstname, lastname, email,
         addr_street, addr_zip_code, addr_city, phone,
         account_type_id, fee_id, group_id, barcode,
         notes, crea_date, modif_date, issue_date,
         birthdate, archived_date, public_type_raw, sex_id_raw) = row

        login = unique_logins.get(uid, f'user_{uid}')
        email = email.strip() if email and email.strip() else None

        # UNIQUE(barcode): empty string would collide for many rows; store NULL instead
        if barcode is None or (isinstance(barcode, str) and not barcode.strip()):
            barcode = None
        elif isinstance(barcode, str):
            barcode = barcode.strip()

        password = None
        if hash_passwords and raw_pw:
            password = hash_password(raw_pw, hasher)
            if password != raw_pw:
                hashed_count += 1

        account_type = ACCOUNT_TYPE_ID_TO_CODE.get(account_type_id, 'guest')
        fee = FEE_ID_TO_CODE.get(fee_id) if fee_id else None

        crea_dt = ts_to_datetime(crea_date)
        modif_dt = ts_to_datetime(modif_date)
        issue_dt = ts_to_datetime(issue_date)
        archived_dt = ts_to_datetime(archived_date)

        status = "deleted" if archived_dt else "active"

        birthdate_db = normalize_birthdate(birthdate)

        # Map legacy integer public_type to FK id in target
        pt_id = None
        if public_type_raw is not None:
            pt_name = PUBLIC_TYPE_INT_TO_NAME.get(int(public_type_raw))
            if pt_name:
                pt_id = pt_name_to_id.get(pt_name)

        dst_cur.execute("""
            INSERT INTO users (
                id, login, password, firstname, lastname, email,
                addr_street, addr_zip_code, addr_city, phone,
                account_type, fee, group_id, barcode, notes,
                public_type, status, birthdate,
                created_at, update_at, expiry_at, archived_at,
                language, sex
            ) VALUES (
                %s, %s, %s, %s, %s, %s,
                %s, %s, %s, %s,
                %s, %s, %s, %s, %s,
                %s, %s, %s,
                %s, %s, %s, %s,
                'french', %s
            ) ON CONFLICT (id) DO UPDATE SET
                login = EXCLUDED.login,
                password = EXCLUDED.password,
                firstname = EXCLUDED.firstname,
                lastname = EXCLUDED.lastname
        """, (
            uid, login, password, firstname, lastname, email,
            addr_street, addr_zip_code, addr_city, phone,
            account_type, fee, group_id, barcode, notes,
            pt_id, status, birthdate_db,
            crea_dt, modif_dt, issue_dt, archived_dt,
            sex_id_raw,
        ))
        migrated += 1

    dst.commit()
    parts = [f"{migrated} users migrated"]
    if hash_passwords:
        parts.append(f"{hashed_count} passwords hashed")
    print(f"  {', '.join(parts)}")


def migrate_simple_table(src, dst, table, columns, conflict_col="id"):
    """Migrate a table with identical source/target structure."""
    print(f"Migrating {table}...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    cols_str = ", ".join(columns)
    placeholders = ", ".join(["%s"] * len(columns))

    src_cur.execute(f"SELECT {cols_str} FROM {table}")
    rows = src_cur.fetchall()

    for row in rows:
        dst_cur.execute(f"""
            INSERT INTO {table} ({cols_str})
            VALUES ({placeholders})
            ON CONFLICT ({conflict_col}) DO NOTHING
        """, row)

    dst.commit()
    print(f"  {len(rows)} {table} migrated")


def migrate_editions(src, dst):
    """Migrate editions: name -> publisher_name, place -> place_of_publication."""
    print("Migrating editions...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    src_cur.execute("SELECT id, key, name, place, notes FROM editions")
    rows = src_cur.fetchall()

    for eid, key, name, place, notes in rows:
        dst_cur.execute("""
            INSERT INTO editions (id, key, publisher_name, place_of_publication, notes)
            VALUES (%s, %s, %s, %s, %s)
            ON CONFLICT (id) DO NOTHING
        """, (eid, key, name, place, notes))

    dst.commit()
    print(f"  {len(rows)} editions migrated")


def migrate_collections(src, dst):
    """Migrate collections: title1 -> name (main display title), title2/3 -> secondary/tertiary_title."""
    print("Migrating collections...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    src_cur.execute("SELECT id, key, title1, title2, title3, issn FROM collections")
    rows = src_cur.fetchall()

    for cid, key, t1, t2, t3, issn in rows:
        # name (formerly primary_title) must be non-null; fall back to key or generated value
        name = t1 or key or f"Collection {cid}"
        dst_cur.execute("""
            INSERT INTO collections (id, key, name, secondary_title, tertiary_title, issn)
            VALUES (%s, %s, %s, %s, %s, %s)
            ON CONFLICT (id) DO NOTHING
        """, (cid, key, name, t2, t3, issn))

    dst.commit()
    print(f"  {len(rows)} collections migrated")


def migrate_items(src, dst):
    """Migrate items from legacy schema to new MARC-aligned schema.

    Source columns (legacy):
        id, media_type (code), identification (isbn), price, barcode, dewey,
        publication_date, lang (int), lang_orig (int), title1-4,
        author1_ids[], author1_functions, author2_ids[], author2_functions,
        author3_ids[], author3_functions, serie_id, serie_vol_number,
        collection_id, collection_number_sub, collection_vol_number,
        source_id, genre, subject, public_type (int),
        edition_id, edition_date, nb_pages, format, content, addon,
        abstract, notes, keywords, nb_specimens, state,
        is_archive, archived_timestamp, is_valid, created_at, update_at

    Target columns (new schema):
        id, media_type (camelCase), isbn, title, subject, audience_type (camelCase),
        lang (camelCase), lang_orig (camelCase), publication_date,
        collection_id, collection_sequence_number,
        collection_volume_number, source_id, edition_id, page_extent, format,
        table_of_contents, accompanying_material, abstract, notes, keywords (array),
        is_valid, created_at, updated_at, archived_at
    Legacy serie_id / serie_vol_number -> item_series junction (migration 044).

    Rows with both empty/null `identification` (ISBN) and no text in `title1`..`title4` are skipped.
    `title` is filled from the first non-empty of title1, title2, title3, title4.
    """
    print("Migrating items...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    src_cur.execute("SELECT COUNT(*) FROM items")
    total = src_cur.fetchone()[0]

    BATCH = 1000
    offset = 0
    item_authors_batch = []
    item_series_batch = []
    item_collections_batch = []
    skipped_biblio_ids = set()
    skipped_biblios = 0

    while offset < total:
        src_cur.execute(f"""
            SELECT id, media_type, identification, publication_date,
                   lang, lang_orig, title1, title2, title3, title4,
                   author1_ids, author1_functions,
                   author2_ids, author2_functions,
                   author3_ids, author3_functions,
                   serie_id, serie_vol_number,
                   collection_id, collection_number_sub, collection_vol_number,
                   source_id, subject, public_type,
                   edition_id, nb_pages, format, content, addon,
                   abstract, notes, keywords, is_valid,
                   is_archive, archived_timestamp, crea_date, modif_date
            FROM items ORDER BY id LIMIT {BATCH} OFFSET {offset}
        """)

        for row in src_cur.fetchall():
            (iid, media_type_raw, identification, publication_date,
             lang_raw, lang_orig_raw, title1, title2, title3, title4,
             author1_ids, author1_functions,
             author2_ids, author2_functions,
             author3_ids, author3_functions,
             serie_id, serie_vol_number,
             collection_id, collection_number_sub, collection_vol_number,
             source_id, subject, public_type_raw,
             edition_id, nb_pages, fmt, content, addon,
             abstract_, notes, keywords, is_valid,
             is_archive, archived_timestamp, crea_date_raw, modif_date_raw) = row

            if legacy_biblio_missing_isbn_and_title(
                identification, title1, title2, title3, title4
            ):
                skipped_biblio_ids.add(iid)
                skipped_biblios += 1
                continue

            if identification is None:
                isbn_norm = None
            else:
                isbn_norm = re.sub(r'[^0-9X]', '', str(identification).strip(), flags=re.IGNORECASE).upper()
                isbn_norm = isbn_norm or None

            title_for_biblio = legacy_biblio_primary_title(title1, title2, title3, title4)

            media_type = map_media_type(media_type_raw)
            lang = map_lang(lang_raw)
            lang_orig = map_lang(lang_orig_raw)
            audience_type = map_audience_type(public_type_raw)

            archived_ts = ts_to_datetime(archived_timestamp)
            crea_dt = ts_to_datetime(crea_date_raw)
            modif_dt = ts_to_datetime(modif_date_raw)

            archived_at = archived_ts if (is_archive == 1) else None

            source_id = legacy_fk_id(source_id)
            edition_id = legacy_fk_id(edition_id)
            serie_fk = legacy_fk_id(serie_id)
            collection_fk = legacy_fk_id(collection_id)

            # keywords: legacy may be a comma-separated string
            keywords_arr = None
            if keywords:
                if isinstance(keywords, list):
                    keywords_arr = [k.strip() for k in keywords if k and k.strip()]
                elif isinstance(keywords, str) and keywords.strip():
                    keywords_arr = [k.strip() for k in re.split(r'\s*,\s*', keywords) if k.strip()]

            dst_cur.execute("""
                INSERT INTO biblios (
                    id, media_type, isbn, title, subject, audience_type,
                    lang, lang_orig, publication_date,
                    source_id, edition_id, page_extent, format,
                    table_of_contents, accompanying_material, abstract, notes,
                    keywords, is_valid,
                    created_at, updated_at, archived_at
                ) VALUES (
                    %s,%s,%s,%s,%s,%s,
                    %s,%s,%s,
                    %s,%s,%s,%s,
                    %s,%s,%s,%s,
                    %s,%s,
                    %s,%s,%s
                ) ON CONFLICT (id) DO NOTHING
            """, (
                iid, media_type, isbn_norm, title_for_biblio, subject, audience_type,
                lang, lang_orig, publication_date,
                source_id, edition_id, nb_pages, fmt,
                content, addon, abstract_, notes,
                keywords_arr, is_valid,
                crea_dt, modif_dt, archived_at,
            ))

            if serie_fk is not None:
                item_series_batch.append((iid, serie_fk, 1, serie_vol_number))

            if collection_fk is not None:
                item_collections_batch.append((iid, collection_fk, 1, collection_vol_number))

            # Collect item_authors entries from legacy author arrays
            position = 1
            for ids_arr, funcs_str in [
                (author1_ids, author1_functions),
                (author2_ids, author2_functions),
                (author3_ids, author3_functions),
            ]:
                if not ids_arr:
                    continue
                func_list = [f.strip() for f in (funcs_str or '').split(',') if f.strip()]
                for idx, author_id in enumerate(ids_arr):
                    aid = legacy_fk_id(author_id)
                    if aid is None:
                        continue
                    raw_func = func_list[idx] if idx < len(func_list) else None
                    function = map_author_function(raw_func)
                    item_authors_batch.append((iid, aid, function, 0, position))
                    position += 1

        dst.commit()
        offset += BATCH
        print(f"  {min(offset, total)}/{total} items")

    print(
        f"  {total - skipped_biblios} biblios migrated, "
        f"{skipped_biblios} skipped (no ISBN and no title in title1-4)"
    )

    print("  Migrating biblio_series...")
    for i in range(0, len(item_series_batch), 500):
        for entry in item_series_batch[i:i + 500]:
            try:
                dst_cur.execute("""
                    INSERT INTO biblio_series (biblio_id, series_id, position, volume_number)
                    VALUES (%s, %s, %s, %s)
                    ON CONFLICT (biblio_id, series_id) DO NOTHING
                """, entry)
            except Exception:
                dst.rollback()
        dst.commit()
    print(f"  {len(item_series_batch)} biblio_series rows migrated")

    print("  Migrating biblio_collections...")
    for i in range(0, len(item_collections_batch), 500):
        for entry in item_collections_batch[i:i + 500]:
            try:
                dst_cur.execute("""
                    INSERT INTO biblio_collections (biblio_id, collection_id, position, volume_number)
                    VALUES (%s, %s, %s, %s)
                    ON CONFLICT (biblio_id, collection_id) DO NOTHING
                """, entry)
            except Exception:
                dst.rollback()
        dst.commit()
    print(f"  {len(item_collections_batch)} biblio_collections rows migrated")

    # Insert biblio_authors in batches
    print("  Migrating biblio_authors...")
    ia_count = 0
    for i in range(0, len(item_authors_batch), 500):
        for entry in item_authors_batch[i:i + 500]:
            try:
                dst_cur.execute("""
                    INSERT INTO biblio_authors (biblio_id, author_id, function, author_type, position)
                    VALUES (%s, %s, %s, %s, %s)
                    ON CONFLICT (biblio_id, author_id, function) DO NOTHING
                """, entry)
                ia_count += 1
            except Exception:
                dst.rollback()
        dst.commit()
    print(f"  {ia_count} biblio_authors migrated")

    return skipped_biblio_ids


def migrate_specimens(src, dst, skipped_biblio_ids=None):
    """Migrate specimens (physical copies) into the new `items` table.

    Source columns (legacy specimens):
        id, id_item, source_id, identification (barcode), cote (call_number),
        place, status (borrow_status: 98=borrowable, 110=not), codestat (circulation_status),
        notes, price, modif_date (int), is_archive, archive_date (int), crea_date (int)

    Target columns (items table):
        id, biblio_id, source_id, barcode, call_number, place,
        borrowable (bool), circulation_status, notes, price,
        updated_at (timestamptz), archived_at (timestamptz), created_at (timestamptz)

    Barcode: empty/whitespace -> NULL. Duplicate non-null barcodes (legacy data): keep first
    by specimen id, set later rows to NULL to satisfy idx_items_barcode_unique.

    Specimens whose legacy `id_item` points to a skipped biblio (no ISBN + no title) are not migrated.
    """
    print("Migrating specimens -> items...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    if skipped_biblio_ids is None:
        skipped_biblio_ids = set()

    src_cur.execute("SELECT COUNT(*) FROM specimens")
    total = src_cur.fetchone()[0]

    # UNIQUE(barcode) WHERE barcode IS NOT NULL: empty strings and duplicates must become NULL
    seen_barcodes = set()
    duplicate_barcode_dropped = 0
    skipped_for_skipped_biblio = 0
    migrated_specimen_ids = set()

    BATCH = 1000
    offset = 0

    while offset < total:
        src_cur.execute(f"""
            SELECT id, id_item, source_id, identification, cote, place,
                   status, codestat, notes, price, modif_date,
                   is_archive, archive_date, crea_date
            FROM specimens ORDER BY id LIMIT {BATCH} OFFSET {offset}
        """)

        for row in src_cur.fetchall():
            (sid, id_item, source_id, identification, cote, place,
             borrow_status, codestat, notes, price, modif_date_raw,
             is_archive, archive_date_raw, crea_date_raw) = row

            legacy_bid = None
            if id_item is not None:
                try:
                    legacy_bid = int(id_item)
                except (TypeError, ValueError):
                    legacy_bid = None
            if legacy_bid is not None and legacy_bid in skipped_biblio_ids:
                skipped_for_skipped_biblio += 1
                continue

            # 98 = borrowable, 110 = not borrowable; NULL defaults to True
            borrowable = True
            if borrow_status == 110:
                borrowable = False
            elif borrow_status == 98:
                borrowable = True

            updated_dt = ts_to_datetime(modif_date_raw)
            archived_dt = ts_to_datetime(archive_date_raw)
            created_dt = ts_to_datetime(crea_date_raw)

            # is_archive=1 means archived; ensure archived_at is set
            if is_archive == 1 and archived_dt is None:
                archived_dt = datetime.now(tz=timezone.utc)

            biblio_id = legacy_fk_id(id_item)
            source_id = legacy_fk_id(source_id)

            if identification is None:
                barcode = None
            elif isinstance(identification, str):
                barcode = identification.strip() or None
            else:
                barcode = str(identification).strip() or None

            if barcode is not None:
                if barcode in seen_barcodes:
                    barcode = None
                    duplicate_barcode_dropped += 1
                else:
                    seen_barcodes.add(barcode)


            dst_cur.execute("""
                INSERT INTO items (
                    id, biblio_id, source_id, barcode, call_number, place,
                    borrowable, circulation_status, notes, price,
                    updated_at, archived_at, created_at
                ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
                ON CONFLICT (id) DO NOTHING
            """, (
                sid, biblio_id, source_id, barcode, cote, place,
                borrowable, codestat, notes, price,
                updated_dt, archived_dt, created_dt,
            ))
            migrated_specimen_ids.add(sid)

        dst.commit()
        offset += BATCH
        print(f"  {min(offset, total)}/{total} items (physical copies)")

    print(
        f"  {len(migrated_specimen_ids)} items (physical copies) migrated"
        + (f", {skipped_for_skipped_biblio} skipped (biblio not migrated)" if skipped_for_skipped_biblio else "")
    )
    if duplicate_barcode_dropped:
        print(
            f"  {duplicate_barcode_dropped} duplicate barcodes cleared (NULL); "
            "first specimen by id kept each barcode (idx_items_barcode_unique)"
        )

    return migrated_specimen_ids


def migrate_loans(src, dst, migrated_specimen_ids=None):
    """Migrate borrows -> loans + loans_archives.

    Legacy borrows columns: id, user_id, specimen_id, date (int), renew_date (int),
        nb_renews, issue_date (int), notes, returned_date (int), item_id
    specimen_id -> item_id (new items = former specimens)

    Active loans (returned_date IS NULL or 0) -> loans table
    Returned loans (returned_date set) -> loans_archives with user enrichment
    """
    print("Migrating borrows -> loans/loans_archives...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    # Load user info for archive enrichment
    src_cur.execute("SELECT id, addr_city, account_type_id, public_type FROM users")
    user_info = {}
    for uid, city, at_id, pt in src_cur.fetchall():
        at_code = ACCOUNT_TYPE_ID_TO_CODE.get(at_id, 'guest') if at_id else 'guest'
        user_info[uid] = (city, at_code, pt)

    # Load public_type FK mapping from target
    dst_cur.execute("SELECT id, name FROM public_types")
    pt_name_to_id = {name: pid for pid, name in dst_cur.fetchall()}

    src_cur.execute("""
        SELECT id, user_id, specimen_id, item_id, date, renew_date,
               nb_renews, issue_date, notes, returned_date
        FROM borrows
    """)
    borrows = src_cur.fetchall()

    active = 0
    archived = 0
    skipped = 0

    for row in borrows:
        vals = list(row)
        user_id = vals[1]
        item_id = legacy_fk_id(vals[2])  # legacy specimen_id -> items.id; 0 means no copy
        returned_date_raw = vals[9]

        vals[4] = ts_to_datetime(vals[4])   # date -> timestamptz
        vals[5] = ts_to_datetime(vals[5])   # renew_date -> renew_at
        vals[7] = ts_to_datetime(vals[7])   # issue_date -> expiry_at
        vals[9] = ts_to_datetime(vals[9])   # returned_date -> returned_at

        if item_id is None:
            skipped += 1
            continue
        if migrated_specimen_ids is not None and item_id not in migrated_specimen_ids:
            skipped += 1
            continue

        if returned_date_raw and returned_date_raw != 0:
            city, at_code, pt_raw = user_info.get(user_id, (None, 'guest', None))

            pt_id = None
            if pt_raw is not None:
                pt_name = PUBLIC_TYPE_INT_TO_NAME.get(int(pt_raw))
                if pt_name:
                    pt_id = pt_name_to_id.get(pt_name)

            dst_cur.execute("""
                INSERT INTO loans_archives (
                    id, user_id, item_id, date, nb_renews,
                    expiry_at, returned_at, notes,
                    borrower_public_type, addr_city, account_type
                ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
                ON CONFLICT (id) DO NOTHING
            """, (
                vals[0], user_id, item_id,
                vals[4], vals[6], vals[7], vals[9], vals[8],
                pt_id, city, at_code,
            ))
            archived += 1
        else:
            dst_cur.execute("""
                INSERT INTO loans (
                    id, user_id, item_id, date, renew_at,
                    nb_renews, expiry_at, notes, returned_at
                ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s)
                ON CONFLICT (id) DO NOTHING
            """, (vals[0], vals[1], item_id, vals[4], vals[5], vals[6], vals[7], vals[8], vals[9]))
            active += 1

    dst.commit()
    print(f"  {len(borrows)} borrows processed: {active} active loans, {archived} archived")
    if skipped:
        print(
            f"  {skipped} skipped (invalid specimen_id, or item not migrated "
            f"e.g. skipped biblio / specimen)"
        )


def migrate_loans_archives(src, dst, migrated_specimen_ids=None):
    """Migrate borrows_archives -> loans_archives.

    Legacy columns: id, item_id, date (int), nb_renews, issue_date (int),
        returned_date (int), notes, specimen_id, borrower_public_type,
        occupation (dropped), addr_city, sex_id (dropped), account_type_id
    specimen_id -> item_id (new items = former specimens)
    account_type_id -> account_type (slug), no user_id in source
    """
    print("Migrating borrows_archives -> loans_archives...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    # Load public_type FK mapping from target
    dst_cur.execute("SELECT id, name FROM public_types")
    pt_name_to_id = {name: pid for pid, name in dst_cur.fetchall()}

    src_cur.execute("""
        SELECT id, item_id, specimen_id, date, nb_renews, issue_date,
               returned_date, notes, borrower_public_type,
               addr_city, account_type_id
        FROM borrows_archives
    """)
    rows = src_cur.fetchall()

    migrated = 0
    skipped = 0

    for row in rows:
        vals = list(row)
        item_id = legacy_fk_id(vals[2])  # legacy specimen_id -> items.id; 0 means no copy
        at_id = vals[10]
        pt_raw = vals[8]

        if item_id is None:
            skipped += 1
            continue
        if migrated_specimen_ids is not None and item_id not in migrated_specimen_ids:
            skipped += 1
            continue

        date_val = ts_to_datetime(vals[3])
        issue_dt = ts_to_datetime(vals[5])
        returned_dt = ts_to_datetime(vals[6])
        at_code = ACCOUNT_TYPE_ID_TO_CODE.get(at_id, 'guest') if at_id else 'guest'

        pt_id = None
        if pt_raw is not None:
            pt_name = PUBLIC_TYPE_INT_TO_NAME.get(int(pt_raw))
            if pt_name:
                pt_id = pt_name_to_id.get(pt_name)

        dst_cur.execute("""
            INSERT INTO loans_archives (
                id, item_id, date, nb_renews, expiry_at,
                returned_at, notes, borrower_public_type,
                addr_city, account_type
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
            ON CONFLICT (id) DO NOTHING
        """, (
            vals[0], item_id, date_val, vals[4], issue_dt,
            returned_dt, vals[7], pt_id, vals[9], at_code,
        ))
        migrated += 1

    dst.commit()
    print(f"  {migrated}/{len(rows)} loans_archives migrated")
    if skipped:
        print(
            f"  {skipped} skipped (invalid specimen_id, or item not migrated "
            f"e.g. skipped biblio / specimen)"
        )


def migrate_loans_settings(src, dst):
    """Migrate borrows_settings -> loans_settings: account_type_id -> account_type slug."""
    print("Migrating borrows_settings -> loans_settings...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    src_cur.execute("""
        SELECT id, media_type, nb_max, nb_renews, duration, notes, account_type_id
        FROM borrows_settings
    """)
    rows = src_cur.fetchall()

    for row in rows:
        sid, media_type, nb_max, nb_renews, duration, notes, at_id = row
        media_type_db = map_media_type(media_type)
        at_code = ACCOUNT_TYPE_ID_TO_CODE.get(at_id) if at_id else None

        dst_cur.execute("""
            INSERT INTO loans_settings (id, media_type, nb_max, nb_renews, duration, notes, account_type)
            VALUES (%s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (media_type) DO UPDATE SET
                nb_max = EXCLUDED.nb_max, nb_renews = EXCLUDED.nb_renews,
                duration = EXCLUDED.duration, notes = EXCLUDED.notes,
                account_type = EXCLUDED.account_type
        """, (sid, media_type_db, nb_max, nb_renews, duration, notes, at_code))

    dst.commit()
    print(f"  {len(rows)} loans_settings migrated")


def migrate_z3950servers(src, dst):
    """Migrate z3950servers: activated int->bool, add encoding (default utf-8)."""
    print("Migrating z3950servers...")
    src_cur = src.cursor()
    dst_cur = dst.cursor()

    src_cur.execute("""
        SELECT id, address, port, name, description, activated,
               login, password, database, format
        FROM z3950servers
    """)
    rows = src_cur.fetchall()

    for row in rows:
        (zid, address, port, name, description, activated_raw,
         login, password, database, fmt) = row
        activated = bool(activated_raw) if activated_raw is not None else True

        dst_cur.execute("""
            INSERT INTO z3950servers (
                id, address, port, name, description, activated,
                login, password, database, format, encoding
            ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s, 'utf-8')
            ON CONFLICT (id) DO NOTHING
        """, (zid, address, port, name, description, activated,
              login, password, database, fmt))

    dst.commit()
    print(f"  {len(rows)} z3950servers migrated")


def reset_sequences(conn):
    """Reset all sequences to their max ID value."""
    print("Resetting sequences...")
    cur = conn.cursor()

    tables = [
        'users', 'authors', 'editions', 'collections', 'series', 'sources',
        'biblios', 'biblio_authors', 'biblio_series', 'biblio_collections', 'items',
        'loans', 'loans_archives', 'loans_settings', 'z3950servers',
        'public_types', 'public_type_loan_settings',
        'visitor_counts', 'schedule_periods', 'schedule_slots', 'schedule_closures',
        'equipment', 'events', 'audit_log',
    ]

    for table in tables:
        try:
            cur.execute(
                f"SELECT setval('{table}_id_seq', "
                f"COALESCE((SELECT MAX(id) FROM {table}), 1), true)"
            )
        except Exception:
            conn.rollback()

    conn.commit()
    print("  Sequences reset")


# =============================================================================
# MAIN
# =============================================================================

def main():
    parser = argparse.ArgumentParser(
        description='Migrate Elidune data from legacy to new schema',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Full migration with reset (recommended for fresh target)
  python migrate_data.py --source-db postgres://user:pass@host/old --target-db postgres://user:pass@host/new --reset -y

  # Incremental migration (target schema must already exist)
  python migrate_data.py --source-db <old> --target-db <new>

  # Skip specific tables
  python migrate_data.py --source-db <old> --target-db <new> --skip-items --skip-users
""",
    )
    parser.add_argument('--source-db', required=True, help='Source database URL')
    parser.add_argument('--target-db', required=True, help='Target database URL')
    parser.add_argument('--reset', action='store_true', help='Drop and recreate target schema before migration')
    parser.add_argument('--skip-items', action='store_true', help='Skip items/specimens migration')
    parser.add_argument('--skip-users', action='store_true', help='Skip users migration')
    parser.add_argument('--no-hash', action='store_true', help='Skip password hashing (migrate plaintext)')
    parser.add_argument('-y', '--yes', action='store_true', help='Skip confirmation prompts')
    args = parser.parse_args()

    hash_passwords = not args.no_hash

    print()
    print("=" * 60)
    print("  Elidune Data Migration")
    print("=" * 60)
    print()
    print(f"Source: {args.source_db}")
    print(f"Target: {args.target_db}")
    print(f"Options: reset={args.reset}, hash={hash_passwords}, "
          f"skip_items={args.skip_items}, skip_users={args.skip_users}")
    print()

    if hash_passwords and not ARGON2_AVAILABLE:
        print("ERROR: argon2-cffi required for password hashing.")
        print("Install with: pip install argon2-cffi")
        print("Or use --no-hash to skip password hashing.")
        sys.exit(1)

    if args.reset and not args.yes:
        print("WARNING: --reset will DELETE ALL DATA in the target database.")
        if input("Type 'yes' to confirm: ").strip().lower() != 'yes':
            print("Cancelled.")
            sys.exit(0)

    try:
        src = connect_db(args.source_db)
        dst = connect_db(args.target_db)
        print("Connected to databases")
        print()

        if args.reset:
            reset_target_database(dst)
            print()

        # Migrate in dependency order
        migrate_account_types(src, dst)
        migrate_fees(src, dst)

        if not args.skip_users:
            migrate_users(src, dst, hash_passwords=hash_passwords)

        migrate_simple_table(src, dst, 'authors', ['id', 'key', 'lastname', 'firstname', 'bio', 'notes'])
        migrate_editions(src, dst)
        migrate_collections(src, dst)
        migrate_simple_table(src, dst, 'series', ['id', 'key', 'name'])
        migrate_simple_table(src, dst, 'sources', ['id', 'key', 'name'])

        migrated_specimen_ids = None
        if not args.skip_items:
            skipped_biblio_ids = migrate_items(src, dst)
            migrated_specimen_ids = migrate_specimens(src, dst, skipped_biblio_ids)

        migrate_loans(src, dst, migrated_specimen_ids)
        migrate_loans_archives(src, dst, migrated_specimen_ids)
        migrate_loans_settings(src, dst)
        migrate_z3950servers(src, dst)

        reset_sequences(dst)

        print()
        print("=" * 60)
        print("  Migration completed successfully!")
        print("=" * 60)
        if hash_passwords:
            print("  Passwords hashed with Argon2 - users can log in with original passwords.")

    except Exception as e:
        print(f"\nERROR: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
    finally:
        for conn_name in ('src', 'dst'):
            if conn_name in locals():
                locals()[conn_name].close()


if __name__ == '__main__':
    main()
