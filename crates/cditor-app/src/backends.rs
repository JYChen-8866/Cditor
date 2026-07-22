use std::sync::Arc;

use cditor_api::Cditor;
use cditor_storage_postgres::PostgresStorageProvider;
use cditor_storage_sqlite::{SqliteStorageOptions, SqliteStorageProvider};
use sqlx::PgPool;

pub use cditor_storage_postgres::{LargeDemoSeedOptions, PostgresStorageProvider as Postgres};
pub use cditor_storage_sqlite::{
    SqliteDurability, SqliteStorageOptions as SqliteOptions, SqliteStorageProvider as Sqlite,
};

pub trait CditorStorageExt {
    fn with_sqlite_path(self, path: impl Into<std::path::PathBuf>) -> Self;
    fn with_sqlite_options(self, options: SqliteStorageOptions) -> Self;
    fn with_postgres_url(self, url: impl Into<String>) -> Self;
    fn with_postgres_pool(self, pool: PgPool) -> Self;
}

impl CditorStorageExt for Cditor {
    fn with_sqlite_path(self, path: impl Into<std::path::PathBuf>) -> Self {
        self.with_storage_provider(Arc::new(SqliteStorageProvider::file(path)))
    }

    fn with_sqlite_options(self, options: SqliteStorageOptions) -> Self {
        self.with_storage_provider(Arc::new(SqliteStorageProvider::new(options)))
    }

    fn with_postgres_url(self, url: impl Into<String>) -> Self {
        self.with_storage_provider(Arc::new(PostgresStorageProvider::from_url(url)))
    }

    fn with_postgres_pool(self, pool: PgPool) -> Self {
        self.with_storage_provider(Arc::new(PostgresStorageProvider::from_pool(pool)))
    }
}
