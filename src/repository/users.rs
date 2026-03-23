//! Users domain methods on Repository

use async_trait::async_trait;
use chrono::Utc;
use sqlx::Row;

use super::Repository;
use crate::{
    error::{AppError, AppResult},
    models::user::{AccountTypeSlug, Rights, UpdateProfile, User, UserPayload, UserQuery, UserRights, UserShort, UserStatus},
};


/// Minimal user info used for bulk email targeting
#[derive(Debug, sqlx::FromRow)]
pub struct UserEmailTarget {
    pub id: i64,
    pub email: Option<String>,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub language: Option<String>,
}

// Note: not `mockall::automock` — several methods use `Option<&str>` which mockall cannot derive for.
#[async_trait]
pub trait UsersRepository: Send + Sync {
    async fn users_get_by_id(&self, id: i64) -> AppResult<User>;
    async fn users_get_by_login(&self, login: &str) -> AppResult<Option<User>>;
    async fn users_get_by_email(&self, email: &str) -> AppResult<Option<User>>;
    async fn users_update_password(&self, id: i64, password_hash: &str) -> AppResult<()>;
    async fn users_email_exists(&self, email: &str, exclude_id: Option<i64>) -> AppResult<bool>;
    async fn users_login_exists(&self, login: &str, exclude_id: Option<i64>) -> AppResult<bool>;
    async fn users_get_rights(&self, account_type: &AccountTypeSlug) -> AppResult<UserRights>;
    async fn users_search(&self, query: &UserQuery) -> AppResult<(Vec<UserShort>, i64)>;
    async fn users_create(
        &self,
        user: &UserPayload,
        password: Option<String>,
    ) -> AppResult<User>;
    async fn users_update(
        &self,
        id: i64,
        user: &UserPayload,
        password: Option<String>,
    ) -> AppResult<User>;
    async fn users_delete(&self, id: i64, force: bool) -> AppResult<()>;
    async fn users_block(&self, id: i64) -> AppResult<User>;
    async fn users_unblock(&self, id: i64) -> AppResult<User>;
    async fn users_update_profile(
        &self,
        id: i64,
        profile: &UpdateProfile,
        password: Option<String>,
    ) -> AppResult<User>;
    async fn users_update_account_type(
        &self,
        id: i64,
        account_type: &AccountTypeSlug,
    ) -> AppResult<User>;
    async fn users_update_2fa_settings(
        &self,
        id: i64,
        enabled: bool,
        method: Option<&str>,
        totp_secret: Option<&str>,
        recovery_codes: Option<&str>,
    ) -> AppResult<()>;
    async fn users_mark_recovery_code_used(&self, id: i64, used_codes: &str) -> AppResult<()>;
    async fn users_get_emails_by_public_type(
        &self,
        public_type: Option<i64>,
    ) -> AppResult<Vec<UserEmailTarget>>;
}

// ---------------------------------------------------------------------------
// Trait implementation — forwards to inherent methods above.
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl UsersRepository for Repository {
    async fn users_get_by_id(&self, id: i64) -> crate::error::AppResult<User> {
        Repository::users_get_by_id(self, id).await
    }
    async fn users_get_by_login(&self, login: &str) -> crate::error::AppResult<Option<User>> {
        Repository::users_get_by_login(self, login).await
    }
    async fn users_get_by_email(&self, email: &str) -> crate::error::AppResult<Option<User>> {
        Repository::users_get_by_email(self, email).await
    }
    async fn users_update_password(&self, id: i64, password_hash: &str) -> crate::error::AppResult<()> {
        Repository::users_update_password(self, id, password_hash).await
    }
    async fn users_email_exists(&self, email: &str, exclude_id: Option<i64>) -> crate::error::AppResult<bool> {
        Repository::users_email_exists(self, email, exclude_id).await
    }
    async fn users_login_exists(&self, login: &str, exclude_id: Option<i64>) -> crate::error::AppResult<bool> {
        Repository::users_login_exists(self, login, exclude_id).await
    }
    async fn users_get_rights(&self, account_type: &crate::models::user::AccountTypeSlug) -> crate::error::AppResult<crate::models::user::UserRights> {
        Repository::users_get_rights(self, account_type).await
    }
    async fn users_search(&self, query: &crate::models::user::UserQuery) -> crate::error::AppResult<(Vec<crate::models::user::UserShort>, i64)> {
        Repository::users_search(self, query).await
    }
    async fn users_create(&self, user: &crate::models::user::UserPayload, password: Option<String>) -> crate::error::AppResult<User> {
        Repository::users_create(self, user, password).await
    }
    async fn users_update(&self, id: i64, user: &crate::models::user::UserPayload, password: Option<String>) -> crate::error::AppResult<User> {
        Repository::users_update(self, id, user, password).await
    }
    async fn users_delete(&self, id: i64, force: bool) -> crate::error::AppResult<()> {
        Repository::users_delete(self, id, force).await
    }
    async fn users_block(&self, id: i64) -> crate::error::AppResult<User> {
        Repository::users_block(self, id).await
    }
    async fn users_unblock(&self, id: i64) -> crate::error::AppResult<User> {
        Repository::users_unblock(self, id).await
    }
    async fn users_update_profile(&self, id: i64, profile: &crate::models::user::UpdateProfile, password: Option<String>) -> crate::error::AppResult<User> {
        Repository::users_update_profile(self, id, profile, password).await
    }
    async fn users_update_account_type(&self, id: i64, account_type: &crate::models::user::AccountTypeSlug) -> crate::error::AppResult<User> {
        Repository::users_update_account_type(self, id, account_type).await
    }
    async fn users_update_2fa_settings(
        &self,
        id: i64,
        enabled: bool,
        method: Option<&str>,
        totp_secret: Option<&str>,
        recovery_codes: Option<&str>,
    ) -> crate::error::AppResult<()> {
        Repository::users_update_2fa_settings(self, id, enabled, method, totp_secret, recovery_codes).await
    }
    async fn users_mark_recovery_code_used(&self, id: i64, used_codes: &str) -> crate::error::AppResult<()> {
        Repository::users_mark_recovery_code_used(self, id, used_codes).await
    }
    async fn users_get_emails_by_public_type(&self, public_type: Option<i64>) -> crate::error::AppResult<Vec<UserEmailTarget>> {
        Repository::users_get_emails_by_public_type(self, public_type).await
    }
}



impl Repository {
    /// Get user by ID
    #[tracing::instrument(skip(self), err)]
    pub async fn users_get_by_id(&self, id: i64) -> AppResult<User> {
        use crate::models::user::UserRow;
        let user_row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT * FROM users WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("User with id {} not found", id)))?;

        Ok(user_row.into())
    }

    /// Get user by login (primary authentication method)
    #[tracing::instrument(skip(self), err)]
    pub async fn users_get_by_login(&self, login: &str) -> AppResult<Option<User>> {
        use crate::models::user::UserRow;
        let user_row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT * FROM users WHERE LOWER(login) = LOWER($1) AND (status IS NULL OR status != 2)
            "#,
        )
        .bind(login)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user_row.map(|r| r.into()))
    }

    /// Get user by email (primary authentication method)
    #[tracing::instrument(skip(self), err)]
    pub async fn users_get_by_email(&self, email: &str) -> AppResult<Option<User>> {
        use crate::models::user::UserRow;
        let user_row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT * FROM users WHERE LOWER(email) = LOWER($1) AND (status IS NULL OR status != 2)
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user_row.map(|r| r.into()))
    }

    /// Update user password directly (used for password reset flow)
    #[tracing::instrument(skip(self), err)]
    pub async fn users_update_password(&self, id: i64, password_hash: &str) -> AppResult<()> {
        let result = sqlx::query(
            "UPDATE users SET password = $1, update_at = NOW() WHERE id = $2"
        )
        .bind(password_hash)
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("User with id {} not found", id)));
        }

        Ok(())
    }

    /// Check if email already exists
    #[tracing::instrument(skip(self), err)]
    pub async fn users_email_exists(&self, email: &str, exclude_id: Option<i64>) -> AppResult<bool> {
        let exists: bool = if let Some(id) = exclude_id {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE LOWER(email) = LOWER($1) AND id != $2)")
                .bind(email)
                .bind(id)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE LOWER(email) = LOWER($1))")
                .bind(email)
                .fetch_one(&self.pool)
                .await?
        };
        Ok(exists)
    }

    /// Check if login already exists
    #[tracing::instrument(skip(self), err)]
    pub async fn users_login_exists(&self, login: &str, exclude_id: Option<i64>) -> AppResult<bool> {
        let exists: bool = if let Some(id) = exclude_id {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE LOWER(login) = LOWER($1) AND id != $2)")
                .bind(login)
                .bind(id)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE LOWER(login) = LOWER($1))")
                .bind(login)
                .fetch_one(&self.pool)
                .await?
        };
        Ok(exists)
    }

    /// Get user rights from account type
    #[tracing::instrument(skip(self), err)]
    pub async fn users_get_rights(&self, account_type: &AccountTypeSlug) -> AppResult<UserRights> {
        let row = sqlx::query(
            r#"
            SELECT items_rights, users_rights, loans_rights, 
                   borrows_rights, settings_rights, items_archive_rights
            FROM account_types 
            WHERE code = $1
            "#,
        )
        .bind(account_type.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Account type '{}' not found", account_type.as_str())))?;

        Ok(UserRights {
            items_rights: Rights::from(row.get::<Option<String>, _>("items_rights")),
            users_rights: Rights::from(row.get::<Option<String>, _>("users_rights")),
            loans_rights: Rights::from(row.get::<Option<String>, _>("loans_rights")),
            borrows_rights: Rights::from(row.get::<Option<String>, _>("borrows_rights")),
            settings_rights: Rights::from(row.get::<Option<String>, _>("settings_rights")),
            items_archive_rights: Rights::from(row.get::<Option<String>, _>("items_archive_rights")),
        })
    }

    /// Search users with pagination
    #[tracing::instrument(skip(self), err)]
    pub async fn users_search(&self, query: &UserQuery) -> AppResult<(Vec<UserShort>, i64)> {
        let page = query.page.unwrap_or(1);
        let per_page = query.per_page.unwrap_or(20);
        let offset = (page - 1) * per_page;

        let mut conditions = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(ref name) = query.name {
            params.push(format!("%{}%", name.to_lowercase()));
            conditions.push(format!(
                "(LOWER(firstname) LIKE ${} OR LOWER(lastname) LIKE ${})",
                params.len(),
                params.len()
            ));
        }

        if let Some(ref barcode) = query.barcode {
            params.push(barcode.clone());
            conditions.push(format!("barcode = ${}", params.len()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Count total
        let count_query = format!(
            r#"
            SELECT COUNT(*) as count FROM users {}
            "#,
            where_clause
        );

        let mut count_builder = sqlx::query_scalar::<_, i64>(&count_query);
        for param in &params {
            count_builder = count_builder.bind(param);
        }
        let total = count_builder.fetch_one(&self.pool).await?;

        // Fetch users (exclude deleted users by default)
        let status_filter = if conditions.is_empty() {
            "WHERE (u.status IS NULL OR u.status != 2)".to_string()
        } else {
            " AND (u.status IS NULL OR u.status != 2)".to_string()
        };
        
        use crate::models::user::UserShortRow;
        let select_query = format!(
            r#"
            SELECT u.id, u.firstname, u.lastname, u.account_type, u.public_type,
                   (SELECT COUNT(*) FROM loans l WHERE l.user_id = u.id AND l.returned_at IS NULL) as nb_loans,
                   (SELECT COUNT(*) FROM loans l WHERE l.user_id = u.id AND l.returned_at IS NULL AND l.issue_at < NOW()) as nb_late_loans
            FROM users u
            {}{}
            ORDER BY u.lastname, u.firstname
            LIMIT {} OFFSET {}
            "#,
            where_clause, status_filter, per_page, offset
        );

        let mut select_builder = sqlx::query_as::<_, UserShortRow>(&select_query);
        for param in &params {
            select_builder = select_builder.bind(param);
        }
        let user_rows = select_builder.fetch_all(&self.pool).await?;
        let users: Vec<UserShort> = user_rows.into_iter().map(|r| r.into()).collect();

        Ok((users, total))
    }

    /// Create a new user
    #[tracing::instrument(skip(self), err)]
    pub async fn users_create(&self, user: &UserPayload, password: Option<String>) -> AppResult<User> {
        let now = Utc::now();

        let account_type = user.account_type.as_ref().map(|at| at.as_str()).unwrap_or("guest");
        let fee = user.fee.as_ref().map(|f| f.as_str());
        
        // Parse staff dates
        let staff_start_date = user.staff_start_date.as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let staff_end_date = user.staff_end_date.as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let hours_pw = user.hours_per_week.map(|v| v as f32);

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users (
                login, password, firstname, lastname, email,
                addr_street, addr_zip_code, addr_city, phone,
                birthdate, account_type,
                fee, public_type, notes, group_id, barcode,
                sex, staff_type, hours_per_week, staff_start_date, staff_end_date,
                status, created_at, update_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24
            ) RETURNING id
            "#,
        )
        .bind(
            user.login
                .as_deref()
                .ok_or_else(|| AppError::Validation("Login is required".to_string()))?,
        )
        .bind(&password)
        .bind(&user.firstname)
        .bind(&user.lastname)
        .bind(&user.email)
        .bind(&user.addr_street)
        .bind(&user.addr_zip_code)
        .bind(&user.addr_city)
        .bind(&user.phone)
        .bind(&user.birthdate)
        .bind(account_type)
        .bind(fee)
        .bind(&user.public_type)
        .bind(&user.notes)
        .bind(&user.group_id)
        .bind(&user.barcode)
        .bind(&user.sex)
        .bind(&user.staff_type)
        .bind(hours_pw)
        .bind(staff_start_date)
        .bind(staff_end_date)
        .bind(UserStatus::Active as i16)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        self.users_get_by_id(id).await
    }

    /// Update an existing user
    #[tracing::instrument(skip(self), err)]
    pub async fn users_update(&self, id: i64, user: &UserPayload, password: Option<String>) -> AppResult<User> {
        let now = Utc::now();

        // Build dynamic update query
        let mut sets = vec!["update_at = $1".to_string()];
        let mut param_idx = 2;

        macro_rules! add_field {
            ($field:expr, $name:expr) => {
                if $field.is_some() {
                    sets.push(format!("{} = ${}", $name, param_idx));
                    param_idx += 1;
                }
            };
        }
        
        macro_rules! add_field_enum {
            ($field:expr, $name:expr) => {
                if $field.is_some() {
                    sets.push(format!("{} = ${}", $name, param_idx));
                    param_idx += 1;
                }
            };
        }

        add_field!(user.login, "login");
        add_field!(user.firstname, "firstname");
        add_field!(user.lastname, "lastname");
        add_field!(user.email, "email");
        add_field!(user.addr_street, "addr_street");
        add_field!(user.addr_zip_code, "addr_zip_code");
        add_field!(user.addr_city, "addr_city");
        add_field!(user.phone, "phone");
        add_field!(user.birthdate, "birthdate");
        add_field_enum!(user.account_type, "account_type");
        add_field_enum!(user.fee, "fee");
        add_field!(user.public_type, "public_type");
        add_field!(user.notes, "notes");
        add_field!(user.group_id, "group_id");
        add_field!(user.barcode, "barcode");
        add_field!(user.status, "status");
        add_field!(user.sex, "sex");
        add_field!(user.staff_type, "staff_type");
        add_field!(user.hours_per_week, "hours_per_week");
        add_field!(user.staff_start_date, "staff_start_date");
        add_field!(user.staff_end_date, "staff_end_date");

        if password.is_some() {
            sets.push(format!("password = ${}", param_idx));
        }

        let query = format!(
            "UPDATE users SET {} WHERE id = {}",
            sets.join(", "),
            id
        );

        // Parse staff dates before binding
        let staff_start_date = user.staff_start_date.as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let staff_end_date = user.staff_end_date.as_ref()
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let hours_pw = user.hours_per_week.map(|v| v as f32);

        let mut builder = sqlx::query(&query).bind(now);

        macro_rules! bind_field {
            ($field:expr) => {
                if let Some(ref val) = $field {
                    builder = builder.bind(val);
                }
            };
        }
        
        macro_rules! bind_field_enum {
            ($field:expr) => {
                if let Some(ref val) = $field {
                    builder = builder.bind(val.as_str());
                }
            };
        }

        bind_field!(user.login);
        bind_field!(user.firstname);
        bind_field!(user.lastname);
        bind_field!(user.email);
        bind_field!(user.addr_street);
        bind_field!(user.addr_zip_code);
        bind_field!(user.addr_city);
        bind_field!(user.phone);
        bind_field!(user.birthdate);
        bind_field_enum!(user.account_type);
        bind_field_enum!(user.fee);
        bind_field!(user.public_type);
        bind_field!(user.notes);
        bind_field!(user.group_id);
        bind_field!(user.barcode);
        bind_field!(user.status);
        bind_field!(user.sex);
        bind_field!(user.staff_type);
        if user.hours_per_week.is_some() {
            builder = builder.bind(hours_pw);
        }
        if user.staff_start_date.is_some() {
            builder = builder.bind(staff_start_date);
        }
        if user.staff_end_date.is_some() {
            builder = builder.bind(staff_end_date);
        }

        if let Some(ref hash) = password {
            builder = builder.bind(hash);
        }

        builder.execute(&self.pool).await?;

        self.users_get_by_id(id).await
    }

    /// Delete a user (soft delete: anonymize data and set status to deleted)
    #[tracing::instrument(skip(self), err)]
    pub async fn users_delete(&self, id: i64, force: bool) -> AppResult<()> {
        let active_loans = self.loans_count_active_for_user(id).await?;

        if active_loans > 0 && !force {
            return Err(AppError::BusinessRule(
                "User has active loans. Use force=true to delete anyway.".to_string()
            ));
        }

        // Soft delete: anonymize personal data and set status to deleted
        let now = Utc::now();

        sqlx::query(r#"
            UPDATE users SET 
                firstname = NULL,
                lastname = NULL,
                password = NULL,
                email = NULL,
                phone = NULL,
                addr_street = NULL,
                addr_city = NULL,
                status = $1,
                archived_at = $2,
                update_at = $2
            WHERE id = $3
        "#)
            .bind(UserStatus::Deleted as i16)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
    
    /// Block a user
    #[tracing::instrument(skip(self), err)]
    pub async fn users_block(&self, id: i64) -> AppResult<User> {
        let now = Utc::now();

        sqlx::query("UPDATE users SET status = $1, update_at = $2 WHERE id = $3")
            .bind(UserStatus::Blocked as i16)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;

        self.users_get_by_id(id).await
    }
    
    /// Unblock a user
    #[tracing::instrument(skip(self), err)]
    pub async fn users_unblock(&self, id: i64) -> AppResult<User> {
        let now = Utc::now();

        sqlx::query("UPDATE users SET status = $1, update_at = $2 WHERE id = $3")
            .bind(UserStatus::Active as i16)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;

        self.users_get_by_id(id).await
    }

    /// Update user's own profile (firstname, lastname, password)
    #[tracing::instrument(skip(self), err)]
    pub async fn users_update_profile(&self, id: i64, profile: &UpdateProfile, password: Option<String>) -> AppResult<User> {
        let now = Utc::now();

        let mut sets = vec!["update_at = $1".to_string()];
        let mut param_idx = 2;

        // Helper macro to add fields
        macro_rules! add_field {
            ($field:expr, $name:expr) => {
                if $field.is_some() {
                    sets.push(format!("{} = ${}", $name, param_idx));
                    param_idx += 1;
                }
            };
        }

        add_field!(profile.firstname, "firstname");
        add_field!(profile.lastname, "lastname");
        add_field!(profile.email, "email");
        add_field!(profile.login, "login");
        add_field!(profile.addr_street, "addr_street");
        add_field!(profile.addr_zip_code, "addr_zip_code");
        add_field!(profile.addr_city, "addr_city");
        add_field!(profile.phone, "phone");
        add_field!(profile.birthdate, "birthdate");
        add_field!(profile.language, "language");
        
        if password.is_some() {
            sets.push(format!("password = ${}", param_idx));
        }

        let query = format!(
            "UPDATE users SET {} WHERE id = {}",
            sets.join(", "),
            id
        );

        let mut builder = sqlx::query(&query).bind(now);

        // Helper macro to bind fields
        macro_rules! bind_field {
            ($builder:expr, $field:expr) => {
                if let Some(ref val) = $field {
                    $builder = $builder.bind(val);
                }
            };
        }

        bind_field!(builder, profile.firstname);
        bind_field!(builder, profile.lastname);
        bind_field!(builder, profile.email);
        bind_field!(builder, profile.login);
        bind_field!(builder, profile.addr_street);
        bind_field!(builder, profile.addr_zip_code);
        bind_field!(builder, profile.addr_city);
        bind_field!(builder, profile.phone);
        bind_field!(builder, profile.birthdate);
        if let Some(ref lang) = profile.language {
            builder = builder.bind(lang.as_db_str());
        }
        
        if let Some(ref hash) = password {
            builder = builder.bind(hash);
        }

        builder.execute(&self.pool).await?;

        self.users_get_by_id(id).await
    }

    /// Update user's account type (admin only)
    #[tracing::instrument(skip(self), err)]
    pub async fn users_update_account_type(&self, id: i64, account_type: &AccountTypeSlug) -> AppResult<User> {
        let now = Utc::now();

        sqlx::query("UPDATE users SET account_type = $1, update_at = $2 WHERE id = $3")
            .bind(account_type.as_str())
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;

        self.users_get_by_id(id).await
    }
    
    /// Update 2FA settings for a user
    #[tracing::instrument(skip(self), err)]
    pub async fn users_update_2fa_settings(
        &self,
        id: i64,
        enabled: bool,
        method: Option<&str>,
        totp_secret: Option<&str>,
        recovery_codes: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE users 
            SET two_factor_enabled = $1, 
                two_factor_method = $2,
                totp_secret = $3,
                recovery_codes = $4,
                update_at = NOW()
            WHERE id = $5
            "#,
        )
        .bind(enabled)
        .bind(method)
        .bind(totp_secret)
        .bind(recovery_codes)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark a recovery code as used
    #[tracing::instrument(skip(self), err)]
    pub async fn users_mark_recovery_code_used(&self, id: i64, used_codes: &str) -> AppResult<()> {
        sqlx::query(
            "UPDATE users SET recovery_codes_used = $1, update_at = NOW() WHERE id = $2",
        )
        .bind(used_codes)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Fetch all active users with a non-empty email, optionally filtered by public_type.
    /// If `public_type` is None, all users with an email are returned (no filter).
    #[tracing::instrument(skip(self), err)]
    pub async fn users_get_emails_by_public_type(
        &self,
        public_type: Option<i64>,
    ) -> AppResult<Vec<UserEmailTarget>> {
        let rows = if let Some(pt) = public_type {
            sqlx::query_as::<_, UserEmailTarget>(
                r#"
                SELECT id, email, firstname, lastname, language
                FROM users
                WHERE email IS NOT NULL AND email <> ''
                  AND public_type = $1
                  AND (status IS NULL OR status != 2)
                "#,
            )
            .bind(pt)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, UserEmailTarget>(
                r#"
                SELECT id, email, firstname, lastname, language
                FROM users
                WHERE email IS NOT NULL AND email <> ''
                  AND (status IS NULL OR status != 2)
                "#,
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows)
    }
}
