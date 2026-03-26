-- Saved flexible statistics queries (JSON body for POST /stats/query)

CREATE TABLE IF NOT EXISTS saved_queries (
    id          BIGSERIAL   PRIMARY KEY,
    name        VARCHAR(200) NOT NULL,
    description TEXT,
    query_json  JSONB       NOT NULL,
    user_id     BIGINT      NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    is_shared   BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_saved_queries_user_id ON saved_queries(user_id);
CREATE INDEX IF NOT EXISTS idx_saved_queries_shared ON saved_queries(is_shared) WHERE is_shared = TRUE;
