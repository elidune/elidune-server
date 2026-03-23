//! Business logic services

pub mod audit;
pub mod catalog;
pub mod email;
pub mod email_templates;
pub mod equipment;
pub mod events;
pub mod fines;
pub mod inventory;
pub mod library_info;
pub mod loans;
pub mod marc;
pub mod public_types;
pub mod redis;
pub mod reminders;
pub mod reservations;
pub mod schedules;
pub mod scheduler;
pub mod search;
pub mod settings;
pub mod sources;
pub mod stats;
pub mod users;
pub mod visitor_counts;
pub mod z3950;

use std::sync::Arc;

use sqlx::{Pool, Postgres};

use crate::{
    config::{MeilisearchConfig, RedisConfig, UsersConfig},
    dynamic_config::DynamicConfig,
    error::AppResult,
    repository::{
        BibliosRepository, CatalogEntitiesRepository, EquipmentRepository, EventsServiceRepository,
        FinesRepository, InventoryRepository, LoansRepository, LoansServiceRepository,
        PublicTypesRepository, Repository, ReservationsRepository, SchedulesRepository,
        SourcesRepository, UsersRepository, VisitorCountsRepository,
    },
};

/// Container for all services
#[derive(Clone)]
pub struct Services {
    pub audit: audit::AuditService,
    pub catalog: catalog::CatalogService,
    pub email: email::EmailService,
    pub equipment: equipment::EquipmentService,
    pub events: events::EventsService,
    pub fines: fines::FinesService,
    pub inventory: inventory::InventoryService,
    pub library_info: library_info::LibraryInfoService,
    pub loans: loans::LoansService,
    pub marc: marc::MarcService,
    pub public_types: public_types::PublicTypesService,
    pub redis: redis::RedisService,
    pub reminders: reminders::RemindersService,
    pub reservations: reservations::ReservationsService,
    pub schedules: schedules::SchedulesService,
    pub search: Option<Arc<search::MeilisearchService>>,
    pub settings: settings::SettingsService,
    pub sources: sources::SourcesService,
    pub stats: stats::StatsService,
    pub users: users::UsersService,
    pub visitor_counts: visitor_counts::VisitorCountsService,
    pub z3950: z3950::Z3950Service,
    /// Exposed for admin endpoints that need direct DB access (config, settings)
    pool: Pool<Postgres>,
}

impl Services {
    pub fn repository_pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    /// Create all services with the given repository and dynamic config
    pub async fn new(
        repository: Repository,
        auth_config: UsersConfig,
        dynamic_config: Arc<DynamicConfig>,
        redis_config: RedisConfig,
        redis_service: redis::RedisService,
        meilisearch_config: Option<MeilisearchConfig>,
    ) -> AppResult<Self> {
        let pool = repository.pool.clone();

        // Wrap the concrete repository in an Arc so it can be coerced to trait objects.
        let repo = Arc::new(repository.clone());

        // Build optional Meilisearch service
        let search_service: Option<Arc<search::MeilisearchService>> = if let Some(ref cfg) = meilisearch_config {
            let svc = search::MeilisearchService::new(cfg);
            svc.ensure_index().await;
            Some(Arc::new(svc))
        } else {
            tracing::info!("Meilisearch not configured — catalog freesearch will use PostgreSQL fallback");
            None
        };

        let biblios_repo: Arc<dyn BibliosRepository> = repo.clone();
        let entities_repo: Arc<dyn CatalogEntitiesRepository> = repo.clone();
        let catalog = if let Some(ref svc) = search_service {
            catalog::CatalogService::with_search(biblios_repo.clone(), entities_repo, Arc::clone(svc))
        } else {
            catalog::CatalogService::new(biblios_repo, entities_repo)
        };

        let marc_service = marc::MarcService::new(catalog.clone(), redis_service.clone());
        let audit_service = audit::AuditService::new(pool.clone());
        let email_service = email::EmailService::new(dynamic_config.clone());

        let loans_repo: Arc<dyn LoansServiceRepository> = repo.clone();
        let loans_repo_only: Arc<dyn LoansRepository> = repo.clone();
        let reminders_service = reminders::RemindersService::new(
            loans_repo_only,
            email_service.clone(),
            audit_service.clone(),
            dynamic_config.clone(),
        );

        Ok(Self {
            pool,
            audit: audit_service.clone(),
            catalog: catalog.clone(),
            email: email_service.clone(),
            equipment: equipment::EquipmentService::new(repo.clone() as Arc<dyn EquipmentRepository>),
            events: events::EventsService::new(
                repo.clone() as Arc<dyn EventsServiceRepository>,
                email_service.clone(),
                audit_service.clone(),
                dynamic_config.clone(),
            ),
            fines: fines::FinesService::new(repo.clone() as Arc<dyn FinesRepository>),
            inventory: inventory::InventoryService::new(repo.clone() as Arc<dyn InventoryRepository>),
            library_info: library_info::LibraryInfoService::new(repository.clone()),
            loans: loans::LoansService::new(loans_repo),
            marc: marc_service,
            public_types: public_types::PublicTypesService::new(repo.clone() as Arc<dyn PublicTypesRepository>),
            redis: redis_service.clone(),
            reminders: reminders_service,
            reservations: reservations::ReservationsService::new(
                repo.clone() as Arc<dyn ReservationsRepository>,
            ),
            schedules: schedules::SchedulesService::new(repo.clone() as Arc<dyn SchedulesRepository>),
            search: search_service,
            settings: settings::SettingsService::new(repository.clone()),
            sources: sources::SourcesService::new(repo.clone() as Arc<dyn SourcesRepository>),
            stats: stats::StatsService::new(repository.clone()),
            users: users::UsersService::new(repository.clone(), auth_config, redis_service.clone()),
            visitor_counts: visitor_counts::VisitorCountsService::new(
                repo.clone() as Arc<dyn VisitorCountsRepository>,
            ),
            z3950: z3950::Z3950Service::new(
                repository,
                catalog,
                redis_service.clone(),
                redis_config.z3950_cache_ttl_seconds,
            ),
        })
    }
}
