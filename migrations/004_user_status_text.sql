-- Store user account status as camelCase strings (active, blocked, deleted) instead of SMALLINT.

ALTER TABLE users
    ALTER COLUMN status DROP DEFAULT;

ALTER TABLE users
    ALTER COLUMN status TYPE VARCHAR(32)
    USING (
        CASE COALESCE(status::integer, 0)
            WHEN 0 THEN 'active'
            WHEN 1 THEN 'blocked'
            WHEN 2 THEN 'deleted'
            ELSE 'active'
        END
    );

ALTER TABLE users
    ALTER COLUMN status SET DEFAULT 'active';
