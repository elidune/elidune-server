//! Elidune Library Management System
//!
//! A modern Rust implementation of the Elidune library management server,
//! providing a REST JSON API for managing library catalogs, users, and loans.

use std::sync::Arc;

pub mod api;
pub mod config;
pub mod dynamic_config;
pub mod error;
pub mod marc;
pub mod models;
pub mod repository;
pub mod services;

pub use config::AppConfig;
pub use dynamic_config::DynamicConfig;
pub use error::{AppError, AppResult};

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub dynamic_config: Arc<DynamicConfig>,
    pub services: Arc<services::Services>,
}
