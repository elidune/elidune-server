//! Repository layer for database operations.
//!
//! Each domain module defines its own `*Repository` trait next to the [`Repository`] inherent
//! methods (SQL) and a forwarding `impl *Repository for Repository`. The same pattern applies to
//! `settings` ([`RuntimeSettingsRepository`]), `z3950` ([`Z3950Repository`]), `library_info`
//! ([`LibraryInfoRepository`]), and `audit_log` ([`AuditLogRepository`]), not only older domains
//! like loans or biblios.

pub mod account_types;
pub mod audit_log;
pub mod biblios;
pub mod catalog_entities;
pub mod email_templates;
pub mod equipment;
pub mod events;
pub mod fines;
pub mod inventory;
pub mod library_info;
pub mod loans;
pub mod maintenance;
pub mod public_types;
pub mod reader_assistant;
pub mod holds;
pub mod schedules;
pub mod stats;
pub mod settings;
pub mod sources;
pub mod z3950;
pub mod users;
pub mod visitor_counts;

pub use account_types::AccountTypesCatalogRepository;
pub use audit_log::AuditLogRepository;
pub use biblios::BibliosRepository;
pub use catalog_entities::CatalogEntitiesRepository;
pub use email_templates::{EmailTemplateRow, EmailTemplatesRepository};
pub use equipment::EquipmentRepository;
pub use events::{EventsRepository, EventsServiceRepository};
pub use fines::FinesRepository;
pub use inventory::InventoryRepository;
pub use library_info::{LibraryInfoRepository, LibraryInfoSnapshot};
pub use loans::{LoansRepository, LoansServiceRepository};
pub use maintenance::MaintenanceRepository;
pub use public_types::PublicTypesRepository;
pub use reader_assistant::ReaderAssistantRepository;
pub use holds::HoldsRepository;
pub use schedules::SchedulesRepository;
pub use settings::RuntimeSettingsRepository;
pub use sources::SourcesRepository;
pub use users::UsersRepository;
pub use visitor_counts::VisitorCountsRepository;
pub use z3950::{Z3950Repository, Z3950ServerRecord};

use std::sync::Arc;

use sqlx::{Pool, Postgres};

use crate::{dynamic_config::DynamicConfig, email::EmailService};

/// Main repository struct holding database connection pool.
/// Methods are split across domain modules (items, loans, users, etc.) via separate `impl Repository` blocks.
#[derive(Clone)]
pub struct Repository {
    pub(crate) pool: Pool<Postgres>,
    /// When set, loan return uses live `holds.ready_expiry_days` from dynamic config.
    pub(crate) dynamic_config: Option<Arc<DynamicConfig>>,
    /// When set, patrons receive email when their hold becomes `ready` after a return.
    pub(crate) email_service: Option<Arc<EmailService>>,
}

impl Repository {
    /// Create a new repository with the given database pool.
    /// Pass `dynamic_config` from the main server so hold pickup windows follow TOML / DB settings.
    /// Pass `email_service` from the main server so hold-ready notifications can be sent.
    pub fn new(
        pool: Pool<Postgres>,
        dynamic_config: Option<Arc<DynamicConfig>>,
        email_service: Option<Arc<EmailService>>,
    ) -> Self {
        Self {
            pool,
            dynamic_config,
            email_service,
        }
    }

    /// Days until a `ready` hold expires (`expires_at`), from config or default **7** when no dynamic config.
    pub(crate) fn hold_ready_expiry_days(&self) -> i32 {
        self.dynamic_config
            .as_ref()
            .map(|dc| dc.read_holds().ready_expiry_days as i32)
            .unwrap_or_else(|| crate::config::HoldsConfig::default().ready_expiry_days as i32)
    }

    /// Expose the underlying pool for callers that need to begin transactions directly.
    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }
}
