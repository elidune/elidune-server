-- Migration: users.birthdate VARCHAR -> DATE
-- Normalize legacy values: ISO YYYY-MM-DD, YYYYMMDD, YYMMDD (yy<=30 -> 20xx, else 19xx).
-- Values that cannot be parsed become NULL.

CREATE OR REPLACE FUNCTION elidune_migrate_birthdate(s TEXT) RETURNS DATE
LANGUAGE plpgsql
AS $$
DECLARE
  t TEXT;
  y INT;
  m INT;
  d INT;
  yy INT;
BEGIN
  IF s IS NULL THEN
    RETURN NULL;
  END IF;
  t := trim(both from s);
  IF t = '' THEN
    RETURN NULL;
  END IF;

  -- ISO YYYY-MM-DD
  IF t ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$' THEN
    BEGIN
      RETURN t::DATE;
    EXCEPTION WHEN OTHERS THEN
      RETURN NULL;
    END;
  END IF;

  -- YYYYMMDD (8 digits)
  IF length(t) = 8 AND t ~ '^[0-9]+$' THEN
    BEGIN
      y := substring(t, 1, 4)::INT;
      m := substring(t, 5, 2)::INT;
      d := substring(t, 7, 2)::INT;
      RETURN make_date(y, m, d);
    EXCEPTION WHEN OTHERS THEN
      RETURN NULL;
    END;
  END IF;

  -- YYMMDD (6 digits; yy 00-30 -> 2000-2030, 31-99 -> 1931-1999)
  IF length(t) = 6 AND t ~ '^[0-9]+$' THEN
    BEGIN
      yy := substring(t, 1, 2)::INT;
      m := substring(t, 3, 2)::INT;
      d := substring(t, 5, 2)::INT;
      IF yy <= 30 THEN
        y := yy + 2000;
      ELSE
        y := yy + 1900;
      END IF;
      RETURN make_date(y, m, d);
    EXCEPTION WHEN OTHERS THEN
      RETURN NULL;
    END;
  END IF;

  RETURN NULL;
END;
$$;

ALTER TABLE users ADD COLUMN birthdate_new DATE;

UPDATE users SET birthdate_new = elidune_migrate_birthdate(birthdate::text);

ALTER TABLE users DROP COLUMN birthdate;
ALTER TABLE users RENAME COLUMN birthdate_new TO birthdate;

DROP FUNCTION elidune_migrate_birthdate(TEXT);
