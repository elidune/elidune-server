//! Statistics dashboard (delegates to repository).

use chrono::{DateTime, Utc};

use crate::{
    error::AppError,
    error::AppResult,
    models::dto::stats::{
        CatalogStatsResponse, Interval, LoanStatsResponse, StatsResponse, UserLoanStats,
        UserStatsAggregate, UserStatsSortBy,
    },
    models::biblio::MediaType,
    models::stats_builder::{SavedStatsQuery, SavedStatsQueryWrite, StatsBuilderBody, StatsTableResponse},
    repository::Repository,
    services::redis::RedisService,
};

use super::run_stats_query;

pub use crate::repository::stats::StatsFilter;

#[derive(Clone)]
pub struct StatsService {
    repository: Repository,
    redis: RedisService,
}

impl StatsService {
    pub fn new(repository: Repository, redis: RedisService) -> Self {
        Self { repository, redis }
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

    pub async fn run_query(&self, body: &StatsBuilderBody) -> AppResult<StatsTableResponse> {
        run_stats_query(self.repository.pool(), None, body).await
    }

    pub async fn list_saved_queries(
        &self,
        user_id: i64,
        is_admin: bool,
    ) -> AppResult<Vec<SavedStatsQuery>> {
        crate::repository::stats::saved_queries::list_for_user(self.repository.pool(), user_id, is_admin).await
    }

    pub async fn create_saved_query(
        &self,
        user_id: i64,
        body: &SavedStatsQueryWrite,
    ) -> AppResult<SavedStatsQuery> {
        crate::repository::stats::saved_queries::insert(self.repository.pool(), user_id, body).await
    }

    pub async fn update_saved_query(
        &self,
        id: i64,
        user_id: i64,
        is_admin: bool,
        body: &SavedStatsQueryWrite,
    ) -> AppResult<SavedStatsQuery> {
        crate::repository::stats::saved_queries::update(self.repository.pool(), id, user_id, is_admin, body).await
    }

    pub async fn delete_saved_query(
        &self,
        id: i64,
        user_id: i64,
        is_admin: bool,
    ) -> AppResult<()> {
        crate::repository::stats::saved_queries::delete_by_id(self.repository.pool(), id, user_id, is_admin).await
    }

    pub async fn run_saved_query(
        &self,
        id: i64,
        user_id: i64,
        is_admin: bool,
    ) -> AppResult<StatsTableResponse> {
        let saved = crate::repository::stats::saved_queries::get_by_id(
            self.repository.pool(),
            id,
            user_id,
            is_admin,
        )
        .await?
        .ok_or_else(|| AppError::NotFound("Saved query not found".into()))?;
        run_stats_query(self.repository.pool(), Some(&self.redis), &saved.query).await
    }
}
