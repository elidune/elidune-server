-- Shared stats templates: active patrons, registrations by year, active borrowers, loans by dimensions.
-- Requires schema stats fields (age_band_3, active_membership_calendar_year, sources join) from application code.

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Patrons — active (current year) by age, sex, city',
    'Active patrons: membership valid for the current calendar year (no expiry or expiry on/after Jan 1), not deleted. Cross-tab by age band (0–14, 15–64, 65+), sex (male/female/unknown), and city.',
    $$
    {
      "entity": "users",
      "joins": [],
      "select": [
        {"field": "users.age_band_3", "alias": "ageBand"},
        {"field": "users.sex_label", "alias": "sex"},
        {"field": "users.addr_city", "alias": "city"}
      ],
      "filters": [
        {"field": "users.active_membership_calendar_year", "op": "eq", "value": "yes"}
      ],
      "filterGroups": [
        [{"field": "users.status", "op": "isNull", "value": null}],
        [{"field": "users.status", "op": "neq", "value": "deleted"}]
      ],
      "aggregations": [
        {"fn": "countDistinct", "field": "users.id", "alias": "patronCount"}
      ],
      "groupBy": [
        {"field": "users.age_band_3"},
        {"field": "users.sex_label"},
        {"field": "users.addr_city"}
      ],
      "having": [],
      "orderBy": [{"field": "patronCount", "dir": "desc"}],
      "limit": 5000,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Patrons — registrations by year, age, sex, city',
    'All non-deleted patrons grouped by registration year. Cross-tab by age band (0–14, 15–64, 65+), sex, and city.',
    $$
    {
      "entity": "users",
      "joins": [],
      "select": [
        {"field": "users.age_band_3", "alias": "ageBand"},
        {"field": "users.sex_label", "alias": "sex"},
        {"field": "users.addr_city", "alias": "city"}
      ],
      "filters": [],
      "filterGroups": [
        [{"field": "users.status", "op": "isNull", "value": null}],
        [{"field": "users.status", "op": "neq", "value": "deleted"}]
      ],
      "aggregations": [
        {"fn": "count", "field": "users.id", "alias": "patronCount"}
      ],
      "groupBy": [
        {"field": "users.age_band_3"},
        {"field": "users.sex_label"},
        {"field": "users.addr_city"}
      ],
      "having": [],
      "timeBucket": {"field": "users.created_at", "granularity": "year", "alias": "registrationYear"},
      "orderBy": [{"field": "registrationYear", "dir": "desc"}],
      "limit": 10000,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Borrowers — distinct per loan year, age, sex, city',
    'Distinct borrowers per calendar year of loan (loan date), with borrower age band, sex, and city. Excludes deleted users.',
    $$
    {
      "entity": "loans",
      "joins": ["users"],
      "select": [
        {"field": "users.age_band_3", "alias": "ageBand"},
        {"field": "users.sex_label", "alias": "sex"},
        {"field": "users.addr_city", "alias": "city"}
      ],
      "filters": [],
      "filterGroups": [
        [{"field": "users.status", "op": "isNull", "value": null}],
        [{"field": "users.status", "op": "neq", "value": "deleted"}]
      ],
      "aggregations": [
        {"fn": "countDistinct", "field": "loans.user_id", "alias": "distinctBorrowers"}
      ],
      "groupBy": [
        {"field": "users.age_band_3"},
        {"field": "users.sex_label"},
        {"field": "users.addr_city"}
      ],
      "having": [],
      "timeBucket": {"field": "loans.date", "granularity": "year", "alias": "loanYear"},
      "orderBy": [{"field": "loanYear", "dir": "desc"}],
      "limit": 10000,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Loans — by year, media type, audience, item source',
    'Loan counts per calendar year: biblio media type, biblio audience_type (e.g. adult/child), and catalog source of the borrowed item copy.',
    $$
    {
      "entity": "loans",
      "joins": ["items", "items.biblios", "items.sources"],
      "select": [
        {"field": "biblios.media_type", "alias": "mediaType"},
        {"field": "biblios.audience_type", "alias": "audienceType"},
        {"field": "sources.name", "alias": "itemSource"}
      ],
      "filters": [],
      "filterGroups": [],
      "aggregations": [
        {"fn": "count", "field": "loans.id", "alias": "loanCount"}
      ],
      "groupBy": [
        {"field": "biblios.media_type"},
        {"field": "biblios.audience_type"},
        {"field": "sources.name"}
      ],
      "having": [],
      "timeBucket": {"field": "loans.date", "granularity": "year", "alias": "loanYear"},
      "orderBy": [{"field": "loanCount", "dir": "desc"}],
      "limit": 10000,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);
