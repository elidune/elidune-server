//! Loan management service

use chrono::{DateTime, Utc};

use std::sync::Arc;

use crate::{
    error::{AppError, AppResult},
    marc::{MarcRecord, marc_record_for_loan_export},
    models::{
        dto::loans::{LoanSettingsDto, UpdateLoanSettingsRequest},
        Loan,
        loan::{
            CreateLoan, HoldReadyEmailOutcome, LOANS_MARC_EXPORT_MAX, LoanCreateOutcome,
            LoanDetails, LoanMarcExportEncoding, LoanMarcExportFormat, LoanReturnOutcome,
            LoanSettingsRenewAt,
        },
        user::UserStatus,
    },
    repository::LoansServiceRepository,
    services::{
        audit::{self, AuditLogMeta, AuditService},
        email::EmailService,
        event_bus::EventBus,
    },
};
use z3950_rs::marc_rs::{BinaryWriter, Encoding as MarcEncoding, MarcFormat, XmlWriter};

#[derive(Clone)]
pub struct LoansService {
    repository: Arc<dyn LoansServiceRepository>,
    audit: AuditService,
    email: EmailService,
    events: EventBus,
}

impl LoansService {
    pub fn new(
        repository: Arc<dyn LoansServiceRepository>,
        audit: AuditService,
        email: EmailService,
        events: EventBus,
    ) -> Self {
        Self {
            repository,
            audit,
            email,
            events,
        }
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
    pub async fn create_loan(
        &self,
        loan: CreateLoan,
        audit_actor: Option<i64>,
        client_ip: Option<String>,
    ) -> AppResult<LoanCreateOutcome> {
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

        let outcome = self.repository.loans_create(&loan).await?;
        if let Some(item_id) = loan.item_id {
            self.events
                .loan_created(outcome.loan_id, loan.user_id, item_id);
        }
        if let Some(hold_id) = outcome.fulfilled_hold_id {
            self.audit.log(
                audit::event::HOLD_FULFILLED,
                audit_actor,
                Some("hold"),
                Some(hold_id),
                client_ip.clone(),
                Some(serde_json::json!({
                    "loan_id": outcome.loan_id,
                    "user_id": loan.user_id,
                    "item_id": loan.item_id,
                    "trigger": "checkout",
                })),
                AuditLogMeta::success(),
            );
        }
        Ok(outcome)
    }

    /// Return a borrowed item
    pub async fn return_loan(
        &self,
        loan_id: i64,
        audit_actor: Option<i64>,
        client_ip: Option<String>,
    ) -> AppResult<LoanDetails> {
        let mut outcome = self.repository.loans_return(loan_id).await?;
        outcome.hold_ready_email = self.send_hold_ready_email(&outcome).await;
        self.publish_return_events(&outcome);
        self.audit_return_side_effects(audit_actor, client_ip, &outcome);
        Ok(outcome.details)
    }

    /// Return a borrowed item by item identification (barcode or call number)
    pub async fn return_loan_by_item(
        &self,
        item_identification: &str,
        audit_actor: Option<i64>,
        client_ip: Option<String>,
    ) -> AppResult<LoanDetails> {
        let loan = self.repository.loans_get_by_item_identification(item_identification).await?;
        let mut outcome = self.repository.loans_return(loan.id).await?;
        outcome.hold_ready_email = self.send_hold_ready_email(&outcome).await;
        self.publish_return_events(&outcome);
        self.audit_return_side_effects(audit_actor, client_ip, &outcome);
        Ok(outcome.details)
    }

    async fn send_hold_ready_email(
        &self,
        outcome: &LoanReturnOutcome,
    ) -> Option<HoldReadyEmailOutcome> {
        let hold = outcome.readied_hold.as_ref()?;
        let contact = self
            .repository
            .users_hold_ready_contact(hold.user_id)
            .await
            .ok()
            .flatten();
        let to = contact
            .as_ref()
            .and_then(|c| c.email.as_deref().map(str::trim))
            .filter(|s| !s.is_empty())?;
        match crate::hold_email::send_hold_ready(
            &self.email,
            contact.clone(),
            hold,
            &outcome.details,
        )
        .await
        {
            Ok(()) => Some(HoldReadyEmailOutcome {
                email: Some(to.to_string()),
                send_error: None,
            }),
            Err(e) => {
                tracing::warn!(
                    target: "loans",
                    error = %e,
                    hold_id = hold.id,
                    "Failed to queue hold ready email"
                );
                Some(HoldReadyEmailOutcome {
                    email: Some(to.to_string()),
                    send_error: Some(e.to_string()),
                })
            }
        }
    }

    fn publish_return_events(&self, outcome: &LoanReturnOutcome) {
        let user_id = outcome
            .details
            .user
            .as_ref()
            .map(|u| u.id)
            .unwrap_or(0);
        self.events
            .loan_returned(outcome.details.id, user_id, outcome.details.item_id);
        if let Some(ref hold) = outcome.readied_hold {
            self.events.hold_ready(hold.id, hold.user_id, hold.item_id);
        }
    }

    fn audit_return_side_effects(
        &self,
        audit_actor: Option<i64>,
        client_ip: Option<String>,
        outcome: &LoanReturnOutcome,
    ) {
        let Some(ref hold) = outcome.readied_hold else {
            return;
        };

        self.audit.log(
            audit::event::HOLD_READY,
            audit_actor,
            Some("hold"),
            Some(hold.id),
            client_ip.clone(),
            Some(serde_json::json!({
                "user_id": hold.user_id,
                "item_id": hold.item_id,
                "expires_at": hold.expires_at,
                "trigger": "loan_return",
            })),
            AuditLogMeta::success(),
        );

        let Some(ref email_outcome) = outcome.hold_ready_email else {
            return;
        };

        let biblio_id = outcome.details.biblio.id;
        let title = outcome
            .details
            .biblio
            .title
            .as_deref()
            .unwrap_or("(unknown title)");

        if let Some(ref err) = email_outcome.send_error {
            self.audit.log(
                audit::event::EMAIL_HOLD_READY_SENT,
                audit_actor,
                Some("hold"),
                Some(hold.id),
                client_ip.clone(),
                Some(serde_json::json!({
                    "user_id": hold.user_id,
                    "email": email_outcome.email,
                    "biblio_id": biblio_id,
                    "title": title,
                    "trigger": "loan_return",
                })),
                AuditLogMeta::failure_background("email_delivery_failed", err.clone()),
            );
        } else if email_outcome.email.is_some() {
            self.audit.log(
                audit::event::EMAIL_HOLD_READY_SENT,
                audit_actor,
                Some("user"),
                Some(hold.user_id),
                client_ip,
                Some(serde_json::json!({
                    "hold_id": hold.id,
                    "email": email_outcome.email,
                    "biblio_id": biblio_id,
                    "title": title,
                    "trigger": "loan_return",
                })),
                AuditLogMeta::success(),
            );
        }
    }

    /// Get a loan by id
    pub async fn get_loan(&self, loan_id: i64) -> AppResult<Loan> {
        self.repository.loans_get_by_id(loan_id).await
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
    pub async fn get_global_loan_settings(&self) -> AppResult<Vec<LoanSettingsDto>> {
        let rows = self.repository.loans_get_settings().await?;
        Ok(rows
            .into_iter()
            .map(|row| LoanSettingsDto {
                media_type: row.media_type,
                max_loans: row.nb_max.unwrap_or(5),
                max_renewals: row.nb_renews.unwrap_or(2),
                duration_days: row.duration.unwrap_or(21),
                renew_at: row.renew_at.unwrap_or(LoanSettingsRenewAt::Now),
            })
            .collect())
    }

    /// Update global loan rules per media type.
    pub async fn update_global_loan_settings(
        &self,
        request: UpdateLoanSettingsRequest,
    ) -> AppResult<Vec<LoanSettingsDto>> {

        // remove all existing loan settings
        self.repository.loans_settings_delete_rows().await?;

        if let Some(loan_settings) = request.loan_settings {
            for setting in loan_settings {
                let media_key = setting
                    .media_type
                    .as_ref()
                    .map(|m| m.as_db_str().to_string());
                self.repository
                    .loans_settings_upsert_row(
                        media_key,
                        setting.max_loans,
                        setting.max_renewals,
                        setting.duration_days,
                        setting.renew_at,
                    )
                    .await?;
            }
        }
        self.get_global_loan_settings().await
    }

    /// Build a downloadable MARC export for all active or archived loans of a user (no pagination).
    /// Caller must enforce `require_self_or_staff`; this method only checks the user exists.
    pub async fn export_user_loans_marc_file(
        &self,
        user_id: i64,
        archived: bool,
        format: LoanMarcExportFormat,
        encoding: LoanMarcExportEncoding,
    ) -> AppResult<(Vec<u8>, &'static str, &'static str)> {
        self.repository.users_get_by_id(user_id).await?;
        let rows = self
            .repository
            .loans_get_for_marc_export(user_id, archived)
            .await?;
        if rows.len() > LOANS_MARC_EXPORT_MAX {
            return Err(AppError::Validation(format!(
                "Too many loans to export ({} > max {})",
                rows.len(),
                LOANS_MARC_EXPORT_MAX
            )));
        }
        let mut records: Vec<MarcRecord> = Vec::with_capacity(rows.len());
        for row in rows {
            records.push(marc_record_for_loan_export(
                &row.biblio,
                row.start_date,
                row.expiry_at,
                row.returned_at,
            ));
        }
        let bytes = serialize_marc_export_records(&records, format, encoding)?;
        let (ct, name) = export_marc_content_type_filename(format);
        Ok((bytes, ct, name))
    }
}

fn export_marc_content_type_filename(
    format: LoanMarcExportFormat,
) -> (&'static str, &'static str) {
    match format {
        LoanMarcExportFormat::Json => ("application/json", "loans-export.json"),
        LoanMarcExportFormat::Marc21 => ("application/marc", "loans-export.mrc"),
        LoanMarcExportFormat::Unimarc => ("application/marc", "loans-export.mrc"),
        LoanMarcExportFormat::Marcxml => ("application/xml", "loans-export.xml"),
    }
}

fn serialize_marc_export_records(
    records: &[MarcRecord],
    format: LoanMarcExportFormat,
    encoding: LoanMarcExportEncoding,
) -> AppResult<Vec<u8>> {
    let marc_enc = match encoding {
        LoanMarcExportEncoding::Utf8 => MarcEncoding::Utf8,
        LoanMarcExportEncoding::Marc8 => MarcEncoding::Marc8,
    };
    match format {
        LoanMarcExportFormat::Json => serde_json::to_vec(records).map_err(|e| {
            AppError::Internal(format!("MARC JSON export serialization: {}", e))
        }),
        LoanMarcExportFormat::Marc21 => {
            let mut buf = Vec::new();
            let fmt = MarcFormat::Marc21(marc_enc);
            {
                let mut w = BinaryWriter::new(&mut buf);
                for r in records {
                    let mut rec = r.clone();
                    w.write_record(&fmt, &mut rec).map_err(|e| {
                        AppError::Internal(format!("MARC21 binary write: {}", e))
                    })?;
                }
                w.flush()
                    .map_err(|e| AppError::Internal(format!("MARC21 binary flush: {}", e)))?;
            }
            Ok(buf)
        }
        LoanMarcExportFormat::Unimarc => {
            let mut buf = Vec::new();
            let fmt = MarcFormat::Unimarc(marc_enc);
            {
                let mut w = BinaryWriter::new(&mut buf);
                for r in records {
                    let mut rec = r.clone();
                    w.write_record(&fmt, &mut rec).map_err(|e| {
                        AppError::Internal(format!("UNIMARC binary write: {}", e))
                    })?;
                }
                w.flush()
                    .map_err(|e| AppError::Internal(format!("UNIMARC binary flush: {}", e)))?;
            }
            Ok(buf)
        }
        LoanMarcExportFormat::Marcxml => {
            let mut buf = Vec::new();
            // MARC-XML output is always UTF-8; semantic record is serialized via the chosen format.
            let fmt = MarcFormat::Marc21(MarcEncoding::Utf8);
            {
                let mut w = XmlWriter::new(&mut buf);
                w.start_collection()
                    .map_err(|e| AppError::Internal(format!("MARC-XML collection start: {}", e)))?;
                for r in records {
                    w.write_record(&fmt, r).map_err(|e| {
                        AppError::Internal(format!("MARC-XML record: {}", e))
                    })?;
                }
                w.end_collection()
                    .map_err(|e| AppError::Internal(format!("MARC-XML collection end: {}", e)))?;
                w.flush()
                    .map_err(|e| AppError::Internal(format!("MARC-XML flush: {}", e)))?;
            }
            Ok(buf)
        }
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
            token_version: 0,
        }
    }

    #[async_trait::async_trait]
    impl LoansRepository for FakeRepo {
        async fn loans_settings_delete_rows(&self) -> AppResult<()> { Ok(()) }
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
        async fn loans_get_for_marc_export(
            &self,
            _: i64,
            _: bool,
        ) -> AppResult<Vec<crate::models::loan::LoanMarcExportRow>> {
            Ok(vec![])
        }
        async fn loans_create(&self, _: &CreateLoan) -> AppResult<LoanCreateOutcome> {
            Ok(LoanCreateOutcome {
                loan_id: self.loan_id,
                expiry_at: Utc::now(),
                fulfilled_hold_id: None,
            })
        }
        async fn loans_return(&self, _: i64) -> AppResult<LoanReturnOutcome> {
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
            _: Option<String>,
            _: i16,
            _: i16,
            _: i16,
            _: LoanSettingsRenewAt,
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
        async fn users_get_token_version(&self, _: i64) -> AppResult<i64> { Ok(0) }
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
        async fn users_hold_ready_contact(&self, _: i64) -> AppResult<Option<crate::repository::users::HoldReadyUserContact>> { Ok(None) }
    }

    // LoansServiceRepository has a blanket impl for T: LoansRepository + UsersRepository + Send + Sync,
    // so FakeRepo already implements it — no explicit impl needed.

    fn make_service(user: Option<User>, loan_id: i64) -> LoansService {
        let pool = sqlx::Pool::connect_lazy("postgres://localhost/unused").unwrap();
        let audit = AuditService::new(crate::repository::Repository::new(pool.clone(), None));
        let dynamic_config = crate::DynamicConfig::new(crate::AppConfig::for_test());
        let email = crate::EmailService::new(dynamic_config, pool);
        let (tx, _) = tokio::sync::broadcast::channel(1);
        let events = crate::services::event_bus::EventBus::new(tx);
        LoansService::new(Arc::new(FakeRepo { user, loan_id }), audit, email, events)
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
        assert!(svc.create_loan(make_loan(1, false), None, None).await.is_ok());
    }

    #[tokio::test]
    async fn test_create_loan_blocked_user_rejected() {
        let user = make_user(2, Some(UserStatus::Blocked), None);
        let svc = make_service(Some(user), 0);
        assert!(matches!(
            svc.create_loan(make_loan(2, false), None, None).await,
            Err(AppError::BusinessRule(_))
        ));
    }

    #[tokio::test]
    async fn test_create_loan_blocked_user_with_force_succeeds() {
        let user = make_user(3, Some(UserStatus::Blocked), None);
        let svc = make_service(Some(user), 101);
        assert!(svc.create_loan(make_loan(3, true), None, None).await.is_ok());
    }

    #[tokio::test]
    async fn test_create_loan_deleted_user_always_rejected() {
        let user = make_user(4, Some(UserStatus::Deleted), None);
        let svc = make_service(Some(user), 0);
        // force=true should NOT override a deleted account
        assert!(matches!(
            svc.create_loan(make_loan(4, true), None, None).await,
            Err(AppError::BusinessRule(_))
        ));
    }

    #[tokio::test]
    async fn test_create_loan_expired_subscription_rejected() {
        let expired = Utc::now() - chrono::Duration::days(1);
        let user = make_user(5, None, Some(expired));
        let svc = make_service(Some(user), 0);
        assert!(matches!(
            svc.create_loan(make_loan(5, false), None, None).await,
            Err(AppError::BusinessRule(_))
        ));
    }

    #[tokio::test]
    async fn test_create_loan_expired_subscription_with_force_succeeds() {
        let expired = Utc::now() - chrono::Duration::days(1);
        let user = make_user(6, None, Some(expired));
        let svc = make_service(Some(user), 102);
        assert!(svc.create_loan(make_loan(6, true), None, None).await.is_ok());
    }

    #[tokio::test]
    async fn test_create_loan_user_not_found() {
        let svc = make_service(None, 0); // no user pre-loaded
        assert!(matches!(
            svc.create_loan(make_loan(99, false), None, None).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_valid_subscription_not_expired() {
        let future_date = Utc::now() + chrono::Duration::days(30);
        let user = make_user(7, None, Some(future_date)); // subscription valid
        let svc = make_service(Some(user), 103);
        assert!(svc.create_loan(make_loan(7, false), None, None).await.is_ok());
    }
}
