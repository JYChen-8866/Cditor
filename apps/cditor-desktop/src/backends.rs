#[cfg(any(feature = "sqlite", feature = "postgres"))]
use std::sync::Arc;

use cditor_sdk::Cditor;
#[cfg(feature = "postgres")]
use cditor_storage_postgres::PostgresStorageProvider;
#[cfg(feature = "sqlite")]
use cditor_storage_sqlite::{SqliteStorageOptions, SqliteStorageProvider};
#[cfg(feature = "postgres")]
use sqlx::PgPool;

#[cfg(feature = "postgres")]
pub use cditor_storage_postgres::PostgresStorageProvider as Postgres;
#[cfg(feature = "sqlite")]
pub use cditor_storage_sqlite::{
    SqliteDurability, SqliteStorageOptions as SqliteOptions, SqliteStorageProvider as Sqlite,
};

pub trait CditorStorageExt {
    #[cfg(feature = "sqlite")]
    fn with_sqlite_path(self, path: impl Into<std::path::PathBuf>) -> Self;
    #[cfg(feature = "sqlite")]
    fn with_sqlite_options(self, options: SqliteStorageOptions) -> Self;
    #[cfg(feature = "postgres")]
    fn with_postgres_url(self, url: impl Into<String>) -> Self;
    #[cfg(feature = "postgres")]
    fn with_postgres_pool(self, pool: PgPool) -> Self;
}

impl CditorStorageExt for Cditor {
    #[cfg(feature = "sqlite")]
    fn with_sqlite_path(self, path: impl Into<std::path::PathBuf>) -> Self {
        self.with_storage_provider(Arc::new(SqliteStorageProvider::file(path)))
    }

    #[cfg(feature = "sqlite")]
    fn with_sqlite_options(self, options: SqliteStorageOptions) -> Self {
        self.with_storage_provider(Arc::new(SqliteStorageProvider::new(options)))
    }

    #[cfg(feature = "postgres")]
    fn with_postgres_url(self, url: impl Into<String>) -> Self {
        self.with_storage_provider(Arc::new(PostgresStorageProvider::from_url(url)))
    }

    #[cfg(feature = "postgres")]
    fn with_postgres_pool(self, pool: PgPool) -> Self {
        self.with_storage_provider(Arc::new(PostgresStorageProvider::from_pool(pool)))
    }
}
