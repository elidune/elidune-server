-- Reader assistant conversation storage (sessions, messages, recommendations, feedback).

CREATE TABLE IF NOT EXISTS ai_sessions (
    id BIGINT PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS ai_sessions_user_updated_idx
    ON ai_sessions (user_id, updated_at DESC)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS ai_messages (
    id BIGINT PRIMARY KEY,
    session_id BIGINT NOT NULL REFERENCES ai_sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content_redacted TEXT NOT NULL,
    provider TEXT,
    model TEXT,
    token_usage INTEGER,
    latency_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS ai_messages_session_created_idx
    ON ai_messages (session_id, created_at ASC);

CREATE TABLE IF NOT EXISTS ai_recommendations (
    id BIGINT PRIMARY KEY,
    message_id BIGINT NOT NULL REFERENCES ai_messages(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('in_catalog', 'external')),
    biblio_id BIGINT REFERENCES biblios(id) ON DELETE SET NULL,
    external_ref JSONB,
    score DOUBLE PRECISION NOT NULL,
    rationale TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS ai_recommendations_message_idx
    ON ai_recommendations (message_id, created_at ASC);

CREATE TABLE IF NOT EXISTS ai_feedback (
    id BIGINT PRIMARY KEY,
    recommendation_id BIGINT NOT NULL REFERENCES ai_recommendations(id) ON DELETE CASCADE,
    user_feedback SMALLINT NOT NULL CHECK (user_feedback IN (-1, 0, 1)),
    comment TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS ai_feedback_recommendation_idx
    ON ai_feedback (recommendation_id, created_at DESC);
