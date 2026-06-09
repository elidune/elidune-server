use crate::{
    error::{AppError, AppResult},
};

pub const MIN_PASSWORD_LENGTH: usize = 12;

pub fn validate_password_strength(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(AppError::Validation(format!(
            "Password must be at least {MIN_PASSWORD_LENGTH} characters"
        )));
    }
    Ok(())
}
