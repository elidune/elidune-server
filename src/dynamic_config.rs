//! Dynamic (runtime-overridable) configuration management.
//!
//! Sections marked `overridable = true` in the config file can be updated at runtime
//! by admins via the API. Changes are persisted to the `settings` DB table and applied
//! immediately in memory via this struct.

use std::path::Path;
use std::sync::{Arc, RwLock};

use regex::Regex;
use serde_json::Value;
use sqlx::{Pool, Postgres};

use crate::{
    bootstrap::logging::{self, TracingGuard},
    config::{AppConfig, AuditConfig, EmailConfig, HoldsConfig, LoggingConfig, RemindersConfig},
    error::{AppError, AppResult},
    repository::Repository,
};

/// Overridable config section keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Email,
    Logging,
    Reminders,
    Audit,
    Holds,
}

impl Section {
    const ALL: [Self; 5] = [
        Self::Email,
        Self::Logging,
        Self::Reminders,
        Self::Audit,
        Self::Holds,
    ];

    fn key(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Logging => "logging",
            Self::Reminders => "reminders",
            Self::Audit => "audit",
            Self::Holds => "holds",
        }
    }

    fn try_from_key(key: &str) -> Option<Self> {
        match key {
            "email" => Some(Self::Email),
            "logging" => Some(Self::Logging),
            "reminders" => Some(Self::Reminders),
            "audit" => Some(Self::Audit),
            "holds" => Some(Self::Holds),
            _ => None,
        }
    }

    fn overridable(self, config: &AppConfig) -> bool {
        match self {
            Self::Email => config.email.overridable,
            Self::Logging => config.logging.overridable,
            Self::Reminders => config.reminders.overridable,
            Self::Audit => config.audit.overridable,
            Self::Holds => config.holds.overridable,
        }
    }

    fn apply_override(self, config: &mut AppConfig, value: Value) -> bool {
        match self {
            Self::Email => serde_json::from_value(value)
                .ok()
                .map(|v| {
                    config.email = v;
                    true
                })
                .unwrap_or(false),
            Self::Logging => serde_json::from_value(value)
                .ok()
                .map(|v| {
                    config.logging = v;
                    true
                })
                .unwrap_or(false),
            Self::Reminders => serde_json::from_value(value)
                .ok()
                .map(|v| {
                    config.reminders = v;
                    true
                })
                .unwrap_or(false),
            Self::Audit => serde_json::from_value(value)
                .ok()
                .map(|v| {
                    config.audit = v;
                    true
                })
                .unwrap_or(false),
            Self::Holds => serde_json::from_value(value)
                .ok()
                .map(|v| {
                    config.holds = v;
                    true
                })
                .unwrap_or(false),
        }
    }
}

/// Inner mutable state of the dynamic configuration.
#[derive(Clone)]
struct DynamicConfigInner {
    pub email: EmailConfig,
    pub logging: LoggingConfig,
    pub reminders: RemindersConfig,
    pub audit: AuditConfig,
    pub holds: HoldsConfig,
}

/// Guard returned by [`DynamicConfig::apply`]; must be kept alive for the process lifetime.
pub struct ApplyGuard {
    _tracing: TracingGuard,
}

/// Thread-safe, runtime-mutable configuration.
/// Wraps the overridable sections. The original file-based config is kept for reset operations.
pub struct DynamicConfig {
    inner: RwLock<DynamicConfigInner>,
    /// Original file config, used to reset sections to their defaults.
    pub file_config: AppConfig,
    /// DB-overridden section keys loaded at startup (for logging after `apply`).
    db_overrides: Vec<String>,
    logging_reload: RwLock<Option<Arc<logging::LoggingReload>>>,
}

impl DynamicConfig {
    /// Build from an already-merged effective config (file + optional DB overrides).
    pub fn new(config: AppConfig) -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(DynamicConfigInner {
                email: config.email.clone(),
                logging: config.logging.clone(),
                reminders: config.reminders.clone(),
                audit: config.audit.clone(),
                holds: config.holds.clone(),
            }),
            file_config: config,
            db_overrides: Vec::new(),
            logging_reload: RwLock::new(None),
        })
    }

    /// Load DB settings overrides and merge into the effective runtime config.
    /// The original file config is preserved for [`reset_section`].
    pub async fn load_with_db_overrides(file_config: AppConfig, pool: &Pool<Postgres>) -> Arc<Self> {
        let original_file = file_config.clone();
        let mut effective = file_config;
        let mut db_overrides = Vec::new();

        let db_settings = Repository::new(pool.clone(), None)
            .settings_load_overrides()
            .await
            .unwrap_or_default();

        for (key, value) in db_settings {
            let Some(section) = Section::try_from_key(&key) else {
                continue;
            };
            if !section.overridable(&effective) {
                continue;
            }
            if section.apply_override(&mut effective, value) {
                db_overrides.push(key);
            }
        }

        Arc::new(Self {
            inner: RwLock::new(DynamicConfigInner {
                email: effective.email.clone(),
                logging: effective.logging.clone(),
                reminders: effective.reminders.clone(),
                audit: effective.audit.clone(),
                holds: effective.holds.clone(),
            }),
            file_config: original_file,
            db_overrides,
            logging_reload: RwLock::new(None),
        })
    }

    /// Apply all effective configuration side effects at startup.
    ///
    /// Call once after [`load_with_db_overrides`]: initialises tracing from the merged
    /// logging config, registers the runtime reload hook, and seeds email templates.
    pub async fn apply(self: &Arc<Self>, pool: &Pool<Postgres>) -> AppResult<ApplyGuard> {
        let logging = self.read_logging();
        validate_logging_config(&logging)?;

        let tracing_guard = logging::init(&logging)
            .map_err(|e| AppError::Internal(format!("tracing init: {e}")))?;

        self.register_logging_reload(tracing_guard.reload());

        if !self.db_overrides.is_empty() {
            tracing::info!(
                "DB config overrides applied: [{}]",
                self.db_overrides.join(", ")
            );
        }

        let templates_dir = self.read_email().templates_dir.clone();
        if let Err(e) = crate::email_templates::bootstrap_from_files(
            pool,
            Path::new(&templates_dir),
        )
        .await
        {
            tracing::warn!("Email templates bootstrap failed: {e}");
        }

        tracing::info!(
            "Effective config ready (logging.level={}, logging.output={})",
            logging.level,
            logging.output
        );

        Ok(ApplyGuard {
            _tracing: tracing_guard,
        })
    }

    fn register_logging_reload(&self, reload: Arc<logging::LoggingReload>) {
        *self.logging_reload.write().unwrap() = Some(reload);
    }

    fn reload_logging(&self) {
        let Some(reload) = self.logging_reload.read().unwrap().clone() else {
            return;
        };
        let cfg = self.read_logging();
        match reload.reload(&cfg) {
            Ok(()) => {
                // Log after the layer swap; JSON omits span context so this is safe mid-request.
                tracing::info!(
                    "Logging reloaded (level={}, format={}, output={})",
                    cfg.level,
                    cfg.format,
                    cfg.output
                );
            }
            Err(e) => eprintln!("Failed to reload logging config: {e}"),
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

    pub fn read_holds(&self) -> HoldsConfig {
        self.inner.read().unwrap().holds.clone()
    }

    /// Returns true if the given section is marked overridable in the file config.
    pub fn is_overridable(&self, section: &str) -> bool {
        Section::try_from_key(section)
            .map(|s| s.overridable(&self.file_config))
            .unwrap_or(false)
    }

    /// Validate and apply a new config section from a JSON value.
    pub fn update_section(&self, section: &str, value: Value) -> AppResult<()> {
        let section = Section::try_from_key(section).ok_or_else(|| {
            AppError::NotFound(format!("Unknown config section '{section}'"))
        })?;

        if !section.overridable(&self.file_config) {
            return Err(AppError::Authorization(format!(
                "Config section '{}' is not overridable",
                section.key()
            )));
        }

        match section {
            Section::Email => {
                let cfg: EmailConfig = serde_json::from_value(value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid email config: {e}")))?;
                validate_email_config(&cfg)?;
                self.inner.write().unwrap().email = cfg;
            }
            Section::Logging => {
                let cfg: LoggingConfig = serde_json::from_value(value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid logging config: {e}")))?;
                validate_logging_config(&cfg)?;
                self.inner.write().unwrap().logging = cfg;
                self.reload_logging();
            }
            Section::Reminders => {
                let cfg: RemindersConfig = serde_json::from_value(value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid reminders config: {e}")))?;
                validate_reminders_config(&cfg)?;
                self.inner.write().unwrap().reminders = cfg;
            }
            Section::Audit => {
                let cfg: AuditConfig = serde_json::from_value(value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid audit config: {e}")))?;
                validate_audit_config(&cfg)?;
                self.inner.write().unwrap().audit = cfg;
            }
            Section::Holds => {
                let cfg: HoldsConfig = serde_json::from_value(value)
                    .map_err(|e| AppError::BadRequest(format!("Invalid holds config: {e}")))?;
                validate_holds_config(&cfg)?;
                self.inner.write().unwrap().holds = cfg;
            }
        }
        Ok(())
    }

    /// Reset a section to the value from the original file config.
    pub fn reset_section(&self, section: &str) -> AppResult<()> {
        let section = Section::try_from_key(section).ok_or_else(|| {
            AppError::NotFound(format!("Unknown config section '{section}'"))
        })?;

        if !section.overridable(&self.file_config) {
            return Err(AppError::Authorization(format!(
                "Config section '{}' is not overridable",
                section.key()
            )));
        }

        match section {
            Section::Email => {
                self.inner.write().unwrap().email = self.file_config.email.clone();
            }
            Section::Logging => {
                self.inner.write().unwrap().logging = self.file_config.logging.clone();
                self.reload_logging();
            }
            Section::Reminders => {
                self.inner.write().unwrap().reminders = self.file_config.reminders.clone();
            }
            Section::Audit => {
                self.inner.write().unwrap().audit = self.file_config.audit.clone();
            }
            Section::Holds => {
                self.inner.write().unwrap().holds = self.file_config.holds.clone();
            }
        }
        Ok(())
    }

    /// Serialize the current effective value of a section to JSON.
    pub fn get_section_value(&self, section: &str) -> AppResult<Value> {
        let val = match Section::try_from_key(section) {
            Some(Section::Email) => serde_json::to_value(self.read_email()),
            Some(Section::Logging) => serde_json::to_value(self.read_logging()),
            Some(Section::Reminders) => serde_json::to_value(self.read_reminders()),
            Some(Section::Audit) => serde_json::to_value(self.read_audit()),
            Some(Section::Holds) => serde_json::to_value(self.read_holds()),
            None => {
                return Err(AppError::NotFound(format!(
                    "Unknown config section '{section}'"
                )));
            }
        };
        val.map_err(|e| AppError::Internal(format!("Failed to serialize config: {e}")))
    }

    /// List of all overridable section keys.
    pub fn overridable_sections(&self) -> Vec<&'static str> {
        Section::ALL
            .iter()
            .copied()
            .filter(|s| s.overridable(&self.file_config))
            .map(Section::key)
            .collect()
    }
}

// ---- Validation helpers ----

fn validate_email_config(cfg: &EmailConfig) -> AppResult<()> {
    if cfg.smtp_host.trim().is_empty() {
        return Err(AppError::BadRequest(
            "email.smtp_host must not be empty".to_string(),
        ));
    }
    if cfg.smtp_port == 0 {
        return Err(AppError::BadRequest(
            "email.smtp_port must be between 1 and 65535".to_string(),
        ));
    }
    if cfg.smtp_from.trim().is_empty() {
        return Err(AppError::BadRequest(
            "email.smtp_from must not be empty".to_string(),
        ));
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
    if let Some(rotation) = cfg.file_rotation.as_deref() {
        const ROTATIONS: &[&str] = &["monthly", "weekly", "daily", "never"];
        if !ROTATIONS.contains(&rotation) {
            return Err(AppError::BadRequest(format!(
                "logging.file_rotation must be one of: {}",
                ROTATIONS.join(", ")
            )));
        }
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

fn validate_holds_config(cfg: &HoldsConfig) -> AppResult<()> {
    if cfg.ready_expiry_days < 1 || cfg.ready_expiry_days > 365 {
        return Err(AppError::BadRequest(
            "holds.ready_expiry_days must be between 1 and 365".to_string(),
        ));
    }
    Ok(())
}
