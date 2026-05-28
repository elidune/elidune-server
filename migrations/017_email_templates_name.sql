-- Human-readable display label for each email template row (localized per language).

ALTER TABLE email_templates
    ADD COLUMN IF NOT EXISTS name VARCHAR(128);

UPDATE email_templates
SET name = CASE template_id
    WHEN '2fa_code' THEN CASE language
        WHEN 'french' THEN 'Code d''authentification à deux facteurs'
        ELSE 'Two-factor authentication code'
    END
    WHEN 'recovery_code' THEN CASE language
        WHEN 'french' THEN 'Code de récupération de compte'
        ELSE 'Account recovery code'
    END
    WHEN 'password_reset' THEN CASE language
        WHEN 'french' THEN 'Réinitialisation du mot de passe'
        ELSE 'Password reset'
    END
    WHEN 'hold_ready' THEN CASE language
        WHEN 'french' THEN 'Réservation prête au retrait'
        ELSE 'Hold ready for pickup'
    END
    WHEN 'inventory_loan_closed' THEN CASE language
        WHEN 'french' THEN 'Inventaire — prêt clôturé'
        ELSE 'Inventory — loan closed'
    END
    WHEN 'overdue_reminder' THEN CASE language
        WHEN 'french' THEN 'Rappel de prêt en retard'
        ELSE 'Overdue loan reminder'
    END
    WHEN 'event_announcement' THEN CASE language
        WHEN 'french' THEN 'Annonce d''événement'
        ELSE 'Event announcement'
    END
    ELSE template_id
END
WHERE name IS NULL;

ALTER TABLE email_templates
    ALTER COLUMN name SET NOT NULL;

COMMENT ON COLUMN email_templates.name IS 'Human-readable label shown in the Settings UI (localized per language row).';
