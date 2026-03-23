//! Public types service

use std::sync::Arc;

use crate::{
    error::AppResult,
    models::public_type::{
        CreatePublicType, PublicType, PublicTypeLoanSettings, UpdatePublicType,
    },
    repository::PublicTypesRepository,
};

#[derive(Clone)]
pub struct PublicTypesService {
    repository: Arc<dyn PublicTypesRepository>,
}

impl PublicTypesService {
    pub fn new(repository: Arc<dyn PublicTypesRepository>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn list(&self) -> AppResult<Vec<PublicType>> {
        self.repository.public_types_list().await
    }

    pub async fn get_by_id(&self, id: i64) -> AppResult<PublicType> {
        self.repository.public_types_get_by_id(id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn get_loan_settings(&self, public_type_id: i64) -> AppResult<Vec<PublicTypeLoanSettings>> {
        self.repository.public_types_get_loan_settings(public_type_id).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn create(&self, data: &CreatePublicType) -> AppResult<PublicType> {
        self.repository.public_types_create(data).await
    }

    pub async fn update(&self, id: i64, data: &UpdatePublicType) -> AppResult<PublicType> {
        self.repository.public_types_update(id, data).await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn delete(&self, id: i64) -> AppResult<()> {
        self.repository.public_types_delete(id).await
    }

    pub async fn upsert_loan_setting(
        &self,
        public_type_id: i64,
        media_type: &str,
        duration: Option<i16>,
        nb_max: Option<i16>,
        nb_renews: Option<i16>,
    ) -> AppResult<PublicTypeLoanSettings> {
        self.repository
            .public_types_upsert_loan_setting(public_type_id, media_type, duration, nb_max, nb_renews)
            .await
    }

    #[tracing::instrument(skip(self), err)]
    pub async fn delete_loan_setting(&self, public_type_id: i64, media_type: &str) -> AppResult<()> {
        self.repository
            .public_types_delete_loan_setting(public_type_id, media_type)
            .await
    }
}
