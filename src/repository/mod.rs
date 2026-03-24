//! Repository layer for database operations.
//!
//! Each domain module defines its own `*Repository` trait next to the [`Repository`] inherent
//! methods and the forwarding `impl *Repository for Repository` (see module docs in each file).

pub mod biblios;
pub mod catalog_entities;
pub mod equipment;
pub mod events;
pub mod fines;
pub mod inventory;
pub mod loans;
pub mod maintenance;
pub mod public_types;
pub mod reservations;
pub mod schedules;
pub mod sources;
pub mod users;
pub mod visitor_counts;

pub use biblios::BibliosRepository;
pub use catalog_entities::CatalogEntitiesRepository;
pub use equipment::EquipmentRepository;
pub use events::{EventsRepository, EventsServiceRepository};
pub use fines::FinesRepository;
pub use inventory::InventoryRepository;
pub use loans::{LoansRepository, LoansServiceRepository};
pub use maintenance::MaintenanceRepository;
pub use public_types::PublicTypesRepository;
pub use reservations::ReservationsRepository;
pub use schedules::SchedulesRepository;
pub use sources::SourcesRepository;
pub use users::UsersRepository;
pub use visitor_counts::VisitorCountsRepository;

use sqlx::{Pool, Postgres};

/// Main repository struct holding database connection pool.
/// Methods are split across domain modules (items, loans, users, etc.) via separate `impl Repository` blocks.
#[derive(Clone)]
pub struct Repository {
    pub(crate) pool: Pool<Postgres>,
}

impl Repository {
    /// Create a new repository with the given database pool
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Expose the underlying pool for callers that need to begin transactions directly.
    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }
}
