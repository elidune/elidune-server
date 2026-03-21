//! Business logic services

pub mod audit;
pub mod catalog;
pub mod email;
pub mod email_templates;
pub mod equipment;
pub mod events;
pub mod library_info;
pub mod public_types;
pub mod loans;
pub mod marc;
pub mod redis;
pub mod scheduler;
pub mod reminders;
pub mod schedules;
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
    repository::Repository,
};

/// Container for all services
#[derive(Clone)]
pub struct Services {
    pub audit: audit::AuditService,
    pub catalog: catalog::CatalogService,
    pub email: email::EmailService,
    pub equipment: equipment::EquipmentService,
    pub events: events::EventsService,
    pub library_info: library_info::LibraryInfoService,
    pub loans: loans::LoansService,
    pub marc: marc::MarcService,
    pub public_types: public_types::PublicTypesService,
    pub redis: redis::RedisService,
    pub reminders: reminders::RemindersService,
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

        // Build optional Meilisearch service
        let search_service: Option<Arc<search::MeilisearchService>> = if let Some(ref cfg) = meilisearch_config {
            let svc = search::MeilisearchService::new(cfg);
            svc.ensure_index().await;
            Some(Arc::new(svc))
        } else {
            tracing::info!("Meilisearch not configured — catalog freesearch will use PostgreSQL fallback");
            None
        };

        let catalog = if let Some(ref svc) = search_service {
            catalog::CatalogService::with_search(repository.clone(), Arc::clone(svc))
        } else {
            catalog::CatalogService::new(repository.clone())
        };

        let marc_service = marc::MarcService::new(catalog.clone(), redis_service.clone());
        let audit_service = audit::AuditService::new(pool.clone());
        let email_service = email::EmailService::new(dynamic_config.clone());
        let reminders_service = reminders::RemindersService::new(
            repository.clone(),
            email_service.clone(),
            audit_service.clone(),
            dynamic_config.clone(),
        );

        Ok(Self {
            pool,
            audit: audit_service.clone(),
            catalog: catalog.clone(),
            email: email_service.clone(),
            equipment: equipment::EquipmentService::new(repository.clone()),
            events: events::EventsService::new(
                repository.clone(),
                email_service.clone(),
                audit_service.clone(),
                dynamic_config.clone(),
            ),
            library_info: library_info::LibraryInfoService::new(repository.clone()),
            loans: loans::LoansService::new(repository.clone()),
            marc: marc_service,
            public_types: public_types::PublicTypesService::new(repository.clone()),
            redis: redis_service.clone(),
            reminders: reminders_service,
            schedules: schedules::SchedulesService::new(repository.clone()),
            search: search_service,
            settings: settings::SettingsService::new(repository.clone()),
            sources: sources::SourcesService::new(repository.clone()),
            stats: stats::StatsService::new(repository.clone()),
            users: users::UsersService::new(repository.clone(), auth_config, redis_service.clone()),
            visitor_counts: visitor_counts::VisitorCountsService::new(repository.clone()),
            z3950: z3950::Z3950Service::new(
                repository,
                catalog,
                redis_service.clone(),
                redis_config.z3950_cache_ttl_seconds,
            ),
        })
    }
}
