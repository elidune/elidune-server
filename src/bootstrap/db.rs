//! Database pool creation and migrations.

use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};

use crate::config::DatabaseConfig;

/// Create a PostgreSQL pool and run pending migrations.
pub async fn connect_pool(db: &DatabaseConfig) -> Result<Pool<Postgres>, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(db.max_connections)
        .min_connections(db.min_connections)
        .connect(&db.url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
