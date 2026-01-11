use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Pool, Sqlite,
};
use std::str::FromStr;

/// Database connection pool wrapper
#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    /// Connect to the SQLite database
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        // Ensure data directory exists
        if let Some(path) = database_url.strip_prefix("sqlite:") {
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Parse options and enable create_if_missing
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    /// Run database migrations
    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }
}

// Re-export sqlx types for convenience
#[allow(unused_imports)]
pub use sqlx::{FromRow, Row};
