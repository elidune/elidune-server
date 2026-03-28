//! Statistics dashboard (delegates to repository).

use chrono::{DateTime, Utc};

use crate::{
    api::stats::{
        CatalogStatsResponse, Interval, LoanStatsResponse, StatsResponse, UserLoanStats,
        UserStatsAggregate, UserStatsSortBy,
    },
    error::AppResult,
    models::biblio::MediaType,
    repository::Repository,
};

pub use crate::repository::stats::StatsFilter;

#[derive(Clone)]
pub struct StatsService {
    repository: Repository,
}

impl StatsService {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }

    pub async fn get_stats(&self, filter: Option<StatsFilter>) -> AppResult<StatsResponse> {
        self.repository.stats_get_stats(filter).await
    }

    pub async fn get_user_stats(
        &self,
        sort_by: UserStatsSortBy,
        limit: i64,
    ) -> AppResult<Vec<UserLoanStats>> {
        self.repository.stats_get_user_stats(sort_by, limit).await
    }

    pub async fn get_loan_stats(
        &self,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        interval: Interval,
        media_type: Option<&MediaType>,
        public_type: Option<&str>,
        user_id: Option<i64>,
    ) -> AppResult<LoanStatsResponse> {
        self.repository
            .stats_get_loan_stats(
                start_date,
                end_date,
                interval,
                media_type,
                public_type,
                user_id,
            )
            .await
    }

    pub async fn get_user_aggregates(
        &self,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> AppResult<UserStatsAggregate> {
        self.repository
            .stats_get_user_aggregates(start_date, end_date)
            .await
    }

    pub async fn get_catalog_stats(
        &self,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        by_source: bool,
        by_media_type: bool,
        by_public_type: bool,
    ) -> AppResult<CatalogStatsResponse> {
        self.repository
            .stats_get_catalog_stats(
                start_date,
                end_date,
                by_source,
                by_media_type,
                by_public_type,
            )
            .await
    }
}
