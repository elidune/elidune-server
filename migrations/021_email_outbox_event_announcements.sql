CREATE TABLE IF NOT EXISTS email_outbox_event_announcements (
    outbox_id BIGINT NOT NULL REFERENCES email_outbox (id) ON DELETE CASCADE,
    event_id BIGINT NOT NULL REFERENCES events (id) ON DELETE CASCADE,
    PRIMARY KEY (outbox_id)
);
CREATE INDEX email_outbox_event_announcements_event_idx ON email_outbox_event_announcements (event_id);
