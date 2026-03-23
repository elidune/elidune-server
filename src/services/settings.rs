//! Settings service

use sqlx::Row;

use crate::{
    api::settings::{LoanSettings, SettingsResponse, UpdateSettingsRequest, Z3950ServerConfig},
    error::AppResult,
    models::biblio::MediaType,
    repository::Repository,
};

#[derive(Clone)]
pub struct SettingsService {
    repository: Repository,
}

impl SettingsService {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }

    /// Get current settings
    #[tracing::instrument(skip(self), err)]
    pub async fn get_settings(&self) -> AppResult<SettingsResponse> {
        let pool = &self.repository.pool;

        // Get loan settings
        let loan_settings = sqlx::query(
            "SELECT media_type, nb_max, nb_renews, duration FROM loans_settings ORDER BY media_type"
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| LoanSettings {
            media_type: MediaType::from(
                row.get::<Option<String>, _>("media_type")
                    .unwrap_or_default()
                    .as_str(),
            ),
            max_loans: row.get::<Option<i16>, _>("nb_max").unwrap_or(5),
            max_renewals: row.get::<Option<i16>, _>("nb_renews").unwrap_or(2),
            duration_days: row.get::<Option<i16>, _>("duration").unwrap_or(21),
        })
        .collect();

        // Get Z39.50 servers
        let z3950_servers = sqlx::query(
            "SELECT id, name, address, port, database, format, login, password, encoding, activated FROM z3950servers ORDER BY name"
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| Z3950ServerConfig {
            id: row.get("id"),
            name: row.get::<Option<String>, _>("name").unwrap_or_default(),
            address: row.get::<Option<String>, _>("address").unwrap_or_default(),
            port: row.get::<Option<i32>, _>("port").unwrap_or(2200),
            database: row.get("database"),
            format: row.get("format"),
            login: row.get("login"),
            password: row.get("password"),
            encoding: row
                .get::<Option<String>, _>("encoding")
                .unwrap_or_else(|| "utf-8".to_string()),
            is_active: row.get::<Option<i32>, _>("activated").unwrap_or(0) == 1,
        })
        .collect();

        Ok(SettingsResponse {
            loan_settings,
            z3950_servers,
        })
    }

    /// Update settings
    #[tracing::instrument(skip(self), err)]
    pub async fn update_settings(&self, request: UpdateSettingsRequest) -> AppResult<SettingsResponse> {
        let pool = &self.repository.pool;

        // Update loan settings
        if let Some(loan_settings) = request.loan_settings {
            for setting in loan_settings {
                // Try to update existing record first
                let rows_affected = sqlx::query(
                    r#"
                    UPDATE loans_settings
                    SET nb_max = $2, nb_renews = $3, duration = $4
                    WHERE media_type = $1
                    "#,
                )
                .bind(setting.media_type.as_db_str())
                .bind(setting.max_loans)
                .bind(setting.max_renewals)
                .bind(setting.duration_days)
                .execute(pool)
                .await?
                .rows_affected();

                // If no row was updated, insert a new one
                if rows_affected == 0 {
                    sqlx::query(
                        r#"
                        INSERT INTO loans_settings (media_type, nb_max, nb_renews, duration)
                        VALUES ($1, $2, $3, $4)
                        "#,
                    )
                    .bind(setting.media_type.as_db_str())
                    .bind(setting.max_loans)
                    .bind(setting.max_renewals)
                    .bind(setting.duration_days)
                    .execute(pool)
                    .await?;
                }
            }
        }

        // Update Z39.50 servers
        if let Some(z3950_servers) = request.z3950_servers {
            for server in z3950_servers {
                if server.id > 0 {
                    // Update existing
                    sqlx::query(
                        r#"
                        UPDATE z3950servers SET
                            name = $1, address = $2, port = $3, database = $4,
                            format = $5, login = $6, password = $7, encoding = $8, activated = $9
                        WHERE id = $10
                        "#,
                    )
                    .bind(&server.name)
                    .bind(&server.address)
                    .bind(server.port)
                    .bind(&server.database)
                    .bind(&server.format)
                    .bind(&server.login)
                    .bind(&server.password)
                    .bind(&server.encoding)
                    .bind(if server.is_active { 1 } else { 0 })
                    .bind(server.id)
                    .execute(pool)
                    .await?;
                } else {
                    // Insert new
                    sqlx::query(
                        r#"
                        INSERT INTO z3950servers (name, address, port, database, format, login, password, encoding, activated)
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                        "#,
                    )
                    .bind(&server.name)
                    .bind(&server.address)
                    .bind(server.port)
                    .bind(&server.database)
                    .bind(&server.format)
                    .bind(&server.login)
                    .bind(&server.password)
                    .bind(&server.encoding)
                    .bind(if server.is_active { 1 } else { 0 })
                    .execute(pool)
                    .await?;
                }
            }
        }

        self.get_settings().await
    }
}


