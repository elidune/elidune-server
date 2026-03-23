//! User model and related types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::{Decode, Encode, FromRow, Postgres};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

use crate::error::AppError;
use super::Language;

/// User rights levels (matching original C implementation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rights {
    None = 0,
    Read = 1,
    Write = 2,
}

impl From<char> for Rights {
    fn from(c: char) -> Self {
        match c {
            'r' | 'R' => Rights::Read,
            'w' | 'W' => Rights::Write,
            _ => Rights::None,
        }
    }
}

impl From<Option<String>> for Rights {
    fn from(s: Option<String>) -> Self {
        s.and_then(|s| s.chars().next())
            .map(Rights::from)
            .unwrap_or(Rights::None)
    }
}



/// Account type slug (string identifier)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AccountTypeSlug {
    Guest,
    Reader,
    Librarian,
    Admin,
    Group,
}

impl AccountTypeSlug {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountTypeSlug::Guest => "guest",
            AccountTypeSlug::Reader => "reader",
            AccountTypeSlug::Librarian => "librarian",
            AccountTypeSlug::Admin => "admin",
            AccountTypeSlug::Group => "group",
        }
    }
}

impl std::fmt::Display for AccountTypeSlug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for AccountTypeSlug {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "guest" => Ok(AccountTypeSlug::Guest),
            "reader" => Ok(AccountTypeSlug::Reader),
            "librarian" => Ok(AccountTypeSlug::Librarian),
            "admin" => Ok(AccountTypeSlug::Admin),
            "group" => Ok(AccountTypeSlug::Group),
            _ => Err(format!("Invalid account type slug: {}", s)),
        }
    }
}

impl From<String> for AccountTypeSlug {
    fn from(s: String) -> Self {
        s.parse().unwrap_or_else(|_| AccountTypeSlug::Guest)
    }
}

impl From<&str> for AccountTypeSlug {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_else(|_| AccountTypeSlug::Guest)
    }
}

impl From<AccountTypeSlug> for String {
    fn from(slug: AccountTypeSlug) -> Self {
        slug.as_str().to_string()
    }
}

// SQLx conversion for AccountTypeSlug
impl sqlx::Type<Postgres> for AccountTypeSlug {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<Postgres>>::type_info()
    }
}

impl<'r> Decode<'r, Postgres> for AccountTypeSlug {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s: String = Decode::<Postgres>::decode(value)?;
        s.parse().map_err(|e: String| e.into())
    }
}

impl Encode<'_, Postgres> for AccountTypeSlug {
    fn encode_by_ref(&self, buf: &mut sqlx::postgres::PgArgumentBuffer) -> sqlx::encode::IsNull {
        let s: String = self.as_str().to_string();
        <String as Encode<Postgres>>::encode(s, buf)
    }
}

/// Fee slug (string identifier)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum FeeSlug {
    /// Known fee types
    #[serde(rename = "free")]
    Free,
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "foreigner")]
    Foreigner,
    /// Custom fee slug (for user-defined fees)
    Other(String),
}

impl FeeSlug {
    pub fn as_str(&self) -> &str {
        match self {
            FeeSlug::Free => "free",
            FeeSlug::Local => "local",
            FeeSlug::Foreigner => "foreigner",
            FeeSlug::Other(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for FeeSlug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for FeeSlug {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "free" => Ok(FeeSlug::Free),
            "local" => Ok(FeeSlug::Local),
            "foreigner" => Ok(FeeSlug::Foreigner),
            other => Ok(FeeSlug::Other(other.to_string())),
        }
    }
}

impl From<String> for FeeSlug {
    fn from(s: String) -> Self {
        s.parse().unwrap_or_else(|_| FeeSlug::Free)
    }
}

impl From<Option<String>> for FeeSlug {
    fn from(s: Option<String>) -> Self {
        s.map(|s| s.parse().unwrap_or_else(|_| FeeSlug::Free))
            .unwrap_or(FeeSlug::Free)
    }
}

impl From<&str> for FeeSlug {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_else(|_| FeeSlug::Free)
    }
}

impl From<FeeSlug> for Option<String> {
    fn from(slug: FeeSlug) -> Self {
        Some(slug.as_str().to_string())
    }
}

// Note: FeeSlug conversions are handled manually in repository code
// because SQLx doesn't support custom Decode/Encode for enums with Other(String) variant

/// User status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum UserStatus {
    Active = 0,
    Blocked = 1,
    Deleted = 2,
}

impl From<i16> for UserStatus {
    fn from(v: i16) -> Self {
        match v {
            0 => UserStatus::Active,
            1 => UserStatus::Blocked,
            2 => UserStatus::Deleted,
            _ => UserStatus::Active,
        }
    }
}

impl From<Option<i16>> for UserStatus {
    fn from(v: Option<i16>) -> Self {
        v.map(UserStatus::from).unwrap_or(UserStatus::Active)
    }
}

/// Internal row structure for database queries (with String fields)
#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    id: i64,
    group_id: Option<i64>,
    barcode: Option<String>,
    login: Option<String>,
    password: Option<String>,
    firstname: Option<String>,
    lastname: Option<String>,
    email: Option<String>,
    addr_street: Option<String>,
    addr_zip_code: Option<i32>,
    addr_city: Option<String>,
    phone: Option<String>,
    birthdate: Option<String>,
    created_at: Option<DateTime<Utc>>,
    update_at: Option<DateTime<Utc>>,
    issue_at: Option<DateTime<Utc>>,
    account_type: String,
    fee: Option<String>,
    public_type: Option<i64>,
    notes: Option<String>,
    status: Option<i16>,
    archived_at: Option<DateTime<Utc>>,
    language: Option<Language>,
    sex: Option<i16>,
    staff_type: Option<i16>,
    hours_per_week: Option<f32>,
    staff_start_date: Option<chrono::NaiveDate>,
    staff_end_date: Option<chrono::NaiveDate>,
    two_factor_enabled: Option<bool>,
    two_factor_method: Option<String>,
    totp_secret: Option<String>,
    recovery_codes: Option<String>,
    recovery_codes_used: Option<String>,
    receive_reminders: Option<bool>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        User {
            id: row.id,
            group_id: row.group_id,
            barcode: row.barcode,
            login: row.login,
            password: row.password,
            firstname: row.firstname,
            lastname: row.lastname,
            email: row.email,
            addr_street: row.addr_street,
            addr_zip_code: row.addr_zip_code,
            addr_city: row.addr_city,
            phone: row.phone,
            birthdate: row.birthdate,
            created_at: row.created_at,
            update_at: row.update_at,
            issue_at: row.issue_at,
            account_type: row.account_type.parse().unwrap_or(AccountTypeSlug::Guest),
            fee: row.fee.map(|f| f.parse().unwrap_or(FeeSlug::Free)),
            public_type: row.public_type,
            notes: row.notes,
            status: row.status,
            archived_at: row.archived_at,
            language: row.language,
            sex: row.sex,
            staff_type: row.staff_type,
            hours_per_week: row.hours_per_week.map(|v| v as f64),
            staff_start_date: row.staff_start_date.map(|d| d.to_string()),
            staff_end_date: row.staff_end_date.map(|d| d.to_string()),
            two_factor_enabled: row.two_factor_enabled,
            two_factor_method: row.two_factor_method,
            totp_secret: row.totp_secret,
            recovery_codes: row.recovery_codes,
            recovery_codes_used: row.recovery_codes_used,
            receive_reminders: row.receive_reminders.unwrap_or(true),
        }
    }
}

/// Full user model from database
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct User {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub group_id: Option<i64>,
    pub barcode: Option<String>,
    pub login: Option<String>,
    /// Hashed password (argon2)
    #[serde(skip_serializing)]
    pub password: Option<String>,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub email: Option<String>,
    pub addr_street: Option<String>,
    pub addr_zip_code: Option<i32>,
    pub addr_city: Option<String>,
    pub phone: Option<String>,
    pub birthdate: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub update_at: Option<DateTime<Utc>>,
    pub issue_at: Option<DateTime<Utc>>,
    pub account_type: AccountTypeSlug,
    pub fee: Option<FeeSlug>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub public_type: Option<i64>,
    pub notes: Option<String>,
    pub status: Option<i16>,
    pub archived_at: Option<DateTime<Utc>>,
    /// User preferred language
    pub language: Option<Language>,
    /// Sex (70=Female, 77=Male, 85=Unknown)
    pub sex: Option<i16>,
    /// Staff type (NULL=not staff, 0=employee, 1=volunteer)
    pub staff_type: Option<i16>,
    /// Contractual hours per week (for ETPT calculation)
    pub hours_per_week: Option<f64>,
    /// Staff start date (YYYY-MM-DD)
    pub staff_start_date: Option<String>,
    /// Staff end date (YYYY-MM-DD)
    pub staff_end_date: Option<String>,
    // 2FA fields
    pub two_factor_enabled: Option<bool>,
    pub two_factor_method: Option<String>,
    #[serde(skip_serializing)]
    pub totp_secret: Option<String>,
    #[serde(skip_serializing)]
    pub recovery_codes: Option<String>,
    #[serde(skip_serializing)]
    pub recovery_codes_used: Option<String>,
    /// Whether the user wants to receive overdue reminder emails
    pub receive_reminders: bool,
}

/// Internal row structure for UserShort queries
#[derive(Debug, Clone, FromRow)]
pub struct UserShortRow {
    id: i64,
    firstname: Option<String>,
    lastname: Option<String>,
    account_type: Option<String>,
    public_type: Option<i64>,
    nb_loans: Option<i64>,
    nb_late_loans: Option<i64>,
}

impl From<UserShortRow> for UserShort {
    fn from(row: UserShortRow) -> Self {
        UserShort {
            id: row.id,
            firstname: row.firstname,
            lastname: row.lastname,
            account_type: row.account_type.map(|s| s.parse().unwrap_or(AccountTypeSlug::Guest)),
            public_type: row.public_type,
            nb_loans: row.nb_loans,
            nb_late_loans: row.nb_late_loans,
        }
    }
}

/// Short user representation for lists
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserShort {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub account_type: Option<AccountTypeSlug>,
    pub public_type: Option<i64>,
    pub nb_loans: Option<i64>,
    pub nb_late_loans: Option<i64>,
}

/// User query parameters
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct UserQuery {
    pub name: Option<String>,
    pub barcode: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// User create/update body. `login` is required when creating; omit fields for partial update.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct UserPayload {
    pub barcode: Option<String>,
    /// Login (username); required on create, optional on update
    pub login: Option<String>,
    #[validate(length(min = 4, message = "Password must be at least 4 characters"))]
    pub password: Option<String>,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    pub addr_street: Option<String>,
    pub addr_zip_code: Option<i32>,
    pub addr_city: Option<String>,
    pub phone: Option<String>,
    pub birthdate: Option<String>,
    pub account_type: Option<AccountTypeSlug>,
    pub fee: Option<FeeSlug>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub public_type: Option<i64>,
    pub notes: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub group_id: Option<i64>,
    /// User status; for updates only (ignored on create)
    pub status: Option<i16>,
    /// Sex (70=Female, 77=Male, 85=Unknown)
    pub sex: Option<i16>,
    /// Staff type (NULL=not staff, 0=employee, 1=volunteer)
    pub staff_type: Option<i16>,
    /// Contractual hours per week
    pub hours_per_week: Option<f64>,
    /// Staff start date (YYYY-MM-DD)
    pub staff_start_date: Option<String>,
    /// Staff end date (YYYY-MM-DD)
    pub staff_end_date: Option<String>,
}

/// Update own profile request (for authenticated users)
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateProfile {
    /// First name
    pub firstname: Option<String>,
    /// Last name
    pub lastname: Option<String>,
    /// Email address (must be unique)
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    /// Login/username (must be unique if provided)
    #[validate(length(min = 3, message = "Login must be at least 3 characters"))]
    pub login: Option<String>,
    /// Street address
    pub addr_street: Option<String>,
    /// Zip code
    pub addr_zip_code: Option<i32>,
    /// City
    pub addr_city: Option<String>,
    /// Phone number
    pub phone: Option<String>,
    /// Birth date
    pub birthdate: Option<String>,
    /// Current password (required to change password)
    pub current_password: Option<String>,
    /// New password
    #[validate(length(min = 4, message = "Password must be at least 4 characters"))]
    pub new_password: Option<String>,
    /// Preferred language
    pub language: Option<Language>,
}

/// Update account type request (admin only)
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccountType {
    /// New account type slug (guest, reader, librarian, admin, group)
    pub account_type: AccountTypeSlug,
}

/// User rights structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRights {
    pub items_rights: Rights,
    pub users_rights: Rights,
    pub loans_rights: Rights,
    pub borrows_rights: Rights,
    pub settings_rights: Rights,
    pub items_archive_rights: Rights,
}

impl Default for UserRights {
    fn default() -> Self {
        Self {
            items_rights: Rights::None,
            users_rights: Rights::None,
            loans_rights: Rights::None,
            borrows_rights: Rights::None,
            settings_rights: Rights::None,
            items_archive_rights: Rights::None,
        }
    }
}

/// JWT Claims for authenticated users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserClaims {
    pub sub: String,
    pub user_id: i64,
    pub account_type: AccountTypeSlug,
    pub rights: UserRights,
    pub exp: i64,
    pub iat: i64,
}

impl UserClaims {
    /// Create a new JWT token
    pub fn create_token(&self, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
        use jsonwebtoken::{encode, EncodingKey, Header};
        encode(
            &Header::default(),
            self,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
    }

    /// Parse JWT token
    pub fn from_token(token: &str, secret: &str) -> Result<Self, jsonwebtoken::errors::Error> {
        use jsonwebtoken::{decode, DecodingKey, Validation};
        let token_data = decode::<Self>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::default(),
        )?;
        Ok(token_data.claims)
    }

    // Authorization checks
    pub fn require_read_items(&self) -> Result<(), AppError> {
        if self.rights.items_rights as u8 >= Rights::Read as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to read items".to_string()))
        }
    }

    pub fn require_write_items(&self) -> Result<(), AppError> {
        if self.rights.items_rights as u8 >= Rights::Write as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to write items".to_string()))
        }
    }

    pub fn require_read_users(&self) -> Result<(), AppError> {
        if self.rights.users_rights as u8 >= Rights::Read as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to read users".to_string()))
        }
    }

    pub fn require_write_users(&self) -> Result<(), AppError> {
        if self.rights.users_rights as u8 >= Rights::Write as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to write users".to_string()))
        }
    }

    pub fn require_read_catalog(&self) -> Result<(), AppError> {
        if self.rights.items_rights as u8 >= Rights::Read as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to read catalog".to_string()))
        }
    }

    pub fn require_read_borrows(&self) -> Result<(), AppError> {
        if self.rights.borrows_rights as u8 >= Rights::Read as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to read borrows".to_string()))
        }
    }

    pub fn require_read_loans(&self) -> Result<(), AppError> {
        if self.rights.loans_rights as u8 >= Rights::Read as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to read loans".to_string()))
        }
    }

    pub fn require_write_borrows(&self) -> Result<(), AppError> {
        if self.rights.borrows_rights as u8 >= Rights::Write as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to manage borrows".to_string()))
        }
    }

    pub fn require_read_settings(&self) -> Result<(), AppError> {
        if self.rights.settings_rights as u8 >= Rights::Read as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to read settings".to_string()))
        }
    }

    pub fn require_write_settings(&self) -> Result<(), AppError> {
        if self.rights.settings_rights as u8 >= Rights::Write as u8 {
            Ok(())
        } else {
            Err(AppError::Authorization("Insufficient rights to write settings".to_string()))
        }
    }

    /// Check if user is admin (account_type = "admin")
    pub fn is_admin(&self) -> bool {
        self.account_type == AccountTypeSlug::Admin
    }

    /// Check if user is librarian or admin
    pub fn is_librarian(&self) -> bool {
        matches!(self.account_type, AccountTypeSlug::Librarian | AccountTypeSlug::Admin)
    }

    /// Require admin privileges
    pub fn require_admin(&self) -> Result<(), AppError> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(AppError::Authorization("Administrator privileges required".to_string()))
        }
    }
}

