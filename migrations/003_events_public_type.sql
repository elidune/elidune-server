-- Events: target_public (legacy smallint codes) -> public_type (VARCHAR(50), FK to public_types.name)

DO $check$
BEGIN
    IF EXISTS (
        SELECT 1 FROM events
        WHERE target_public IS NOT NULL
          AND target_public NOT IN (97, 106)
    ) THEN
        RAISE EXCEPTION
            'events.target_public contains unmigrated values (only 97→adult, 106→child, NULL are supported)';
    END IF;
END
$check$;

ALTER TABLE events RENAME COLUMN target_public TO public_type;

ALTER TABLE events
    ALTER COLUMN public_type TYPE VARCHAR(50)
    USING (
        CASE
            WHEN public_type IS NULL THEN NULL::VARCHAR(50)
            WHEN public_type = 97 THEN 'adult'
            WHEN public_type = 106 THEN 'child'
        END
    );

ALTER TABLE events
    ADD CONSTRAINT events_public_type_name_fkey
    FOREIGN KEY (public_type)
    REFERENCES public_types(name)
    ON DELETE SET NULL;

COMMENT ON COLUMN events.public_type IS 'Target audience: public_types.name, or NULL for all audiences';
