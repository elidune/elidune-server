-- Optional binary attachment for cultural events (e.g. flyer PDF).

ALTER TABLE events
    ADD COLUMN IF NOT EXISTS attachment_data BYTEA,
    ADD COLUMN IF NOT EXISTS attachment_filename VARCHAR(512),
    ADD COLUMN IF NOT EXISTS attachment_mime_type VARCHAR(255);

COMMENT ON COLUMN events.attachment_data IS 'Optional attachment payload stored in-database';
COMMENT ON COLUMN events.attachment_filename IS 'Original file name for Content-Disposition';
COMMENT ON COLUMN events.attachment_mime_type IS 'MIME type for download Content-Type';
