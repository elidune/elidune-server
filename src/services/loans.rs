//! Loan management service

use chrono::{DateTime, Utc};

use std::sync::Arc;

use crate::{
    api::loans::{LoanSettings as LoanSettingsApi, UpdateLoanSettingsRequest},
    error::{AppError, AppResult},
    models::{
        biblio::MediaType,
        loan::{CreateLoan, LoanDetails},
        user::UserStatus,
    },
    repository::LoansServiceRepository,
};

#[derive(Clone)]
pub struct LoansService {
    repository: Arc<dyn LoansServiceRepository>,
}

impl LoansService {
    pub fn new(repository: Arc<dyn LoansServiceRepository>) -> Self {
        Self { repository }
    }

    /// Get active loans for a user (paginated). `page` and `per_page` must be valid (≥1, capped by caller).
    pub async fn get_user_loans(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<LoanDetails>, i64)> {
        self.repository.users_get_by_id(user_id).await?;
        self.repository.loans_get_for_user(user_id, page, per_page).await
    }

    /// Get archived (returned) loans for a user (paginated).
    pub async fn get_user_archived_loans(
        &self,
        user_id: i64,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<LoanDetails>, i64)> {
        self.repository.users_get_by_id(user_id).await?;
        self.repository.loans_archives_get_for_user(user_id, page, per_page).await
    }

    /// Create a new loan (borrow an item).
    ///
    /// Enforces user-level rules before delegating to the repository:
    /// - blocked users cannot borrow unless `force` is set
    /// - expired subscriptions are rejected unless `force` is set
    ///
    /// The repository enforces the hold queue on the copy: only the patron whose turn it is
    /// (`ready`, else first `pending`) may borrow unless `force=true` (staff clears active holds on that copy).
    pub async fn create_loan(&self, loan: CreateLoan) -> AppResult<(i64, DateTime<Utc>)> {
        let user = self.repository.users_get_by_id(loan.user_id).await?;

        let status = user.status.unwrap_or(UserStatus::Active);
        if status == UserStatus::Deleted {
            return Err(AppError::BusinessRule(
                "Cannot create a loan for a deleted user account".to_string(),
            ));
        }

        if !user.can_borrow() && !loan.force {
            return Err(AppError::BusinessRule(
                "User account is not active or cannot borrow — use force=true to override".to_string()
            ));
        }

        if let Some(expiry_at) = user.expiry_at {
            if expiry_at < Utc::now() && !loan.force {
                return Err(AppError::BusinessRule(format!(
                    "User subscription expired on {} — use force=true to override",
                    expiry_at.format("%Y-%m-%d")
                )));
            }
        }

        self.repository.loans_create(&loan).await
    }

    /// Return a borrowed item
    pub async fn return_loan(&self, loan_id: i64) -> AppResult<LoanDetails> {
        let outcome = self.repository.loans_return(loan_id).await?;
        Ok(outcome.details)
    }

    /// Return a borrowed item by item identification (barcode or call number)
    pub async fn return_loan_by_item(&self, item_identification: &str) -> AppResult<LoanDetails> {
        let loan = self.repository.loans_get_by_item_identification(item_identification).await?;
        let outcome = self.repository.loans_return(loan.id).await?;
        Ok(outcome.details)
    }

    /// Renew a loan
    pub async fn renew_loan(&self, loan_id: i64) -> AppResult<(DateTime<Utc>, i16)> {
        let loan = self.repository.loans_get_by_id(loan_id).await?;
        let user = self.repository.users_get_by_id(loan.user_id).await?;

        if !user.can_borrow() {
            return Err(AppError::BusinessRule(
                "User account is not active or cannot borrow — use force=true to override".to_string()
            ));
        }
        self.repository.loans_renew(loan_id).await
    }

    /// Renew a loan by item identification (barcode or call number)
    pub async fn renew_loan_by_item(&self, item_identification: &str) -> AppResult<(i64, DateTime<Utc>, i16)> {
        let loan = self.repository.loans_get_by_item_identification(item_identification).await?;
        let loan_id = loan.id;
        let (new_expiry_date, renew_count) = self.repository.loans_renew(loan_id).await?;
        Ok((loan_id, new_expiry_date, renew_count))
    }

    /// Count active loans
    pub async fn count_active(&self) -> AppResult<i64> {
        self.repository.loans_count_active().await
    }

    /// Count overdue loans
    pub async fn count_overdue(&self) -> AppResult<i64> {
        self.repository.loans_count_overdue().await
    }

    /// Count active loans for a specific physical item
    pub async fn count_active_for_item(&self, item_id: i64) -> AppResult<i64> {
        self.repository.loans_count_active_for_item(item_id).await
    }

    /// Count active loans across all physical items of a biblio (used by OPAC availability)
    pub async fn count_active_for_biblio(&self, biblio_id: i64) -> AppResult<i64> {
        self.repository.loans_count_active_for_biblio(biblio_id).await
    }

    /// Global loan rules per media type (`loans_settings` table).
    pub async fn get_global_loan_settings(&self) -> AppResult<Vec<LoanSettingsApi>> {
        let rows = self.repository.loans_get_settings().await?;
        Ok(rows
            .into_iter()
            .map(|row| LoanSettingsApi {
                media_type: row.media_type.unwrap_or(MediaType::PrintedText),
                max_loans: row.nb_max.unwrap_or(5),
                max_renewals: row.nb_renews.unwrap_or(2),
                duration_days: row.duration.unwrap_or(21),
            })
            .collect())
    }

    /// Update global loan rules per media type.
    pub async fn update_global_loan_settings(
        &self,
        request: UpdateLoanSettingsRequest,
    ) -> AppResult<Vec<LoanSettingsApi>> {
        if let Some(loan_settings) = request.loan_settings {
            for setting in loan_settings {
                self.repository
                    .loans_settings_upsert_row(
                        setting.media_type.as_db_str(),
                        setting.max_loans,
                        setting.max_renewals,
                        setting.duration_days,
                    )
                    .await?;
            }
        }
        self.get_global_loan_settings().await
    }
}

// =============================================================================
// Unit tests — use manual test doubles to avoid mockall lifetime issues
// with async_trait + &str parameters.
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::AppError,
        models::{
            loan::CreateLoan,
            user::{AccountTypeSlug, User, UserStatus},
        },
        repository::{LoansRepository, UsersRepository},
    };
    // ----- Minimal test double implementing both required traits -----

    struct FakeRepo {
        /// Pre-loaded user to return for `users_get_by_id`
        user: Option<User>,
        /// Return value for `loans_create`
        loan_id: i64,
    }

    fn make_user(id: i64, status: Option<UserStatus>, expiry_at: Option<chrono::DateTime<Utc>>) -> User {
        User {
            id,
            // NULL status in DB is treated as active; tests pass None for the default happy path.
            status: status.or(Some(UserStatus::Active)),
            expiry_at,
            account_type: AccountTypeSlug::Reader,
            group_id: None,
            barcode: None,
            login: None,
            password: None,
            firstname: None,
            lastname: None,
            email: None,
            addr_street: None,
            addr_zip_code: None,
            addr_city: None,
            phone: None,
            birthdate: None,
            created_at: None,
            update_at: None,
            fee: None,
            archived_at: None,
            language: None,
            sex: None,
            staff_type: None,
            hours_per_week: None,
            staff_start_date: None,
            staff_end_date: None,
            public_type: None,
            notes: None,
            two_factor_enabled: None,
            two_factor_method: None,
            totp_secret: None,
            recovery_codes: None,
            recovery_codes_used: None,
            receive_reminders: true,
            must_change_password: false,
        }
    }

    #[async_trait::async_trait]
    impl LoansRepository for FakeRepo {
        async fn loans_get_by_id(&self, _: i64) -> AppResult<crate::models::loan::Loan> { unimplemented!() }
        async fn loans_get_by_item_identification(&self, _: &str) -> AppResult<crate::models::loan::Loan> { unimplemented!() }
        async fn loans_get_for_user(
            &self,
            _: i64,
            _: i64,
            _: i64,
        ) -> AppResult<(Vec<LoanDetails>, i64)> {
            Ok((vec![], 0))
        }
        async fn loans_archives_get_for_user(
            &self,
            _: i64,
            _: i64,
            _: i64,
        ) -> AppResult<(Vec<LoanDetails>, i64)> {
            Ok((vec![], 0))
        }
        async fn loans_create(&self, _: &CreateLoan) -> AppResult<(i64, chrono::DateTime<Utc>)> {
            Ok((self.loan_id, Utc::now()))
        }
        async fn loans_return(&self, _: i64) -> AppResult<crate::models::loan::LoanReturnOutcome> {
            unimplemented!()
        }
        async fn loans_renew(&self, _: i64) -> AppResult<(chrono::DateTime<Utc>, i16)> { unimplemented!() }
        async fn loans_get_settings(&self) -> AppResult<Vec<crate::models::loan::LoanSettings>> { Ok(vec![]) }
        async fn loans_count_active(&self) -> AppResult<i64> { Ok(0) }
        async fn loans_count_overdue(&self) -> AppResult<i64> { Ok(0) }
        async fn loans_get_active_ids_for_item(&self, _: i64) -> AppResult<Vec<i64>> { Ok(vec![]) }
        async fn loans_count_active_for_item(&self, _: i64) -> AppResult<i64> { Ok(0) }
        async fn loans_get_active_ids_for_biblio(&self, _: i64) -> AppResult<Vec<i64>> { Ok(vec![]) }
        async fn loans_get_active_ids_for_user(&self, _: i64) -> AppResult<Vec<i64>> { Ok(vec![]) }
        async fn loans_count_active_for_biblio(&self, _: i64) -> AppResult<i64> { Ok(0) }
        async fn loans_count_active_for_user(&self, _: i64) -> AppResult<i64> { Ok(0) }
        async fn loans_get_overdue_for_reminders(&self, _: u32) -> AppResult<Vec<crate::repository::loans::OverdueLoanRow>> { Ok(vec![]) }
        async fn loans_get_overdue(&self, _: i64, _: i64) -> AppResult<(Vec<crate::repository::loans::OverdueLoanRow>, i64)> { Ok((vec![], 0)) }
        async fn loans_update_reminder_sent(&self, _: &[i64]) -> AppResult<()> { Ok(()) }
        async fn loans_settings_upsert_row(
            &self,
            _: &str,
            _: i16,
            _: i16,
            _: i16,
        ) -> AppResult<()> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl UsersRepository for FakeRepo {
        async fn users_count(&self) -> AppResult<i64> { Ok(0) }
        async fn users_set_must_change_password(&self, _: i64, _: bool) -> AppResult<()> { Ok(()) }
        async fn users_get_by_id(&self, _: i64) -> AppResult<User> {
            self.user.clone().ok_or_else(|| AppError::NotFound("user not found".into()))
        }
        async fn users_get_by_login(&self, _: &str) -> AppResult<Option<User>> { Ok(None) }
        async fn users_get_by_email(&self, _: &str) -> AppResult<Option<User>> { Ok(None) }
        async fn users_update_password(&self, _: i64, _: &str) -> AppResult<()> { Ok(()) }
        async fn users_email_exists(&self, _: &str, _: Option<i64>) -> AppResult<bool> { Ok(false) }
        async fn users_login_exists(&self, _: &str, _: Option<i64>) -> AppResult<bool> { Ok(false) }
        async fn users_get_rights(&self, _: &AccountTypeSlug) -> AppResult<crate::models::user::UserRights> { unimplemented!() }
        async fn users_search(&self, _: &crate::models::user::UserQuery) -> AppResult<(Vec<crate::models::user::UserShort>, i64)> { Ok((vec![], 0)) }
        async fn users_create(&self, _: &crate::models::user::UserPayload, _: Option<String>) -> AppResult<User> { unimplemented!() }
        async fn users_update(&self, _: i64, _: &crate::models::user::UserPayload, _: Option<String>) -> AppResult<User> { unimplemented!() }
        async fn users_delete(&self, _: i64, _: bool) -> AppResult<()> { Ok(()) }
        async fn users_block(&self, _: i64) -> AppResult<User> { unimplemented!() }
        async fn users_unblock(&self, _: i64) -> AppResult<User> { unimplemented!() }
        async fn users_update_profile(&self, _: i64, _: &crate::models::user::UpdateProfile, _: Option<String>) -> AppResult<User> { unimplemented!() }
        async fn users_update_account_type(&self, _: i64, _: &AccountTypeSlug) -> AppResult<User> { unimplemented!() }
        async fn users_update_2fa_settings(&self, _: i64, _: bool, _: Option<&str>, _: Option<&str>, _: Option<&str>) -> AppResult<()> { Ok(()) }
        async fn users_mark_recovery_code_used(&self, _: i64, _: &str) -> AppResult<()> { Ok(()) }
        async fn users_get_emails_by_public_type(&self, _: Option<i64>) -> AppResult<Vec<crate::repository::users::UserEmailTarget>> { Ok(vec![]) }
    }

    // LoansServiceRepository has a blanket impl for T: LoansRepository + UsersRepository + Send + Sync,
    // so FakeRepo already implements it — no explicit impl needed.

    fn make_service(user: Option<User>, loan_id: i64) -> LoansService {
        LoansService::new(Arc::new(FakeRepo { user, loan_id }))
    }

    fn make_loan(user_id: i64, force: bool) -> CreateLoan {
        CreateLoan {
            user_id,
            item_id: Some(42),
            item_identification: None,
            force,
        }
    }

    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_loan_active_user_succeeds() {
        let user = make_user(1, None, None);
        let svc = make_service(Some(user), 100);
        assert!(svc.create_loan(make_loan(1, false)).await.is_ok());
    }

    #[tokio::test]
    async fn test_create_loan_blocked_user_rejected() {
        let user = make_user(2, Some(UserStatus::Blocked), None);
        let svc = make_service(Some(user), 0);
        assert!(matches!(
            svc.create_loan(make_loan(2, false)).await,
            Err(AppError::BusinessRule(_))
        ));
    }

    #[tokio::test]
    async fn test_create_loan_blocked_user_with_force_succeeds() {
        let user = make_user(3, Some(UserStatus::Blocked), None);
        let svc = make_service(Some(user), 101);
        assert!(svc.create_loan(make_loan(3, true)).await.is_ok());
    }

    #[tokio::test]
    async fn test_create_loan_deleted_user_always_rejected() {
        let user = make_user(4, Some(UserStatus::Deleted), None);
        let svc = make_service(Some(user), 0);
        // force=true should NOT override a deleted account
        assert!(matches!(
            svc.create_loan(make_loan(4, true)).await,
            Err(AppError::BusinessRule(_))
        ));
    }

    #[tokio::test]
    async fn test_create_loan_expired_subscription_rejected() {
        let expired = Utc::now() - chrono::Duration::days(1);
        let user = make_user(5, None, Some(expired));
        let svc = make_service(Some(user), 0);
        assert!(matches!(
            svc.create_loan(make_loan(5, false)).await,
            Err(AppError::BusinessRule(_))
        ));
    }

    #[tokio::test]
    async fn test_create_loan_expired_subscription_with_force_succeeds() {
        let expired = Utc::now() - chrono::Duration::days(1);
        let user = make_user(6, None, Some(expired));
        let svc = make_service(Some(user), 102);
        assert!(svc.create_loan(make_loan(6, true)).await.is_ok());
    }

    #[tokio::test]
    async fn test_create_loan_user_not_found() {
        let svc = make_service(None, 0); // no user pre-loaded
        assert!(matches!(
            svc.create_loan(make_loan(99, false)).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_valid_subscription_not_expired() {
        let future_date = Utc::now() + chrono::Duration::days(30);
        let user = make_user(7, None, Some(future_date)); // subscription valid
        let svc = make_service(Some(user), 103);
        assert!(svc.create_loan(make_loan(7, false)).await.is_ok());
    }
}
