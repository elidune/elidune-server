//! One-time initial setup when the database has no users and no `settings` rows.

use axum::{extract::State, http::StatusCode, Json};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use utoipa::ToSchema;

use crate::{
    config::EmailConfig,
    error::{AppError, AppResult},
    models::{
        user::{AccountTypeSlug, UpdateProfile},
        Language, Sex,
    },
    services::audit,
    AppState,
};

use super::{
    auth::UserInfo,
    library_info::{LibraryInfo, UpdateLibraryInfoRequest},
    ClientIp,
};

/// SMTP and related options for first setup (camelCase JSON). Mirrors file `EmailConfig`.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FirstSetupEmailBody {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_from: String,
    pub smtp_from_name: Option<String>,
    pub smtp_use_tls: bool,
    #[serde(default)]
    pub templates_dir: Option<String>,
}

impl FirstSetupEmailBody {
    fn into_email_config(self, file_defaults: &EmailConfig) -> EmailConfig {
        EmailConfig {
            smtp_host: self.smtp_host,
            smtp_port: self.smtp_port,
            smtp_username: self.smtp_username,
            smtp_password: self.smtp_password,
            smtp_from: self.smtp_from,
            smtp_from_name: self.smtp_from_name,
            smtp_use_tls: self.smtp_use_tls,
            templates_dir: self
                .templates_dir
                .unwrap_or_else(|| file_defaults.templates_dir.clone()),
            overridable: file_defaults.overridable,
        }
    }
}

/// Admin account fields for bootstrap (required patron fields are enforced).
#[serde_as]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FirstSetupAdminBody {
    pub login: String,
    pub password: String,
    pub firstname: String,
    pub lastname: String,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub email: Option<String>,
    pub sex: Sex,
    pub birthdate: NaiveDate,
    pub language: Option<Language>,
}

fn profile_language_only(lang: Language) -> UpdateProfile {
    UpdateProfile {
        firstname: None,
        lastname: None,
        email: None,
        login: None,
        addr_street: None,
        addr_zip_code: None,
        addr_city: None,
        phone: None,
        birthdate: None,
        current_password: None,
        new_password: None,
        language: Some(lang),
    }
}

/// Full first-setup payload: admin user, library details, optional runtime email override.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FirstSetupRequest {
    pub admin: FirstSetupAdminBody,
    pub library: UpdateLibraryInfoRequest,
    /// When set, persisted to `settings` and applied only if `email.overridable` is true in the server file config.
    pub email: Option<FirstSetupEmailBody>,
}

/// Response mirrors login success so the client can store the JWT immediately.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FirstSetupResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserInfo,
    pub library_info: LibraryInfo,
}

/// Single-shot bootstrap: create admin user, library row, optional email override.
#[utoipa::path(
    post,
    path = "/first_setup",
    tag = "health",
    request_body = FirstSetupRequest,
    responses(
        (status = 201, description = "Setup completed; returns JWT like login", body = FirstSetupResponse),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 409, description = "Setup already done or preconditions failed", body = crate::error::ErrorResponse),
    )
)]
pub async fn post_first_setup(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<FirstSetupRequest>,
) -> AppResult<(StatusCode, Json<FirstSetupResponse>)> {
    let repo = state.services.minimal_repository();
    if repo.users_count().await? != 0 || repo.settings_count().await? != 0 {
        return Err(AppError::Conflict(
            "Initial setup has already been completed".into(),
        ));
    }

    let default_public_type: i64 = repo
        .public_types_first_id()
        .await?
        .ok_or_else(|| {
            AppError::Internal(
                "No public_type row found; run database migrations first".into(),
            )
        })?;

    let addr_city = body
        .library
        .addr_city
        .clone()
        .or_else(|| body.library.name.clone())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            AppError::Validation(
                "library.addrCity or library.name is required for the patron record".into(),
            )
        })?;

    if body.email.is_some() && !state.dynamic_config.is_overridable("email") {
        return Err(AppError::BadRequest(
            "email.overridable must be true in the server config file to store SMTP from first setup"
                .into(),
        ));
    }

    let admin = &body.admin;
    let user_payload = crate::models::user::UserPayload {
        login: Some(admin.login.trim().to_string()),
        password: Some(admin.password.clone()),
        firstname: Some(admin.firstname.trim().to_string()),
        lastname: Some(admin.lastname.trim().to_string()),
        email: admin.email.as_ref().map(|e| e.trim().to_string()).filter(|s| !s.is_empty()),
        account_type: Some(AccountTypeSlug::Admin),
        sex: Some(admin.sex),
        birthdate: Some(admin.birthdate),
        public_type: Some(default_public_type),
        addr_city: Some(addr_city),
        ..Default::default()
    };

    let created = state.services.users.create_user(user_payload).await?;
    state
        .services
        .users
        .set_must_change_password(created.id, false)
        .await?;

    let user = if let Some(lang) = admin.language {
        repo.users_update_profile(created.id, &profile_language_only(lang), None)
            .await?
    } else {
        state.services.users.get_by_id(created.id).await?
    };

    let has_email_override = body.email.is_some();
    let library_info = state
        .services
        .library_info
        .update(body.library.clone())
        .await?;

    if let Some(email_body) = body.email {
        let merged = email_body.into_email_config(&state.config.email);
        let value = serde_json::to_value(&merged)
            .map_err(|e| AppError::Internal(format!("serialize email config: {e}")))?;
        state.dynamic_config.update_section("email", value.clone())?;
        repo
            .settings_upsert_section("email", &value)
            .await
            .map_err(|e| AppError::Internal(format!("persist email config: {e}")))?;
    }

    let token = state.services.users.issue_access_token(&user).await?;

    state.services.audit.log(
        audit::event::FIRST_SETUP_COMPLETED,
        Some(user.id),
        None,
        None,
        ip,
        Some(serde_json::json!({
            "libraryName": library_info.name,
            "hasEmailOverride": has_email_override,
        })),
    );

    Ok((
        StatusCode::CREATED,
        Json(FirstSetupResponse {
            token,
            token_type: "Bearer".to_string(),
            expires_in: (state.config.users.jwt_expiration_hours * 3600) as i64,
            user: UserInfo {
                id: user.id,
                login: user.login.unwrap_or_default(),
                email: user.email,
                firstname: user.firstname,
                lastname: user.lastname,
                addr_street: user.addr_street,
                addr_zip_code: user.addr_zip_code,
                addr_city: user.addr_city,
                phone: user.phone,
                birthdate: user.birthdate,
                account_type: user.account_type.to_string(),
                language: user.language.unwrap_or(Language::French),
            },
            library_info,
        }),
    ))
}

pub fn router() -> axum::Router<AppState> {
    use axum::routing::post;
    axum::Router::new().route("/first_setup", post(post_first_setup))
}
