//! Business logic services

pub mod catalog;
pub mod email;
pub mod equipment;
pub mod events;
pub mod loans;
pub mod marc;
pub mod redis;
pub mod schedules;
pub mod settings;
pub mod sources;
pub mod stats;
pub mod users;
pub mod visitor_counts;
pub mod z3950;

use crate::{config::{UsersConfig, EmailConfig, RedisConfig}, error::AppResult, repository::Repository};

/// Container for all services
#[derive(Clone)]
pub struct Services {
    pub users: users::UsersService,
    pub catalog: catalog::CatalogService,
    pub loans: loans::LoansService,
    pub z3950: z3950::Z3950Service,
    pub stats: stats::StatsService,
    pub settings: settings::SettingsService,
    pub email: email::EmailService,
    pub redis: redis::RedisService,
    pub visitor_counts: visitor_counts::VisitorCountsService,
    pub schedules: schedules::SchedulesService,
    pub sources: sources::SourcesService,
    pub equipment: equipment::EquipmentService,
    pub events: events::EventsService,
    pub marc: marc::MarcService,
}

impl Services {
    /// Create all services with the given repository
    pub async fn new(
        repository: Repository,
        auth_config: UsersConfig,
        email_config: EmailConfig,
        redis_config: RedisConfig,
        redis_service: redis::RedisService,
    ) -> AppResult<Self> {
        let catalog = catalog::CatalogService::new(repository.clone());
        let marc_service = marc::MarcService::new(catalog.clone(), redis_service.clone());
        Ok(Self {
            catalog: catalog.clone(),
            users: users::UsersService::new(repository.clone(), auth_config.clone(), redis_service.clone()),
            loans: loans::LoansService::new(repository.clone()),
            z3950: z3950::Z3950Service::new(repository.clone(), catalog, redis_service.clone(), redis_config.z3950_cache_ttl_seconds),
            stats: stats::StatsService::new(repository.clone()),
            settings: settings::SettingsService::new(repository.clone()),
            email: email::EmailService::new(email_config.clone()),
            visitor_counts: visitor_counts::VisitorCountsService::new(repository.clone()),
            schedules: schedules::SchedulesService::new(repository.clone()),
            sources: sources::SourcesService::new(repository.clone()),
            equipment: equipment::EquipmentService::new(repository.clone()),
            events: events::EventsService::new(repository),
            redis: redis_service,
            marc: marc_service,
        })
    }
}


