-- Remap legacy language codes/IDs to camelCase strings (serialized Language)
--
-- NOTE: Previous migrations may have set `lang`/`lang_orig` to VARCHAR(3) (ISO-like
-- codes). CamelCase serialized values like 'unknown'/'portuguese' require a wider
-- type, so widen columns before updating.
ALTER TABLE items
  ALTER COLUMN lang TYPE VARCHAR(32),
  ALTER COLUMN lang_orig TYPE VARCHAR(32);

UPDATE items
SET lang = CASE lang
    WHEN '0' THEN 'unknown'
    WHEN '1' THEN 'french'
    WHEN '2' THEN 'english'
    WHEN '3' THEN 'german'
    WHEN '4' THEN 'japanese'
    WHEN '5' THEN 'spanish'
    WHEN '6' THEN 'portuguese'
    WHEN 'fre' THEN 'french'
    WHEN 'fra' THEN 'french'
    WHEN 'eng' THEN 'english'
    WHEN 'ger' THEN 'german'
    WHEN 'deu' THEN 'german'
    WHEN 'jpn' THEN 'japanese'
    WHEN 'spa' THEN 'spanish'
    WHEN 'por' THEN 'portuguese'
    ELSE lang
END
WHERE lang IS NOT NULL
  AND lang IN ('0','1','2','3','4','5','6','fre','fra','eng','ger','deu','jpn','spa','por');

UPDATE items
SET lang_orig = CASE lang_orig
    WHEN '0' THEN 'unknown'
    WHEN '1' THEN 'french'
    WHEN '2' THEN 'english'
    WHEN '3' THEN 'german'
    WHEN '4' THEN 'japanese'
    WHEN '5' THEN 'spanish'
    WHEN '6' THEN 'portuguese'
    WHEN 'fre' THEN 'french'
    WHEN 'fra' THEN 'french'
    WHEN 'eng' THEN 'english'
    WHEN 'ger' THEN 'german'
    WHEN 'deu' THEN 'german'
    WHEN 'jpn' THEN 'japanese'
    WHEN 'spa' THEN 'spanish'
    WHEN 'por' THEN 'portuguese'
    ELSE lang_orig
END
WHERE lang_orig IS NOT NULL
  AND lang_orig IN ('0','1','2','3','4','5','6','fre','fra','eng','ger','deu','jpn','spa','por');

