//! Library information service

use sqlx::Row;

use crate::{
    api::library_info::{LibraryInfo, UpdateLibraryInfoRequest},
    error::AppResult,
    repository::Repository,
};

#[derive(Clone)]
pub struct LibraryInfoService {
    repository: Repository,
}

impl LibraryInfoService {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }

    /// Get library information (always returns a record, empty if not yet set)
    #[tracing::instrument(skip(self), err)]
    pub async fn get(&self) -> AppResult<LibraryInfo> {
        let pool = &self.repository.pool;

        let result = sqlx::query(
            r#"
            SELECT name, addr_line1, addr_line2, addr_postcode, addr_city, addr_country,
                   phones, email, updated_at
            FROM library_info WHERE id = 1
            "#,
        )
        .fetch_optional(pool)
        .await?;

        match result {
            Some(row) => {
                let phones_val: Option<serde_json::Value> = row.get("phones");
                let phones: Vec<String> = phones_val
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();

                Ok(LibraryInfo {
                    name: row.get("name"),
                    addr_line1: row.get("addr_line1"),
                    addr_line2: row.get("addr_line2"),
                    addr_postcode: row.get("addr_postcode"),
                    addr_city: row.get("addr_city"),
                    addr_country: row.get("addr_country"),
                    phones,
                    email: row.get("email"),
                    updated_at: row.get("updated_at"),
                })
            }
            None => Ok(LibraryInfo {
                name: None,
                addr_line1: None,
                addr_line2: None,
                addr_postcode: None,
                addr_city: None,
                addr_country: None,
                phones: vec![],
                email: None,
                updated_at: None,
            }),
        }
    }

    /// Update library information (partial update: only provided fields are changed)
    #[tracing::instrument(skip(self), err)]
    pub async fn update(&self, req: UpdateLibraryInfoRequest) -> AppResult<LibraryInfo> {
        let pool = &self.repository.pool;

        let phones_json = req
            .phones
            .map(|p| serde_json::to_value(p).unwrap_or(serde_json::json!([])));

        sqlx::query(
            r#"
            INSERT INTO library_info (id, name, addr_line1, addr_line2, addr_postcode,
                                      addr_city, addr_country, phones, email, updated_at)
            VALUES (1, $1, $2, $3, $4, $5, $6, COALESCE($7::jsonb, '[]'::jsonb), $8, NOW())
            ON CONFLICT (id) DO UPDATE SET
                name          = COALESCE($1, library_info.name),
                addr_line1    = COALESCE($2, library_info.addr_line1),
                addr_line2    = COALESCE($3, library_info.addr_line2),
                addr_postcode = COALESCE($4, library_info.addr_postcode),
                addr_city     = COALESCE($5, library_info.addr_city),
                addr_country  = COALESCE($6, library_info.addr_country),
                phones        = COALESCE($7::jsonb, library_info.phones),
                email         = COALESCE($8, library_info.email),
                updated_at    = NOW()
            "#,
        )
        .bind(&req.name)
        .bind(&req.addr_line1)
        .bind(&req.addr_line2)
        .bind(&req.addr_postcode)
        .bind(&req.addr_city)
        .bind(&req.addr_country)
        .bind(phones_json)
        .bind(&req.email)
        .execute(pool)
        .await?;

        self.get().await
    }
}
