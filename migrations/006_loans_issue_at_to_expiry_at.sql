ALTER TABLE loans RENAME COLUMN issue_at TO expiry_at;
ALTER TABLE loans_archives RENAME COLUMN issue_at TO expiry_at;
