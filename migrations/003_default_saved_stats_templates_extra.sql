-- Additional shared templates (extends 002). Safe after 002; distinct template names.
-- Covers city, account type, archived loans, average renewals.

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Registrations — by city',
    'Patron distribution by city (addr_city); empty values appear as a blank row.',
    $$
    {
      "entity": "users",
      "joins": [],
      "select": [
        {"field": "users.addr_city", "alias": "city"}
      ],
      "filters": [],
      "aggregations": [
        {"fn": "count", "field": "users.id", "alias": "registeredCount"}
      ],
      "groupBy": [
        {"field": "users.addr_city", "alias": "city"}
      ],
      "having": [],
      "orderBy": [{"field": "registeredCount", "dir": "desc"}],
      "limit": 200,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Registrations — by account type',
    'Patron count by account type (guest, reader, librarian, …).',
    $$
    {
      "entity": "users",
      "joins": ["account_types"],
      "select": [
        {"field": "account_types.name", "alias": "accountType"}
      ],
      "filters": [],
      "aggregations": [
        {"fn": "count", "field": "users.id", "alias": "registeredCount"}
      ],
      "groupBy": [
        {"field": "account_types.name", "alias": "accountType"}
      ],
      "having": [],
      "orderBy": [{"field": "registeredCount", "dir": "desc"}],
      "limit": 50,
      "offset": 0
    }
    $$::jsonb,
    u.id,
    true
FROM (SELECT id FROM users ORDER BY id LIMIT 1) AS u
WHERE EXISTS (SELECT 1 FROM users LIMIT 1);

INSERT INTO saved_queries (name, description, query_json, user_id, is_shared)
SELECT
    'Loans (archived) — by month',
    'Historical loan volume (loans_archives) per month.',
    $$
    {
      "entity": "loans_archives",
      "joins": [],
      "select": [],
      "filters": [],
      "aggregations": [
        {"fn": "count", "field": "loans_archives.id", "alias": "archivedLoanCount"}
      ],
      "groupBy": [],
      "having": [],
      "timeBucket": {"field": "loans_archives.date", "granularity": "month", "alias": "month"},
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
    'Loans — average renewals per month',
    'Average number of renewals (nb_renews) per loan month.',
    $$
    {
      "entity": "loans",
      "joins": [],
      "select": [],
      "filters": [],
      "aggregations": [
        {"fn": "avg", "field": "loans.nb_renews", "alias": "avgRenewals"}
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
