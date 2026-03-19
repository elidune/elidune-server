//! Public type model (borrower audience: child, adult, school, staff, senior)

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sqlx::FromRow;
use utoipa::ToSchema;

/// Public type from database (borrower audience category)
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct PublicType {
    #[serde_as(as = "serde_with::DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    pub name: String,
    pub label: String,
    pub subscription_duration_days: Option<i32>,
    pub age_min: Option<i16>,
    pub age_max: Option<i16>,
    /// Subscription price in cents (e.g. 1500 = 15.00€)
    pub subscription_price: Option<i32>,
    pub max_loans: Option<i16>,
    pub loan_duration_days: Option<i16>,
}

/// Per-media-type loan settings override for a public type
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct PublicTypeLoanSettings {
    #[serde_as(as = "serde_with::DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde_as(as = "serde_with::DisplayFromStr")]
    #[schema(value_type = String)]
    pub public_type_id: i64,
    pub media_type: String,
    pub duration: Option<i16>,
    pub nb_max: Option<i16>,
    pub nb_renews: Option<i16>,
}

/// Create public type request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreatePublicType {
    pub name: String,
    pub label: String,
    pub subscription_duration_days: Option<i32>,
    pub age_min: Option<i16>,
    pub age_max: Option<i16>,
    pub subscription_price: Option<i32>,
    pub max_loans: Option<i16>,
    pub loan_duration_days: Option<i16>,
}

/// Update public type request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdatePublicType {
    pub name: Option<String>,
    pub label: Option<String>,
    pub subscription_duration_days: Option<i32>,
    pub age_min: Option<i16>,
    pub age_max: Option<i16>,
    pub subscription_price: Option<i32>,
    pub max_loans: Option<i16>,
    pub loan_duration_days: Option<i16>,
}
