//! Events service

use crate::{
    error::AppResult,
    models::event::{CreateEvent, Event, EventQuery, UpdateEvent},
    repository::{events, events::EventAnnualStats, Repository},
};

#[derive(Clone)]
pub struct EventsService {
    repository: Repository,
}

impl EventsService {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }

    pub async fn list(&self, query: &EventQuery) -> AppResult<(Vec<Event>, i64)> {
        self.repository.events_list(query).await
    }

    pub async fn get_by_id(&self, id: i64) -> AppResult<Event> {
        self.repository.events_get_by_id(id).await
    }

    pub async fn create(&self, data: &CreateEvent) -> AppResult<Event> {
        self.repository.events_create(data).await
    }

    pub async fn update(&self, id: i64, data: &UpdateEvent) -> AppResult<Event> {
        self.repository.events_update(id, data).await
    }

    pub async fn delete(&self, id: i64) -> AppResult<()> {
        self.repository.events_delete(id).await
    }

    /// Get annual event statistics (for annual report)
    pub async fn annual_stats(&self, year: i32) -> AppResult<EventAnnualStats> {
        self.repository.events_annual_stats(year).await
    }
}
