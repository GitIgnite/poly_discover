//! Persistence layer for Poly Discover
//!
//! Provides SQLite storage for discovery backtests (knowledge base).

pub mod repository;
pub mod schema;

pub use sqlx::sqlite::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database connection error: {0}")]
    Connection(String),

    #[error("Query error: {0}")]
    Query(String),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub type DbResult<T> = Result<T, DbError>;

/// Database connection pool
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection
    pub async fn new(path: impl AsRef<Path>) -> DbResult<Self> {
        let path = path.as_ref();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let url = format!("sqlite:{}?mode=rwc", path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .map_err(|e| DbError::Connection(e.to_string()))?;

        let db = Self { pool };
        db.run_migrations().await?;
        db.configure_pragmas().await?;

        Ok(db)
    }

    /// Create an in-memory database (for testing)
    pub async fn in_memory() -> DbResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .map_err(|e| DbError::Connection(e.to_string()))?;

        let db = Self { pool };
        db.run_migrations().await?;
        db.configure_pragmas().await?;

        Ok(db)
    }

    /// Run database migrations (execute each statement individually)
    async fn run_migrations(&self) -> DbResult<()> {
        // Create tables
        for statement in schema::CREATE_TABLES.split(';') {
            // Strip comment-only lines, then check if any SQL remains
            let sql: String = statement
                .lines()
                .filter(|line| !line.trim().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n");
            let sql = sql.trim();
            if sql.is_empty() {
                continue;
            }
            sqlx::query(sql)
                .execute(&self.pool)
                .await
                .map_err(|e| DbError::Migration(format!("{e}: {sql}")))?;
        }

        // Run ALTER TABLE migrations (tolerate "duplicate column name" errors)
        for migration in schema::MIGRATIONS {
            match sqlx::query(migration).execute(&self.pool).await {
                Ok(_) => {}
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("duplicate column name") {
                        // Column already exists — this is expected on subsequent runs
                    } else {
                        return Err(DbError::Migration(format!("{e}: {migration}")));
                    }
                }
            }
        }

        Ok(())
    }

    /// Configure SQLite pragmas for optimal performance
    async fn configure_pragmas(&self) -> DbResult<()> {
        // WAL mode: allows concurrent reads during writes
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Connection(format!("WAL pragma failed: {e}")))?;

        // NORMAL sync: good balance of safety and performance
        sqlx::query("PRAGMA synchronous=NORMAL")
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Connection(format!("synchronous pragma failed: {e}")))?;

        // Enable foreign key constraints
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Connection(format!("foreign_keys pragma failed: {e}")))?;

        // 8 MB cache size (negative = KiB)
        sqlx::query("PRAGMA cache_size=-8000")
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Connection(format!("cache_size pragma failed: {e}")))?;

        Ok(())
    }

    /// Get the connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Clone the pool for use in spawned tasks
    pub fn pool_clone(&self) -> SqlitePool {
        self.pool.clone()
    }
}
