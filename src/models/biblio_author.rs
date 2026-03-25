//! BiblioAuthor junction model (N:M relationship between biblios and authors)

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Author type in MARC context
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[repr(i16)]
pub enum AuthorType {
    Personal = 0,
    Corporate = 1,
    Meeting = 2,
}

impl From<i16> for AuthorType {
    fn from(v: i16) -> Self {
        match v {
            1 => AuthorType::Corporate,
            2 => AuthorType::Meeting,
            _ => AuthorType::Personal,
        }
    }
}

/// Junction row linking a biblio to an author with role and position
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BiblioAuthor {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub biblio_id: i64,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub author_id: i64,
    pub role: Option<String>,
    pub author_type: i16,
    pub position: i16,
}
