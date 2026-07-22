use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;

use cditor_storage::{
    DocumentStorage, StorageBackendKind, StorageError, StorageProvider, StorageResult,
};

use crate::{
    LargeDemoSeedOptions, PostgresDocumentStorage, PostgresPoolConfig, create_pg_pool,
    ensure_large_mixed_demo_seeded, run_migrations,
};

#[derive(Clone)]
enum PostgresConnection {
    Url(String),
    Pool(PgPool),
}

#[derive(Clone)]
pub struct PostgresStorageProvider {
    connection: PostgresConnection,
    seed: Option<LargeDemoSeedOptions>,
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
            .field("has_seed", &self.seed.is_some())
            .finish()
    }
}

impl PostgresStorageProvider {
    pub fn from_url(url: impl Into<String>) -> Self {
        Self {
            connection: PostgresConnection::Url(url.into()),
            seed: None,
        }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            connection: PostgresConnection::Pool(pool),
            seed: None,
        }
    }

    pub fn with_large_demo_seed(mut self, options: LargeDemoSeedOptions) -> Self {
        self.seed = Some(options);
        self
    }

    async fn pool(&self) -> StorageResult<PgPool> {
        let pool = match &self.connection {
            PostgresConnection::Url(url) => create_pg_pool(&PostgresPoolConfig::new(url.clone()))
                .await
                .map_err(storage_error)?,
            PostgresConnection::Pool(pool) => pool.clone(),
        };
        run_migrations(&pool).await.map_err(storage_error)?;
        if let Some(seed) = &self.seed {
            let storage = PostgresDocumentStorage::from_pool(pool.clone());
            ensure_large_mixed_demo_seeded(
                storage.document_store(),
                storage.payload_store(),
                *seed,
            )
            .await
            .map_err(storage_error)?;
        }
        Ok(pool)
    }
}

#[async_trait]
impl StorageProvider for PostgresStorageProvider {
    fn label(&self) -> &str {
        "PostgreSQL"
    }

    fn open_timeout(&self) -> std::time::Duration {
        if self.seed.is_some() {
            std::time::Duration::from_secs(30 * 60)
        } else {
            std::time::Duration::from_secs(90)
        }
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
    use crate::pg_document_id_from_runtime;

    #[test]
    fn postgres_provider_exposes_backend_owned_timeout_policy() {
        let regular = PostgresStorageProvider::from_url("postgres://localhost/cditor");
        let seeded = regular
            .clone()
            .with_large_demo_seed(LargeDemoSeedOptions::new(
                pg_document_id_from_runtime(42),
                7,
                100_000,
            ));

        assert_eq!(regular.label(), "PostgreSQL");
        assert_eq!(regular.open_timeout(), std::time::Duration::from_secs(90));
        assert_eq!(
            seeded.open_timeout(),
            std::time::Duration::from_secs(30 * 60)
        );
        assert!(!format!("{regular:?}").contains("postgres://"));
    }
}
