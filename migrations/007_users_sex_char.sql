-- Store patron sex as single-letter codes (nullable): 'm', 'f', or NULL (unknown / prefer not to say).
-- Replaces legacy numeric codes (70=female, 77=male, 85=unknown).

ALTER TABLE users ALTER COLUMN sex DROP DEFAULT;

ALTER TABLE users ADD COLUMN sex_new VARCHAR(1);

UPDATE users SET sex_new = CASE
    WHEN sex = 70 THEN 'f'
    WHEN sex = 77 THEN 'm'
    ELSE NULL
END;

ALTER TABLE users DROP COLUMN sex;

ALTER TABLE users RENAME COLUMN sex_new TO sex;

ALTER TABLE users ADD CONSTRAINT users_sex_check CHECK (sex IS NULL OR sex IN ('m', 'f'));
