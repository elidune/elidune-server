//! Configuration management for Elidune server

use config::{Config, ConfigError, File};
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
    /// "daily" | "hourly" | "never" (default: "daily")
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
    #[serde(default)]
    pub meilisearch: Option<MeilisearchConfig>,
}

impl AppConfig {
    /// Load configuration from the given file path.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::from(path.as_ref().to_path_buf().as_path()).required(true))
            .build()?;
        config.try_deserialize()
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
