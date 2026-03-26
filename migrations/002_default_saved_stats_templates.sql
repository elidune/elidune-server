-- Default shared stats templates (registrations / loans style).
-- Reference exports (PDF/ODT) are often screenshots only; indicators mirror common LMS reports.
-- Payload shape: POST /stats/query JSON (camelCase) — see models::stats_builder::StatsBuilderBody.

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Registrations — by audience type',
    'Patron count by audience type label (public_types).',
    $$
    {
      "entity": "users",
      "joins": ["public_types"],
      "select": [
        {"field": "public_types.label", "alias": "audienceType"}
      ],
      "filters": [],
      "aggregations": [
        {"fn": "count", "field": "users.id", "alias": "registeredCount"}
      ],
      "groupBy": [
        {"field": "public_types.label", "alias": "audienceType"}
      ],
      "having": [],
      "orderBy": [{"field": "registeredCount", "dir": "desc"}],
      "limit": 100,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Registrations — by month',
    'New patron records per month (created_at).',
    $$
    {
      "entity": "users",
      "joins": [],
      "select": [],
      "filters": [],
      "aggregations": [
        {"fn": "count", "field": "users.id", "alias": "newRegistrations"}
      ],
      "groupBy": [],
      "having": [],
      "timeBucket": {"field": "users.created_at", "granularity": "month", "alias": "month"},
      "orderBy": [{"field": "month", "dir": "asc"}],
      "limit": 120,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Loans — by month',
    'Loan volume per month (loan date).',
    $$
    {
      "entity": "loans",
      "joins": [],
      "select": [],
      "filters": [],
      "aggregations": [
        {"fn": "count", "field": "loans.id", "alias": "loanCount"}
      ],
      "groupBy": [],
      "having": [],
      "timeBucket": {"field": "loans.date", "granularity": "month", "alias": "month"},
      "orderBy": [{"field": "month", "dir": "asc"}],
      "limit": 120,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Loans — by audience and media type',
    'Loan counts cross-tabulated by audience (public_types) and biblio media type.',
    $$
    {
      "entity": "loans",
      "joins": ["users.public_types", "items.biblios"],
      "select": [
        {"field": "public_types.label", "alias": "audienceType"},
        {"field": "biblios.media_type", "alias": "media"}
      ],
      "filters": [],
      "aggregations": [
        {"fn": "count", "field": "loans.id", "alias": "loanCount"}
      ],
      "groupBy": [
        {"field": "public_types.label", "alias": "audienceType"},
        {"field": "biblios.media_type", "alias": "media"}
      ],
      "having": [],
      "orderBy": [{"field": "loanCount", "dir": "desc"}],
      "limit": 500,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Loans — unique borrowers per month',
    'Distinct borrowers per month (count distinct user_id).',
    $$
    {
      "entity": "loans",
      "joins": [],
      "select": [],
      "filters": [],
      "aggregations": [
        {"fn": "countDistinct", "field": "loans.user_id", "alias": "uniqueBorrowers"}
      ],
      "groupBy": [],
      "having": [],
      "timeBucket": {"field": "loans.date", "granularity": "month", "alias": "month"},
      "orderBy": [{"field": "month", "dir": "asc"}],
      "limit": 120,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);
