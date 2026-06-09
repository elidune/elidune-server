//! Loans domain methods on [`Repository`].

mod mutations;
mod queries;
mod reminders;
mod settings;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::Repository;
use crate::{
    error::AppResult,
    models::loan::{
        CreateLoan, Loan, LoanDetails, LoanCreateOutcome, LoanMarcExportRow, LoanReturnOutcome,
        LoanSettings, LoanSettingsRenewAt,
    },
};

pub use reminders::OverdueLoanRow;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LoansRepository: Send + Sync {
    async fn loans_get_by_id(&self, id: i64) -> AppResult<Loan>;
    async fn loans_get_borrower_for_loan(
        &self,
        loan_id: i64,
    ) -> AppResult<crate::models::user::UserShort>;
    async fn loans_get_by_item_identification(&self, item_identification: &str) -> AppResult<Loan>;
    async fn loans_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<LoanDetails>, i64)>;
    async fn loans_archives_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<LoanDetails>, i64)>;
    /// All loans for MARC export (no pagination). Active or archived only.
    async fn loans_get_for_marc_export(
        &self,
        user_id: i64,
        archived: bool,
    ) -> AppResult<Vec<LoanMarcExportRow>>;
    async fn loans_create(&self, loan: &CreateLoan) -> AppResult<LoanCreateOutcome>;
    async fn loans_return(&self, loan_id: i64) -> AppResult<LoanReturnOutcome>;
    async fn loans_renew(&self, loan_id: i64) -> AppResult<(DateTime<Utc>, i16)>;
    async fn loans_get_settings(&self) -> AppResult<Vec<LoanSettings>>;
    async fn loans_count_active(&self) -> AppResult<i64>;
    async fn loans_count_overdue(&self) -> AppResult<i64>;
    async fn loans_count_active_for_item(&self, item_id: i64) -> AppResult<i64>;
    async fn loans_get_active_ids_for_item(&self, item_id: i64) -> AppResult<Vec<i64>>;
    async fn loans_get_active_ids_for_biblio(&self, biblio_id: i64) -> AppResult<Vec<i64>>;
    async fn loans_get_active_ids_for_user(&self, user_id: i64) -> AppResult<Vec<i64>>;
    async fn loans_count_active_for_biblio(&self, biblio_id: i64) -> AppResult<i64>;
    async fn loans_count_active_for_user(&self, user_id: i64) -> AppResult<i64>;
    async fn loans_get_overdue_for_reminders(
        &self,
        frequency_days: u32,
    ) -> AppResult<Vec<OverdueLoanRow>>;
    async fn loans_get_overdue(
        &self,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<OverdueLoanRow>, i64)>;
    async fn loans_update_reminder_sent(&self, loan_ids: &[i64]) -> AppResult<()>;
    /// Upsert global loan rules (`loans_settings`). `media_type == None` updates the default row (`media_type` IS NULL).
    async fn loans_settings_upsert_row(
        &self,
        media_type: Option<String>,
        nb_max: i16,
        nb_renews: i16,
        duration: i16,
        renew_at: LoanSettingsRenewAt,
    ) -> AppResult<()>;
    async fn loans_settings_delete_rows(&self) -> AppResult<()>;
}

/// Combined repository trait used by [`crate::services::loans::LoansService`].
///
/// Implemented by the concrete [`Repository`] via blanket impl below.
pub trait LoansServiceRepository:
    LoansRepository + crate::repository::UsersRepository + Send + Sync
{
}

impl<T: LoansRepository + crate::repository::UsersRepository + Send + Sync> LoansServiceRepository
    for T
{
}

#[async_trait::async_trait]
impl LoansRepository for Repository {
    async fn loans_get_by_id(&self, id: i64) -> crate::error::AppResult<Loan> {
        Repository::loans_get_by_id(self, id).await
    }
    async fn loans_get_borrower_for_loan(
        &self,
        loan_id: i64,
    ) -> crate::error::AppResult<crate::models::user::UserShort> {
        Repository::loans_get_borrower_for_loan(self, loan_id).await
    }
    async fn loans_get_by_item_identification(
        &self,
        identification: &str,
    ) -> crate::error::AppResult<Loan> {
        Repository::loans_get_by_item_identification(self, identification).await
    }
    async fn loans_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> crate::error::AppResult<(Vec<LoanDetails>, i64)> {
        Repository::loans_get_for_user(self, user_id, page, per_page).await
    }
    async fn loans_archives_get_for_user(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> crate::error::AppResult<(Vec<LoanDetails>, i64)> {
        Repository::loans_archives_get_for_user(self, user_id, page, per_page).await
    }
    async fn loans_get_for_marc_export(
        &self,
        user_id: i64,
        archived: bool,
    ) -> crate::error::AppResult<Vec<LoanMarcExportRow>> {
        Repository::loans_get_for_marc_export(self, user_id, archived).await
    }
    async fn loans_create(
        &self,
        loan: &CreateLoan,
    ) -> crate::error::AppResult<LoanCreateOutcome> {
        Repository::loans_create(self, loan).await
    }
    async fn loans_return(&self, loan_id: i64) -> crate::error::AppResult<LoanReturnOutcome> {
        Repository::loans_return(self, loan_id).await
    }
    async fn loans_renew(
        &self,
        loan_id: i64,
    ) -> crate::error::AppResult<(chrono::DateTime<chrono::Utc>, i16)> {
        Repository::loans_renew(self, loan_id).await
    }
    async fn loans_get_settings(
        &self,
    ) -> crate::error::AppResult<Vec<crate::models::loan::LoanSettings>> {
        Repository::loans_get_settings(self).await
    }
    async fn loans_count_active(&self) -> crate::error::AppResult<i64> {
        Repository::loans_count_active(self).await
    }
    async fn loans_count_overdue(&self) -> crate::error::AppResult<i64> {
        Repository::loans_count_overdue(self).await
    }
    async fn loans_count_active_for_item(&self, item_id: i64) -> crate::error::AppResult<i64> {
        Repository::loans_count_active_for_item(self, item_id).await
    }
    async fn loans_get_active_ids_for_item(
        &self,
        item_id: i64,
    ) -> crate::error::AppResult<Vec<i64>> {
        Repository::loans_get_active_ids_for_item(self, item_id).await
    }
    async fn loans_get_active_ids_for_biblio(
        &self,
        biblio_id: i64,
    ) -> crate::error::AppResult<Vec<i64>> {
        Repository::loans_get_active_ids_for_biblio(self, biblio_id).await
    }
    async fn loans_get_active_ids_for_user(
        &self,
        user_id: i64,
    ) -> crate::error::AppResult<Vec<i64>> {
        Repository::loans_get_active_ids_for_user(self, user_id).await
    }
    async fn loans_count_active_for_biblio(
        &self,
        biblio_id: i64,
    ) -> crate::error::AppResult<i64> {
        Repository::loans_count_active_for_biblio(self, biblio_id).await
    }
    async fn loans_count_active_for_user(&self, user_id: i64) -> crate::error::AppResult<i64> {
        Repository::loans_count_active_for_user(self, user_id).await
    }
    async fn loans_get_overdue_for_reminders(
        &self,
        frequency_days: u32,
    ) -> crate::error::AppResult<Vec<OverdueLoanRow>> {
        Repository::loans_get_overdue_for_reminders(self, frequency_days).await
    }
    async fn loans_get_overdue(
        &self,
        page: i64,
        per_page: i64,
    ) -> crate::error::AppResult<(Vec<OverdueLoanRow>, i64)> {
        Repository::loans_get_overdue(self, page, per_page).await
    }
    async fn loans_update_reminder_sent(&self, loan_ids: &[i64]) -> crate::error::AppResult<()> {
        Repository::loans_update_reminder_sent(self, loan_ids).await
    }
    async fn loans_settings_upsert_row(
        &self,
        media_type: Option<String>,
        nb_max: i16,
        nb_renews: i16,
        duration: i16,
        renew_at: LoanSettingsRenewAt,
    ) -> crate::error::AppResult<()> {
        Repository::loans_settings_upsert_row(
            self, media_type, nb_max, nb_renews, duration, renew_at,
        )
        .await
    }
    async fn loans_settings_delete_rows(&self) -> crate::error::AppResult<()> {
        Repository::loans_settings_delete_rows(self).await
    }
}

/// Scalar subquery (column alias `author`): first author on biblio `b` as JSON for [`BiblioShort`].
pub(crate) const LOAN_DETAILS_FIRST_AUTHOR_SQL: &str = r#"(SELECT jsonb_build_object(
                'id', a.id::text, 'lastname', a.lastname, 'firstname', a.firstname,
                'bio', a.bio, 'notes', a.notes, 'function', ba.function
            ) FROM biblio_authors ba JOIN authors a ON a.id = ba.author_id
            WHERE ba.biblio_id = b.id ORDER BY ba.position LIMIT 1) as author"#;
