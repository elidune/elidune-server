//! Plaintext credentials wrapped so `Debug` / tracing never leak them.

use secrecy::{Secret, Zeroize};
use serde::{Deserialize, Serialize};
use validator::ValidationError;

use crate::auth_policy::validate_password_strength;

/// Inner value for [`PlaintextPassword`]. Implements [`secrecy::SerializableSecret`] so
/// admin APIs (Z39.50 settings, OpenAPI) can serialize responses without logging secrets via `Debug`.
#[derive(Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct PlaintextSecret(String);

impl PlaintextSecret {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zeroize for PlaintextSecret {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl secrecy::SerializableSecret for PlaintextSecret {}

impl secrecy::DebugSecret for PlaintextSecret {}

impl secrecy::CloneableSecret for PlaintextSecret {}

/// User/catalog/SMTP password in memory (never redacted in intentional JSON responses; redacted in `Debug`).
pub type PlaintextPassword = Secret<PlaintextSecret>;

pub use secrecy::ExposeSecret;

#[must_use]
pub fn plaintext_password(s: impl Into<String>) -> PlaintextPassword {
    PlaintextPassword::new(PlaintextSecret(s.into()))
}

/// Non-empty secret (e.g. login password).
pub fn validate_nonempty_secret(value: &PlaintextPassword) -> Result<(), ValidationError> {
    if value.expose_secret().as_str().trim().is_empty() {
        let mut err = ValidationError::new("length");
        err.message = Some("password is required".into());
        return Err(err);
    }
    Ok(())
}

/// Minimum length policy for a required secret field.
pub fn validate_password_strength_secret(value: &PlaintextPassword) -> Result<(), ValidationError> {
    match validate_password_strength(value.expose_secret().as_str()) {
        Ok(()) => Ok(()),
        Err(crate::error::AppError::Validation(msg)) => {
            let mut err = ValidationError::new("password_strength");
            err.message = Some(msg.into());
            Err(err)
        }
        Err(_) => Err(ValidationError::new("password_strength")),
    }
}

#[must_use]
pub fn optional_exposed_str(opt: &Option<PlaintextPassword>) -> Option<&str> {
    opt.as_ref()
        .map(|s| s.expose_secret().as_str())
}

#[must_use]
pub fn optional_exposed_string(opt: &Option<PlaintextPassword>) -> Option<String> {
    opt.as_ref()
        .map(|s| s.expose_secret().as_str().to_string())
}
