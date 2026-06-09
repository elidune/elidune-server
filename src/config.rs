//! Configuration management for Elidune server

use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Allowed CORS origins in production (e.g. `["https://app.example.com"]`).
    /// When absent or empty, all origins are allowed (development mode).
    #[serde(default)]
    pub cors_origins: Option<Vec<String>>,
    /// Requests per second allowed per IP on auth endpoints (default: 4).
    #[serde(default)]
    pub auth_rate_per_second: Option<u64>,
    /// Burst size for auth endpoint rate limiter (default: 2).
    #[serde(default)]
    pub auth_rate_burst: Option<u32>,
    /// Sustained requests per second per IP on public (OPAC/covers) endpoints (default: 30).
    #[serde(default)]
    pub public_rate_per_second: Option<u64>,
    /// Burst size for public endpoint rate limiter (default: 100).
    #[serde(default)]
    pub public_rate_burst: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UsersConfig {
    pub jwt_secret: String,
    pub jwt_expiration_hours: u64,
    /// When set, used as the reset link template if the client omits `resetUrl` on
    /// `POST /auth/request-password-reset`. Must contain the literal `<token>` placeholder.
    #[serde(default)]
    pub password_reset_url_template: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoggingConfig {
    pub level: String,
    /// "pretty" | "plain" | "json"
    pub format: String,
    /// "stdout" | "stderr" | "file" | "syslog"
    pub output: String,
    /// Path to log file; required when output = "file"
    pub file_path: Option<String>,
    /// "monthly" | "weekly" | "daily" | "never" (default: "daily"; only used when output = "file")
    pub file_rotation: Option<String>,
    /// Whether this section can be overridden via the DB settings table
    #[serde(default)]
    pub overridable: bool,
}

fn default_email_templates_dir() -> String {
    "data/email_templates".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_from: String,
    pub smtp_from_name: Option<String>,
    pub smtp_use_tls: bool,
    #[serde(default = "default_email_templates_dir")]
    pub templates_dir: String,
    /// Whether this section can be overridden via the DB settings table
    #[serde(default)]
    pub overridable: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RedisConfig {
    pub url: String,
    #[serde(default = "default_z3950_cache_ttl")]
    pub z3950_cache_ttl_seconds: u64,
}

fn default_z3950_cache_ttl() -> u64 {
    7 * 24 * 3600
}

fn default_meili_index() -> String {
    "items".to_string()
}

fn default_reminders_enabled() -> bool {
    true
}

fn default_reminder_frequency() -> u32 {
    7
}

fn default_reminder_time() -> String {
    "09:00".to_string()
}

fn default_smtp_throttle_ms() -> u64 {
    100
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RemindersConfig {
    /// Whether the automatic reminder scheduler is enabled
    #[serde(default = "default_reminders_enabled")]
    pub enabled: bool,
    /// Minimum days between two reminders for the same loan
    #[serde(default = "default_reminder_frequency")]
    pub frequency_days: u32,
    /// Time of day to send reminders automatically (HH:MM, 24h)
    #[serde(default = "default_reminder_time")]
    pub send_time: String,
    /// Delay in milliseconds between each email send to avoid SMTP rate limits
    #[serde(default = "default_smtp_throttle_ms")]
    pub smtp_throttle_ms: u64,
    /// Whether this section can be overridden via the DB settings table
    #[serde(default)]
    pub overridable: bool,
}

fn default_audit_retention() -> u32 {
    365
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuditConfig {
    /// Number of days to retain audit log entries (older entries are deleted by the scheduler)
    #[serde(default = "default_audit_retention")]
    pub retention_days: u32,
    /// Whether this section can be overridden via the DB settings table
    #[serde(default)]
    pub overridable: bool,
}

impl Default for RemindersConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            frequency_days: 7,
            send_time: "09:00".to_string(),
            smtp_throttle_ms: 100,
            overridable: false,
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            retention_days: 365,
            overridable: false,
        }
    }
}

fn default_hold_ready_expiry_days() -> u32 {
    7
}

/// Hold behaviour (pickup window when a copy becomes available).
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HoldsConfig {
    /// Days a `ready` hold stays valid for pickup (`expires_at` after notification).
    #[serde(default = "default_hold_ready_expiry_days")]
    pub ready_expiry_days: u32,
    /// Whether this section can be overridden via the DB `settings` table and admin API
    #[serde(default)]
    pub overridable: bool,
}

impl Default for HoldsConfig {
    fn default() -> Self {
        Self {
            ready_expiry_days: 7,
            overridable: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MeilisearchConfig {
    /// Meilisearch server URL, e.g. "http://meilisearch:7700"
    pub url: String,
    /// Master key / API key (optional for development without auth)
    pub api_key: Option<String>,
    /// Name of the Meilisearch index to use for catalog search
    #[serde(default = "default_meili_index")]
    pub index_name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub users: UsersConfig,
    pub logging: LoggingConfig,
    pub email: EmailConfig,
    pub redis: RedisConfig,
    #[serde(default)]
    pub reminders: RemindersConfig,
    #[serde(default)]
    pub audit: AuditConfig,
    /// Holds / physical item queue. Accepts legacy TOML section `[reservations]`.
    #[serde(default, alias = "reservations")]
    pub holds: HoldsConfig,
    #[serde(default)]
    pub meilisearch: Option<MeilisearchConfig>,
}

impl AppConfig {
    /// Load configuration from the given file path.
    pub fn load(path: Option<impl AsRef<Path>>) -> Result<Self, ConfigError> {

        let mut builder = Config::builder();
        if let Some(path) = path {
            builder = builder.add_source(File::from(path.as_ref().to_path_buf().as_path()).required(false));
        }
        
        let config = builder.add_source(
            Environment::with_prefix("ELIDUNE")
                .prefix_separator("_")
                .separator("__"),
        ).build()?;

        
        config.try_deserialize()
    }

    /// Reject insecure defaults before serving traffic.
    pub fn validate_security(&self) -> Result<(), String> {
        const DEFAULT_SECRET: &str = "change-this-secret-in-production";
        const MIN_SECRET_LEN: usize = 32;

        if self.users.jwt_secret == DEFAULT_SECRET {
            return Err(
                "users.jwt_secret is the sample default — set a strong secret via config or ELIDUNE_USERS__JWT_SECRET"
                    .into(),
            );
        }
        if self.users.jwt_secret.len() < MIN_SECRET_LEN {
            return Err(format!(
                "users.jwt_secret must be at least {MIN_SECRET_LEN} characters"
            ));
        }

        let prod_mode = std::env::var("ELIDUNE_PRODUCTION").ok().as_deref() == Some("true");
        if prod_mode {
            let origins = self.server.cors_origins.as_deref().unwrap_or(&[]);
            if origins.is_empty() {
                return Err(
                    "server.cors_origins must be set when ELIDUNE_PRODUCTION=true".into(),
                );
            }
        }
        Ok(())
    }


 
    /// Minimal configuration for integration tests (uses `DATABASE_URL` / `REDIS_URL` env vars).
    pub fn for_test() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://elidune:elidune@localhost:5432/elidune_test".into());
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());

        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig {
                url: database_url,
                max_connections: 5,
                min_connections: 1,
            },
            users: UsersConfig {
                jwt_secret: "integration-test-jwt-secret-min-32-chars".into(),
                jwt_expiration_hours: 24,
                password_reset_url_template: None,
            },
            logging: LoggingConfig::default(),
            email: EmailConfig {
                smtp_host: String::new(),
                smtp_port: 587,
                smtp_username: None,
                smtp_password: None,
                smtp_from: "test@elidune.local".into(),
                smtp_from_name: Some("Elidune Test".into()),
                smtp_use_tls: false,
                templates_dir: "data/email_templates".into(),
                overridable: true,
            },
            redis: RedisConfig {
                url: redis_url,
                z3950_cache_ttl_seconds: 3600,
            },
            reminders: RemindersConfig {
                enabled: false,
                ..RemindersConfig::default()
            },
            audit: AuditConfig::default(),
            holds: HoldsConfig {
                overridable: true,
                ..HoldsConfig::default()
            },
            meilisearch: None,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            cors_origins: None,
            auth_rate_per_second: None,
            auth_rate_burst: None,
            public_rate_per_second: None,
            public_rate_burst: None,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            output: "stdout".to_string(),
            file_path: None,
            file_rotation: None,
            overridable: false,
        }
    }
}
