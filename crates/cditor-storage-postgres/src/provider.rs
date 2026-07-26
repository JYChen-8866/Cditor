use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;

use cditor_storage::{
    DocumentStorage, StorageBackendKind, StorageError, StorageProvider, StorageResult,
};

use crate::{PostgresDocumentStorage, PostgresPoolConfig, create_pg_pool, run_migrations};

#[derive(Clone)]
enum PostgresConnection {
    Url(String),
    Pool(PgPool),
}

#[derive(Clone)]
pub struct PostgresStorageProvider {
    connection: PostgresConnection,
}

impl std::fmt::Debug for PostgresStorageProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let connection = match self.connection {
            PostgresConnection::Url(_) => "url",
            PostgresConnection::Pool(_) => "pool",
        };
        formatter
            .debug_struct("PostgresStorageProvider")
            .field("connection", &connection)
            .finish()
    }
}

impl PostgresStorageProvider {
    pub fn from_url(url: impl Into<String>) -> Self {
        Self {
            connection: PostgresConnection::Url(url.into()),
        }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            connection: PostgresConnection::Pool(pool),
        }
    }

    async fn pool(&self) -> StorageResult<PgPool> {
        let pool = match &self.connection {
            PostgresConnection::Url(url) => create_pg_pool(&PostgresPoolConfig::new(url.clone()))
                .await
                .map_err(storage_error)?,
            PostgresConnection::Pool(pool) => pool.clone(),
        };
        run_migrations(&pool).await.map_err(storage_error)?;
        Ok(pool)
    }
}

#[async_trait]
impl StorageProvider for PostgresStorageProvider {
    fn label(&self) -> &str {
        "PostgreSQL"
    }

    fn open_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(90)
    }

    async fn open(&self) -> StorageResult<Arc<dyn DocumentStorage>> {
        Ok(Arc::new(PostgresDocumentStorage::from_pool(
            self.pool().await?,
        )))
    }
}

fn storage_error(error: impl std::fmt::Display) -> StorageError {
    StorageError::Backend {
        backend: StorageBackendKind::Postgres,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_provider_exposes_backend_owned_timeout_policy_without_credentials() {
        let regular = PostgresStorageProvider::from_url("postgres://localhost/cditor");

        assert_eq!(regular.label(), "PostgreSQL");
        assert_eq!(regular.open_timeout(), std::time::Duration::from_secs(90));
        assert!(!format!("{regular:?}").contains("postgres://"));
    }
}
