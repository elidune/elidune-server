//! Visitor counts service

use chrono::NaiveDate;

use std::sync::Arc;

use crate::{
    error::AppResult,
    models::visitor_count::{CreateVisitorCount, VisitorCount},
    repository::VisitorCountsRepository,
};

#[derive(Clone)]
pub struct VisitorCountsService {
    repository: Arc<dyn VisitorCountsRepository>,
}

impl VisitorCountsService {
    pub fn new(repository: Arc<dyn VisitorCountsRepository>) -> Self {
        Self { repository }
    }

    /// List visitor counts for a date range
    #[tracing::instrument(skip(self), err)]
    pub async fn list(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> AppResult<Vec<VisitorCount>> {
        self.repository.visitor_counts_list(start_date, end_date).await
    }

    /// Get total visitor count for a date range
    #[tracing::instrument(skip(self), err)]
    pub async fn total(&self, start_date: NaiveDate, end_date: NaiveDate) -> AppResult<i64> {
        self.repository.visitor_counts_total(start_date, end_date).await
    }

    /// Create a visitor count record
    #[tracing::instrument(skip(self), err)]
    pub async fn create(&self, data: &CreateVisitorCount) -> AppResult<VisitorCount> {
        self.repository.visitor_counts_create(data).await
    }

    /// Delete a visitor count record
    #[tracing::instrument(skip(self), err)]
    pub async fn delete(&self, id: i64) -> AppResult<()> {
        self.repository.visitor_counts_delete(id).await
    }
}
