//! Authentication and user management service

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

use std::collections::HashSet;
use totp_lite::totp_custom;

use crate::{
    config::UsersConfig,
    error::{AppError, AppResult},
    models::user::{AccountTypeSlug, UpdateProfile, User, UserClaims, UserPayload, UserQuery, UserShort},
    repository::Repository,
};

#[derive(Clone)]
pub struct UsersService {
    repository: Repository,
    config: UsersConfig,
    redis: crate::services::redis::RedisService,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PasswordResetClaims {
    sub: String,
    user_id: i64,
    purpose: String,
    exp: i64,
    iat: i64,
}

impl UsersService {
    pub fn new(repository: Repository, config: UsersConfig, redis: crate::services::redis::RedisService) -> Self {
        Self { repository, config, redis }
    }

    /// Authenticate user by login and return JWT token
    /// Returns (token, user) if 2FA is not enabled, or (None, user) if 2FA is required
    pub async fn authenticate(&self, login: &str, password: &str, device_id: Option<&str>) -> AppResult<(Option<String>, User)> {
        // Authenticate by login (primary method)
        let user = self.repository.users_get_by_login(login)
            .await?
            .ok_or_else(|| AppError::Authentication("Invalid login or password".to_string()))?;

        // Check if user is blocked or deleted
        if let Some(status) = user.status {
            if status == 1 {
                return Err(AppError::Authentication("Account is blocked".to_string()));
            }
            if status == 2 {
                return Err(AppError::Authentication("Invalid login or password".to_string()));
            }
        }

        // Check password
        let password_valid = self.verify_password(&user, password)?;
        if !password_valid {
            return Err(AppError::Authentication("Invalid login or password".to_string()));
        }

        // Check if 2FA is enabled
        if user.two_factor_enabled.unwrap_or(false) {
            // Check if device is trusted (bypass 2FA if trusted)
            if let Some(device) = device_id {
                let is_trusted = self.redis.is_device_trusted(user.id, device).await?;
                if is_trusted {
                    // Device is trusted, skip 2FA and create token directly
                    return self.create_token_for_user(&user).await.map(|token| (Some(token), user));
                }
            }
            // Return user without token - 2FA verification required
            return Ok((None, user));
        }

        // Get user rights
        let rights = self.repository.users_get_rights(&user.account_type).await?;

        // Create JWT token
        let now = Utc::now().timestamp();
        let exp = now + (self.config.jwt_expiration_hours as i64 * 3600);

        let claims = UserClaims {
            sub: user.login.clone().unwrap_or_default(),
            user_id: user.id,
            account_type: user.account_type.clone(),
            rights,
            exp,
            iat: now,
        };

        let token = claims
            .create_token(&self.config.jwt_secret)
            .map_err(|e| AppError::Internal(format!("Failed to create token: {}", e)))?;

        Ok((Some(token), user))
    }

    /// Verify 2FA code and return JWT token
    pub async fn verify_2fa(&self, user_id: i64, code: &str, device_id: Option<&str>, trust_device: bool) -> AppResult<String> {
        let user = self.repository.users_get_by_id(user_id).await?;

        if !user.two_factor_enabled.unwrap_or(false) {
            return Err(AppError::Validation("2FA is not enabled for this user".to_string()));
        }

        let method = user.two_factor_method.as_deref().unwrap_or("totp");

        let is_valid = match method {
            "totp" => {
                if let Some(ref secret) = user.totp_secret {
                    // Decode base32 secret to get original bytes
                    let secret_bytes = base32::decode(base32::Alphabet::RFC4648 { padding: false }, secret)
                        .ok_or_else(|| AppError::Internal("Invalid TOTP secret format".to_string()))?;
                    
                    let now = Utc::now().timestamp() as i64;
                    // totp_custom(step, digits, secret, time)
                    let totp_code = totp_custom::<sha1::Sha1>(30, 6, &secret_bytes, now as u64);
                    
                    code == totp_code
                } else {
                    false
                }
            }
            "email" => {
                // Verify email code from Redis
                self.redis.verify_2fa_code(user_id, code).await?
            }
            _ => return Err(AppError::Validation("Invalid 2FA method".to_string())),
        };

        if !is_valid {
            return Err(AppError::Authentication("Invalid 2FA code".to_string()));
        }

        // If device_id is provided and trust_device is true, store the device as trusted
        if trust_device {
            if let Some(device) = device_id {
                self.redis.store_trusted_device(user_id, device).await?;
            }
        }

        // Create token for user
        self.create_token_for_user(&user).await
    }

    /// Verify recovery code and return JWT token
    pub async fn verify_recovery_code(&self, user_id: i64, code: &str) -> AppResult<String> {
        let user = self.repository.users_get_by_id(user_id).await?;

        if !user.two_factor_enabled.unwrap_or(false) {
            return Err(AppError::Validation("2FA is not enabled for this user".to_string()));
        }

        let recovery_codes: Vec<String> = user
            .recovery_codes
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let used_codes: HashSet<String> = user
            .recovery_codes_used
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        if !recovery_codes.contains(&code.to_string()) {
            return Err(AppError::Authentication("Invalid recovery code".to_string()));
        }

        if used_codes.contains(&code.to_string()) {
            return Err(AppError::Authentication("Recovery code has already been used".to_string()));
        }

        // Mark code as used
        let mut new_used_codes = used_codes;
        new_used_codes.insert(code.to_string());
        let used_codes_json = serde_json::to_string(&new_used_codes)
            .map_err(|e| AppError::Internal(format!("Failed to serialize used codes: {}", e)))?;

        self.repository.users_mark_recovery_code_used(user_id, &used_codes_json).await?;

        // Create token
        self.create_token_for_user(&user).await
    }

    /// Create JWT token for a user
    async fn create_token_for_user(&self, user: &User) -> AppResult<String> {
        let rights = self.repository.users_get_rights(&user.account_type).await?;

        let now = Utc::now().timestamp();
        let exp = now + (self.config.jwt_expiration_hours as i64 * 3600);

        let claims = UserClaims {
            sub: user.login.clone().unwrap_or_default(),
            user_id: user.id,
            account_type: user.account_type.clone(),
            rights,
            exp,
            iat: now,
        };

        claims
            .create_token(&self.config.jwt_secret)
            .map_err(|e| AppError::Internal(format!("Failed to create token: {}", e)))
    }

    /// Generate TOTP secret and provisioning URI
    pub fn setup_totp(&self, user: &User) -> AppResult<(String, String)> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut secret_bytes = [0u8; 20];
        rng.fill(&mut secret_bytes);
        let secret = base32::encode(base32::Alphabet::RFC4648 { padding: false }, &secret_bytes);

        let issuer = "Elidune";
        let account_name = match &user.login {
            Some(login) => login.as_str(),
            None => {
                // Format will be handled in label
                ""
            }
        };
        let label = if account_name.is_empty() {
            format!("{}:user_{}", issuer, user.id)
        } else {
            format!("{}:{}", issuer, account_name)
        };
        let uri = format!(
            "otpauth://totp/{}?secret={}&issuer={}",
            label, secret, issuer
        );

        Ok((secret, uri))
    }

    /// Generate recovery codes
    pub fn generate_recovery_codes(&self, count: usize) -> Vec<String> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..count)
            .map(|_| {
                let num = rng.gen_range(100000..999999);
                format!("{:06}", num)
            })
            .collect()
    }

    /// Enable 2FA for a user
    pub async fn enable_2fa(
        &self,
        user_id: i64,
        method: &str,
        totp_secret: Option<String>,
    ) -> AppResult<Vec<String>> {
        if method != "totp" && method != "email" {
            return Err(AppError::Validation("Invalid 2FA method. Must be 'totp' or 'email'".to_string()));
        }

        let user = self.get_by_id(user_id).await?;

        if method == "email" && user.email.is_none() {
            return Err(AppError::Validation("Email is required for email-based 2FA".to_string()));
        }

        let recovery_codes = self.generate_recovery_codes(10);
        let recovery_codes_json = serde_json::to_string(&recovery_codes)
            .map_err(|e| AppError::Internal(format!("Failed to serialize recovery codes: {}", e)))?;

        self.repository.users_update_2fa_settings(
            user_id,
            true,
            Some(method),
            totp_secret.as_deref(),
            Some(&recovery_codes_json),
        )
        .await?;

        Ok(recovery_codes)
    }

    /// Disable 2FA for a user
    pub async fn disable_2fa(&self, user_id: i64) -> AppResult<()> {
        self.repository.users_update_2fa_settings(user_id, false, None, None, None).await?;

        Ok(())
    }

    /// Verify user password
    fn verify_password(&self, user: &User, password: &str) -> AppResult<bool> {
        // First try the new hashed password
        if let Some(ref hash) = user.password {
            let parsed_hash = PasswordHash::new(hash)
                .map_err(|_| AppError::Internal("Invalid password hash".to_string()))?;
            return Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok());
        }

        Ok(false)
    }

    /// Hash a password using Argon2
    pub fn hash_password(&self, password: &str) -> AppResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))?;
        Ok(hash.to_string())
    }

    /// Get user by ID
    pub async fn get_by_id(&self, id: i64) -> AppResult<User> {
        self.repository.users_get_by_id(id).await
    }

    /// Search users
    pub async fn search_users(&self, query: &UserQuery) -> AppResult<(Vec<UserShort>, i64)> {
        self.repository.users_search(query).await
    }

    /// Create a new user
    pub async fn create_user(&self, mut user: UserPayload) -> AppResult<User> {
        let login = user
            .login
            .take()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Validation("Login is required".to_string()))?;
        if login.len() < 3 {
            return Err(AppError::Validation(
                "Login must be at least 3 characters".to_string(),
            ));
        }

        if self.repository.users_login_exists(&login, None).await? {
            return Err(AppError::Conflict("Login already exists".to_string()));
        }

        // Email is optional, no uniqueness check needed

        // Hash password if provided
        let password = if let Some(ref password) = user.password {
            Some(self.hash_password(password)?)
        } else {
            None
        };

        user.login = Some(login);

        self.repository.users_create(&user, password).await
    }

    /// Update an existing user
    pub async fn update_user(&self, id: i64, user: UserPayload) -> AppResult<User> {
        // Check if user exists
        self.repository.users_get_by_id(id).await?;

        // Check if login already exists for another user (login is required and unique)
        if let Some(ref login) = user.login {
            if self.repository.users_login_exists(login, Some(id)).await? {
                return Err(AppError::Conflict("Login already exists".to_string()));
            }
        }
        // Email is optional, no uniqueness check needed

        // Hash password if provided
        let password = if let Some(ref password) = user.password {
            Some(self.hash_password(password)?)
        } else {
            None
        };

        self.repository.users_update(id, &user, password).await
    }

    /// Delete a user
    pub async fn delete_user(&self, id: i64, force: bool) -> AppResult<()> {
        self.repository.users_delete(id, force).await
    }

    /// Update user's own profile (name, password)
    pub async fn update_profile(&self, user_id: i64, profile: UpdateProfile) -> AppResult<User> {
        // Get current user
        let user = self.repository.users_get_by_id(user_id).await?;

        // Check if login already exists for another user (login is required and unique)
        if let Some(ref login) = profile.login {
            if self.repository.users_login_exists(login, Some(user_id)).await? {
                return Err(AppError::Conflict("Login already exists".to_string()));
            }
        }
        // Email is optional, no uniqueness check needed

        // If changing password, verify current password
        if profile.new_password.is_some() {
            let current_password = profile.current_password.as_ref()
                .ok_or_else(|| AppError::Validation("Current password required to change password".to_string()))?;
            
            if !self.verify_password(&user, current_password)? {
                return Err(AppError::Authentication("Current password is incorrect".to_string()));
            }
        }

        // Hash new password if provided
        let password = if let Some(ref new_password) = profile.new_password {
            Some(self.hash_password(new_password)?)
        } else {
            None
        };

        // Update only allowed fields
        self.repository.users_update_profile(user_id, &profile, password).await
    }

    /// Update user's account type (admin only)
    pub async fn update_account_type(&self, user_id: i64, account_type: &AccountTypeSlug) -> AppResult<User> {
        // Check if user exists
        self.repository.users_get_by_id(user_id).await?;

        // Account type is already validated by the enum type

        self.repository.users_update_account_type(user_id, account_type).await
    }

    /// Request password reset by login or email.
    /// Returns destination email, reset token, and user language for email template.
    pub async fn request_password_reset(
        &self,
        identifier: &str,
    ) -> AppResult<(String, String, Option<crate::models::Language>)> {
        let user = if let Some(u) = self.repository.users_get_by_login(identifier).await? {
            u
        } else if let Some(u) = self.repository.users_get_by_email(identifier).await? {
            u
        } else {
            return Err(AppError::NotFound("User not found".to_string()));
        };

        let email = user
            .email
            .as_deref()
            .ok_or_else(|| AppError::Validation("No email configured for this user".to_string()))?;

        let now = Utc::now().timestamp();
        let exp = now + (30 * 60); // 30 minutes
        let claims = PasswordResetClaims {
            sub: user.login.clone().unwrap_or_else(|| format!("user_{}", user.id)),
            user_id: user.id,
            purpose: "password_reset".to_string(),
            exp,
            iat: now,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.config.jwt_secret.as_bytes()),
        )
        .map_err(|e| AppError::Internal(format!("Failed to create reset token: {}", e)))?;

        Ok((email.to_string(), token, user.language))
    }

    /// Reset password using a reset token and a new password.
    pub async fn reset_password(&self, token: &str, new_password: &str) -> AppResult<()> {
        let token_data = decode::<PasswordResetClaims>(
            token,
            &DecodingKey::from_secret(self.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| AppError::Authentication("Invalid or expired reset token".to_string()))?;

        let claims = token_data.claims;
        if claims.purpose != "password_reset" {
            return Err(AppError::Authentication("Invalid reset token purpose".to_string()));
        }

        let hash = self.hash_password(new_password)?;
        self.repository.users_update_password(claims.user_id, &hash).await
    }
}

