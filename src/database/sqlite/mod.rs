use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::path::Path;
use tracing::{debug, info};

pub mod models;
pub mod queries;

#[cfg(test)]
mod tests;

pub use models::*;
pub use queries::*;

pub type DbPool = Pool<Sqlite>;

#[derive(Debug, Clone)]
pub struct Database {
    pool: DbPool,
}

impl Database {
    #[inline]
    pub async fn new(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(database_url)
            .create_if_missing(true)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await
            .context("Failed to create database connection pool")?;

        let database = Self { pool };
        database.run_migrations().await?;

        Ok(database)
    }

    #[inline]
    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    #[inline]
    pub async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations");

        sqlx::query(include_str!("migrations/001_initial_schema.sql"))
            .execute(&self.pool)
            .await
            .context("Failed to run initial schema migration")?;

        debug!("Database migrations completed successfully");
        Ok(())
    }

    #[inline]
    pub async fn initialize_from_config_dir(config_dir: &Path) -> Result<Self> {
        let db_path = config_dir.join("metadata.db");
        let db_url = db_path.to_string_lossy();

        std::fs::create_dir_all(config_dir).with_context(|| {
            format!(
                "Failed to create config directory: {}",
                config_dir.display()
            )
        })?;

        Self::new(&db_url).await
    }

    #[inline]
    pub async fn begin_transaction(&self) -> Result<sqlx::Transaction<'_, Sqlite>> {
        self.pool
            .begin()
            .await
            .context("Failed to begin database transaction")
    }
}
