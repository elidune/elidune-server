//! Dynamic (runtime-overridable) configuration management.
//!
//! Sections marked `overridable = true` in the config file can be updated at runtime
//! by admins via the API. Changes are persisted to the `settings` DB table and applied
//! immediately in memory via this struct.

use std::sync::{Arc, RwLock};
use regex::Regex;
use serde_json::Value;

use crate::{
    config::{AppConfig, AuditConfig, EmailConfig, LoggingConfig, RemindersConfig},
    error::{AppError, AppResult},
};

/// Callback type for hot-reloading the tracing log level at runtime.
/// Takes the new level string (e.g. "debug") and returns an error message on failure.
type LogLevelReloadFn = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Inner mutable state of the dynamic configuration
#[derive(Clone)]
struct DynamicConfigInner {
    pub email: EmailConfig,
    pub logging: LoggingConfig,
    pub reminders: RemindersConfig,
    pub audit: AuditConfig,
}

/// Thread-safe, runtime-mutable configuration.
/// Wraps the overridable sections. The original file-based config is kept for reset operations.
pub struct DynamicConfig {
    inner: RwLock<DynamicConfigInner>,
    /// Original file config, used to reset sections to their defaults
    pub file_config: AppConfig,
    /// Optional callback to hot-reload the tracing log level without restart
    log_level_reload: RwLock<Option<LogLevelReloadFn>>,
}

impl DynamicConfig {
    pub fn new(config: AppConfig) -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(DynamicConfigInner {
                email: config.email.clone(),
                logging: config.logging.clone(),
                reminders: config.reminders.clone(),
                audit: config.audit.clone(),
            }),
            file_config: config,
            log_level_reload: RwLock::new(None),
        })
    }

    /// Register a callback to hot-reload the tracing log level.
    /// Called once at startup after the tracing subscriber is initialized.
    pub fn set_log_level_reload(&self, f: LogLevelReloadFn) {
        *self.log_level_reload.write().unwrap() = Some(f);
    }

    /// Invoke the log level reload callback with the given level string.
    fn reload_log_level(&self, level: &str) {
        if let Some(f) = self.log_level_reload.read().unwrap().as_ref() {
            if let Err(e) = f(level) {
                tracing::warn!("Failed to reload log level to '{}': {}", level, e);
            } else {
                tracing::info!("Log level changed to '{}'", level);
            }
        }
    }

    pub fn read_email(&self) -> EmailConfig {
        self.inner.read().unwrap().email.clone()
    }

    pub fn read_logging(&self) -> LoggingConfig {
        self.inner.read().unwrap().logging.clone()
    }

    pub fn read_reminders(&self) -> RemindersConfig {
        self.inner.read().unwrap().reminders.clone()
    }

    pub fn read_audit(&self) -> AuditConfig {
        self.inner.read().unwrap().audit.clone()
    }

    /// Returns true if the given section is marked overridable in the file config.
    pub fn is_overridable(&self, section: &str) -> bool {
        match section {
            "email" => self.file_config.email.overridable,
            "logging" => self.file_config.logging.overridable,
            "reminders" => self.file_config.reminders.overridable,
            "audit" => self.file_config.audit.overridable,
            _ => false,
        }
    }

    /// Validate and apply a new config section from a JSON value.
    /// The section must be marked `overridable = true` in the file config.
    pub fn update_section(&self, section: &str, value: Value) -> AppResult<()> {
        if !self.is_overridable(section) {
            return Err(AppError::Authorization(format!(
                "Config section '{}' is not overridable",
                section
            )));
        }

        match section {
            "email" => {
                let cfg: EmailConfig = serde_json::from_value(value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid email config: {}", e)))?;
                validate_email_config(&cfg)?;
                self.inner.write().unwrap().email = cfg;
            }
            "logging" => {
                let cfg: LoggingConfig = serde_json::from_value(value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid logging config: {}", e)))?;
                validate_logging_config(&cfg)?;
                let new_level = cfg.level.clone();
                self.inner.write().unwrap().logging = cfg;
                self.reload_log_level(&new_level);
            }
            "reminders" => {
                let cfg: RemindersConfig = serde_json::from_value(value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid reminders config: {}", e)))?;
                validate_reminders_config(&cfg)?;
                self.inner.write().unwrap().reminders = cfg;
            }
            "audit" => {
                let cfg: AuditConfig = serde_json::from_value(value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid audit config: {}", e)))?;
                validate_audit_config(&cfg)?;
                self.inner.write().unwrap().audit = cfg;
            }
            _ => {
                return Err(AppError::NotFound(format!(
                    "Unknown config section '{}'",
                    section
                )));
            }
        }
        Ok(())
    }

    /// Reset a section to the value from the original file config.
    pub fn reset_section(&self, section: &str) -> AppResult<()> {
        if !self.is_overridable(section) {
            return Err(AppError::Authorization(format!(
                "Config section '{}' is not overridable",
                section
            )));
        }
        match section {
            "email" => self.inner.write().unwrap().email = self.file_config.email.clone(),
            "logging" => {
                let reset_level = self.file_config.logging.level.clone();
                self.inner.write().unwrap().logging = self.file_config.logging.clone();
                self.reload_log_level(&reset_level);
            }
            "reminders" => self.inner.write().unwrap().reminders = self.file_config.reminders.clone(),
            "audit" => self.inner.write().unwrap().audit = self.file_config.audit.clone(),
            _ => {
                return Err(AppError::NotFound(format!(
                    "Unknown config section '{}'",
                    section
                )));
            }
        }
        Ok(())
    }

    /// Serialize the current effective value of a section to JSON.
    pub fn get_section_value(&self, section: &str) -> AppResult<Value> {
        let val = match section {
            "email" => serde_json::to_value(self.read_email()),
            "logging" => serde_json::to_value(self.read_logging()),
            "reminders" => serde_json::to_value(self.read_reminders()),
            "audit" => serde_json::to_value(self.read_audit()),
            _ => return Err(AppError::NotFound(format!("Unknown config section '{}'", section))),
        };
        val.map_err(|e| AppError::Internal(format!("Failed to serialize config: {}", e)))
    }

    /// List of all overridable section keys.
    pub fn overridable_sections(&self) -> Vec<&'static str> {
        let mut sections = Vec::new();
        if self.file_config.email.overridable { sections.push("email"); }
        if self.file_config.logging.overridable { sections.push("logging"); }
        if self.file_config.reminders.overridable { sections.push("reminders"); }
        if self.file_config.audit.overridable { sections.push("audit"); }
        sections
    }
}

// ---- Validation helpers ----

fn validate_email_config(cfg: &EmailConfig) -> AppResult<()> {
    if cfg.smtp_host.trim().is_empty() {
        return Err(AppError::BadRequest("email.smtp_host must not be empty".to_string()));
    }
    if cfg.smtp_port == 0 {
        return Err(AppError::BadRequest("email.smtp_port must be between 1 and 65535".to_string()));
    }
    if cfg.smtp_from.trim().is_empty() {
        return Err(AppError::BadRequest("email.smtp_from must not be empty".to_string()));
    }
    if !cfg.smtp_from.contains('@') {
        return Err(AppError::BadRequest(
            "email.smtp_from must be a valid email address".to_string(),
        ));
    }
    Ok(())
}

fn validate_logging_config(cfg: &LoggingConfig) -> AppResult<()> {
    const LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];
    const FORMATS: &[&str] = &["pretty", "plain", "json"];
    const OUTPUTS: &[&str] = &["stdout", "stderr", "file", "syslog"];

    if !LEVELS.contains(&cfg.level.as_str()) {
        return Err(AppError::BadRequest(format!(
            "logging.level must be one of: {}",
            LEVELS.join(", ")
        )));
    }
    if !FORMATS.contains(&cfg.format.as_str()) {
        return Err(AppError::BadRequest(format!(
            "logging.format must be one of: {}",
            FORMATS.join(", ")
        )));
    }
    if !OUTPUTS.contains(&cfg.output.as_str()) {
        return Err(AppError::BadRequest(format!(
            "logging.output must be one of: {}",
            OUTPUTS.join(", ")
        )));
    }
    if cfg.output == "file" && cfg.file_path.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err(AppError::BadRequest(
            "logging.file_path is required when output = \"file\"".to_string(),
        ));
    }
    Ok(())
}

fn validate_reminders_config(cfg: &RemindersConfig) -> AppResult<()> {
    if cfg.frequency_days < 1 {
        return Err(AppError::BadRequest(
            "reminders.frequency_days must be at least 1".to_string(),
        ));
    }
    let hhmm = Regex::new(r"^\d{2}:\d{2}$").unwrap();
    if !hhmm.is_match(&cfg.send_time) {
        return Err(AppError::BadRequest(
            "reminders.send_time must be in HH:MM format (24h)".to_string(),
        ));
    }
    let parts: Vec<&str> = cfg.send_time.split(':').collect();
    let h: u32 = parts[0].parse().unwrap_or(99);
    let m: u32 = parts[1].parse().unwrap_or(99);
    if h > 23 || m > 59 {
        return Err(AppError::BadRequest(
            "reminders.send_time has invalid hour or minute value".to_string(),
        ));
    }
    Ok(())
}

fn validate_audit_config(cfg: &AuditConfig) -> AppResult<()> {
    if cfg.retention_days < 1 {
        return Err(AppError::BadRequest(
            "audit.retention_days must be at least 1".to_string(),
        ));
    }
    Ok(())
}
