//! Loans repository — loan rules and settings persistence.

use sqlx::Row;

use super::super::Repository;
use crate::{
    error::AppResult,
    models::loan::{LoanSettings, LoanSettingsRenewAt},
};

impl Repository {
    /// Resolve loan settings: (duration_days, nb_max_media, nb_max_total_all_media, nb_renews, renew_at_policy).
    ///
    /// `nb_max_total_all_media` comes from `nb_max` on the default rows (audience / global), not per-media rows.
    /// `nb_max_media` applies only to the current `media_type`; it does not use default rows' `nb_max`.
    pub(crate) async fn resolve_loan_settings(
        &self,
        user_public_type: Option<i64>,
        media_type: Option<&str>,
    ) -> AppResult<(i16, i16, i16, i16, LoanSettingsRenewAt)> {
        let default_duration = 21i16;
        let default_nb_max_media = 5i16;
        let default_nb_max_total = 5i16;
        let default_nb_renews = 2i16;

        let pick_renew = |row: Option<&sqlx::postgres::PgRow>| -> Option<LoanSettingsRenewAt> {
            row.and_then(|r| r.get::<Option<String>, _>("renew_at"))
                .map(|s| LoanSettingsRenewAt::from(s.as_str()))
        };

        let ptls_spec = if let (Some(pt_id), Some(mt)) = (user_public_type, media_type) {
            sqlx::query(
                "SELECT duration, nb_max, nb_renews, renew_at FROM public_type_loan_settings WHERE public_type_id = $1 AND media_type = $2",
            )
            .bind(pt_id)
            .bind(mt)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let ptls_default = if let Some(pt_id) = user_public_type {
            sqlx::query(
                "SELECT duration, nb_max, nb_renews, renew_at FROM public_type_loan_settings WHERE public_type_id = $1 AND media_type IS NULL",
            )
            .bind(pt_id)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let ls_spec = if let Some(mt) = media_type {
            sqlx::query(
                "SELECT duration, nb_max, nb_renews, renew_at FROM loans_settings WHERE media_type = $1",
            )
            .bind(mt)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let ls_default = sqlx::query(
            "SELECT duration, nb_max, nb_renews, renew_at FROM loans_settings WHERE media_type IS NULL",
        )
        .fetch_optional(&self.pool)
        .await?;

        let duration = ptls_spec
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("duration"))
            .or_else(|| ptls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("duration")))
            .or_else(|| ls_spec.as_ref().and_then(|r| r.get::<Option<i16>, _>("duration")))
            .or_else(|| ls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("duration")))
            .unwrap_or(default_duration);

        let nb_max_media = ptls_spec
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("nb_max"))
            .or_else(|| ls_spec.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_max")))
            .unwrap_or(default_nb_max_media);

        let nb_max_total = ptls_default
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("nb_max"))
            .or_else(|| ls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_max")))
            .unwrap_or(default_nb_max_total);

        let nb_renews = ptls_spec
            .as_ref()
            .and_then(|r| r.get::<Option<i16>, _>("nb_renews"))
            .or_else(|| ptls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_renews")))
            .or_else(|| ls_spec.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_renews")))
            .or_else(|| ls_default.as_ref().and_then(|r| r.get::<Option<i16>, _>("nb_renews")))
            .unwrap_or(default_nb_renews);

        let renew_at_policy = pick_renew(ptls_spec.as_ref())
            .or_else(|| pick_renew(ptls_default.as_ref()))
            .or_else(|| pick_renew(ls_spec.as_ref()))
            .or_else(|| pick_renew(ls_default.as_ref()))
            .unwrap_or(LoanSettingsRenewAt::Now);

        Ok((
            duration,
            nb_max_media,
            nb_max_total,
            nb_renews,
            renew_at_policy,
        ))
    }

    /// Get loan settings
    pub async fn loans_get_settings(&self) -> AppResult<Vec<LoanSettings>> {
        sqlx::query_as::<_, LoanSettings>(
            r#"SELECT * FROM loans_settings ORDER BY (media_type IS NOT NULL), media_type"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Delete all rows in `loans_settings`.
    pub async fn loans_settings_delete_rows(&self) -> AppResult<()> {
        sqlx::query("DELETE FROM loans_settings")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Upsert one row in `loans_settings`. `media_type == None` is the global default row (`media_type` IS NULL).
    pub async fn loans_settings_upsert_row(
        &self,
        media_type: Option<String>,
        nb_max: i16,
        nb_renews: i16,
        duration: i16,
        renew_at: LoanSettingsRenewAt,
    ) -> AppResult<()> {
        let rows_affected = if let Some(ref mt) = media_type {
            sqlx::query(
                r#"
                UPDATE loans_settings
                SET nb_max = $2, nb_renews = $3, duration = $4, renew_at = $5
                WHERE media_type = $1
                "#,
            )
            .bind(mt.as_str())
            .bind(nb_max)
            .bind(nb_renews)
            .bind(duration)
            .bind(renew_at)
            .execute(&self.pool)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                r#"
                UPDATE loans_settings
                SET nb_max = $1, nb_renews = $2, duration = $3, renew_at = $4
                WHERE media_type IS NULL
                "#,
            )
            .bind(nb_max)
            .bind(nb_renews)
            .bind(duration)
            .bind(renew_at)
            .execute(&self.pool)
            .await?
            .rows_affected()
        };

        if rows_affected == 0 {
            if let Some(mt) = media_type {
                sqlx::query(
                    r#"
                    INSERT INTO loans_settings (media_type, nb_max, nb_renews, duration, renew_at)
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                )
                .bind(mt)
                .bind(nb_max)
                .bind(nb_renews)
                .bind(duration)
                .bind(renew_at)
                .execute(&self.pool)
                .await?;
            } else {
                sqlx::query(
                    r#"
                    INSERT INTO loans_settings (media_type, nb_max, nb_renews, duration, renew_at)
                    VALUES (NULL, $1, $2, $3, $4)
                    "#,
                )
                .bind(nb_max)
                .bind(nb_renews)
                .bind(duration)
                .bind(renew_at)
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }
}
