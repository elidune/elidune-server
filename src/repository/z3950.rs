//! Z39.50 server persistence (`z3950servers` table).

use async_trait::async_trait;

use super::Repository;
use crate::error::AppResult;
use crate::models::secret::plaintext_password;
use crate::models::PlaintextPassword;

/// Row as stored for API and search (selected columns).
#[derive(Clone, sqlx::FromRow)]
struct Z3950ServerRecordRow {
    id: i64,
    name: Option<String>,
    address: Option<String>,
    port: Option<i32>,
    database: Option<String>,
    format: Option<String>,
    login: Option<String>,
    password: Option<String>,
    encoding: Option<String>,
    activated: Option<bool>,
}

/// Row as stored for API and search (selected columns).
#[derive(Debug, Clone)]
pub struct Z3950ServerRecord {
    pub id: i64,
    pub name: Option<String>,
    pub address: Option<String>,
    pub port: Option<i32>,
    pub database: Option<String>,
    pub format: Option<String>,
    pub login: Option<String>,
    pub password: Option<PlaintextPassword>,
    pub encoding: Option<String>,
    pub activated: Option<bool>,
}

impl From<Z3950ServerRecordRow> for Z3950ServerRecord {
    fn from(row: Z3950ServerRecordRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            address: row.address,
            port: row.port,
            database: row.database,
            format: row.format,
            login: row.login,
            password: row.password.map(plaintext_password),
            encoding: row.encoding,
            activated: row.activated,
        }
    }
}

/// DB access for `z3950servers`. Implemented by [`Repository`].
#[async_trait]
pub trait Z3950Repository: Send + Sync {
    async fn z3950_servers_list_all(&self) -> AppResult<Vec<Z3950ServerRecord>>;
    async fn z3950_servers_list_active_for_search(
        &self,
        server_id: Option<i64>,
    ) -> AppResult<Vec<Z3950ServerRecord>>;
    async fn z3950_server_update(
        &self,
        id: i64,
        name: &str,
        address: &str,
        port: i32,
        database: &Option<String>,
        format: &Option<String>,
        login: &Option<String>,
        password: &Option<String>,
        encoding: &str,
        activated: bool,
    ) -> AppResult<()>;
    async fn z3950_server_insert(
        &self,
        name: &str,
        address: &str,
        port: i32,
        database: &Option<String>,
        format: &Option<String>,
        login: &Option<String>,
        password: &Option<String>,
        encoding: &str,
        activated: bool,
    ) -> AppResult<()>;
}

#[async_trait]
impl Z3950Repository for Repository {
    async fn z3950_servers_list_all(&self) -> AppResult<Vec<Z3950ServerRecord>> {
        Repository::z3950_servers_list_all(self).await
    }

    async fn z3950_servers_list_active_for_search(
        &self,
        server_id: Option<i64>,
    ) -> AppResult<Vec<Z3950ServerRecord>> {
        Repository::z3950_servers_list_active_for_search(self, server_id).await
    }

    async fn z3950_server_update(
        &self,
        id: i64,
        name: &str,
        address: &str,
        port: i32,
        database: &Option<String>,
        format: &Option<String>,
        login: &Option<String>,
        password: &Option<String>,
        encoding: &str,
        activated: bool,
    ) -> AppResult<()> {
        Repository::z3950_server_update(
            self, id, name, address, port, database, format, login, password, encoding, activated,
        )
        .await
    }

    async fn z3950_server_insert(
        &self,
        name: &str,
        address: &str,
        port: i32,
        database: &Option<String>,
        format: &Option<String>,
        login: &Option<String>,
        password: &Option<String>,
        encoding: &str,
        activated: bool,
    ) -> AppResult<()> {
        Repository::z3950_server_insert(
            self, name, address, port, database, format, login, password, encoding, activated,
        )
        .await
    }
}

impl Repository {
    /// All servers for staff settings UI (ordered by name).
    pub async fn z3950_servers_list_all(&self) -> AppResult<Vec<Z3950ServerRecord>> {
        let rows = sqlx::query_as::<_, Z3950ServerRecordRow>(
            r#"SELECT id, name, address, port, database, format, login, password, encoding, activated
               FROM z3950servers ORDER BY name"#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Active servers for catalog search (optional filter by server id).
    pub async fn z3950_servers_list_active_for_search(
        &self,
        server_id: Option<i64>,
    ) -> AppResult<Vec<Z3950ServerRecord>> {
        let rows = if let Some(id) = server_id {
            sqlx::query_as::<_, Z3950ServerRecordRow>(
                r#"SELECT id, name, address, port, database, format, login, password, encoding, activated
                   FROM z3950servers WHERE id = $1 AND activated = TRUE"#,
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Z3950ServerRecordRow>(
                r#"SELECT id, name, address, port, database, format, login, password, encoding, activated
                   FROM z3950servers WHERE activated = TRUE"#,
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn z3950_server_update(
        &self,
        id: i64,
        name: &str,
        address: &str,
        port: i32,
        database: &Option<String>,
        format: &Option<String>,
        login: &Option<String>,
        password: &Option<String>,
        encoding: &str,
        activated: bool,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE z3950servers SET
                name = $1, address = $2, port = $3, database = $4,
                format = $5, login = $6, password = $7, encoding = $8, activated = $9
            WHERE id = $10
            "#,
        )
        .bind(name)
        .bind(address)
        .bind(port)
        .bind(database)
        .bind(format)
        .bind(login)
        .bind(password)
        .bind(encoding)
        .bind(activated)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn z3950_server_insert(
        &self,
        name: &str,
        address: &str,
        port: i32,
        database: &Option<String>,
        format: &Option<String>,
        login: &Option<String>,
        password: &Option<String>,
        encoding: &str,
        activated: bool,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO z3950servers (name, address, port, database, format, login, password, encoding, activated)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(name)
        .bind(address)
        .bind(port)
        .bind(database)
        .bind(format)
        .bind(login)
        .bind(password)
        .bind(encoding)
        .bind(activated)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
