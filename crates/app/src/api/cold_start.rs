use sqlx::PgPool;

use cditor_core::document::BlockIndexRecord;
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
use cditor_runtime::DocumentRuntime;
use cditor_runtime::document_runtime::{
    DocumentRuntimeColdStartReport, DocumentRuntimeFromStoreOptions,
};
use cditor_storage_postgres::{
    DocumentRow, LargeDemoSeedOptions, PgDocumentId, PostgresDocumentStore,
    PostgresLayoutCacheStore, PostgresPayloadStore, PostgresPoolConfig, PostgresStorageError,
    PostgresStorageResult, create_pg_pool, ensure_large_mixed_demo_seeded,
    pg_document_id_from_runtime, run_migrations,
};
use sqlx::types::Uuid;

use super::options::{CditorBackend, CditorOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CditorColdStartPlan {
    Demo,
    LargeDemo,
    Memory,
    PostgresUrl {
        document_id: PgDocumentId,
        url: String,
    },
    PostgresPool {
        document_id: PgDocumentId,
    },
    Cloud {
        endpoint: String,
    },
    Invalid {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct CditorPostgresStores {
    pub pool: PgPool,
    pub document_store: PostgresDocumentStore,
    pub payload_store: PostgresPayloadStore,
    pub layout_store: PostgresLayoutCacheStore,
}

#[derive(Debug)]
pub struct CditorRuntimeLoadResult {
    pub runtime: DocumentRuntime,
    pub report: DocumentRuntimeColdStartReport,
    pub postgres_pool: Option<PgPool>,
}

impl CditorColdStartPlan {
    pub fn from_options(options: &CditorOptions) -> Self {
        match &options.backend {
            CditorBackend::Demo => Self::Demo,
            CditorBackend::LargeDemo => Self::LargeDemo,
            CditorBackend::Memory => Self::Memory,
            CditorBackend::PostgresUrl { url } => match options.document_id {
                Some(document_id) => Self::PostgresUrl {
                    document_id: pg_document_id_from_runtime(document_id),
                    url: url.clone(),
                },
                None => Self::Invalid {
                    reason: "PostgreSQL backend requires document_id".to_owned(),
                },
            },
            CditorBackend::PostgresPool { .. } => match options.document_id {
                Some(document_id) => Self::PostgresPool {
                    document_id: pg_document_id_from_runtime(document_id),
                },
                None => Self::Invalid {
                    reason: "PostgreSQL pool backend requires document_id".to_owned(),
                },
            },
            CditorBackend::Cloud { endpoint } => Self::Cloud {
                endpoint: endpoint.clone(),
            },
        }
    }
}

impl CditorPostgresStores {
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            document_store: PostgresDocumentStore::new(pool.clone()),
            payload_store: PostgresPayloadStore::new(pool.clone()),
            layout_store: PostgresLayoutCacheStore::new(pool.clone()),
            pool,
        }
    }

    pub async fn from_url(url: impl Into<String>) -> PostgresStorageResult<Self> {
        let pool = create_pg_pool(&PostgresPoolConfig::new(url)).await?;
        run_migrations(&pool).await?;
        Ok(Self::from_pool(pool))
    }

    pub async fn load_runtime(
        &self,
        document_id: PgDocumentId,
        options: DocumentRuntimeFromStoreOptions,
    ) -> PostgresStorageResult<CditorRuntimeLoadResult> {
        let (runtime, report) = DocumentRuntime::from_store(
            document_id,
            &self.document_store,
            &self.payload_store,
            &self.layout_store,
            options,
        )
        .await?;
        Ok(CditorRuntimeLoadResult {
            runtime,
            report,
            postgres_pool: Some(self.pool.clone()),
        })
    }
}

pub async fn load_runtime_from_options(
    options: &CditorOptions,
) -> PostgresStorageResult<Option<CditorRuntimeLoadResult>> {
    match &options.backend {
        CditorBackend::Demo
        | CditorBackend::LargeDemo
        | CditorBackend::Memory
        | CditorBackend::Cloud { .. } => Ok(None),
        CditorBackend::PostgresUrl { url } => {
            let Some(document_id) = options.document_id else {
                return Ok(None);
            };
            let stores = CditorPostgresStores::from_url(url.clone()).await?;
            seed_postgres_if_requested(&stores, options).await?;
            stores
                .load_runtime(
                    pg_document_id_from_runtime(document_id),
                    cold_start_options(options),
                )
                .await
                .map(Some)
        }
        CditorBackend::PostgresPool { pool } => {
            let Some(document_id) = options.document_id else {
                return Ok(None);
            };
            let stores = CditorPostgresStores::from_pool(pool.clone());
            seed_postgres_if_requested(&stores, options).await?;
            stores
                .load_runtime(
                    pg_document_id_from_runtime(document_id),
                    cold_start_options(options),
                )
                .await
                .map(Some)
        }
    }
}

async fn seed_postgres_if_requested(
    stores: &CditorPostgresStores,
    options: &CditorOptions,
) -> PostgresStorageResult<()> {
    let Some(document_id) = options.document_id else {
        return Ok(());
    };
    if !options.seed_large_demo_to_postgres {
        return ensure_minimal_document_exists(stores, options).await;
    };
    let workspace_id = options.workspace_id.unwrap_or(1);
    let report = ensure_large_mixed_demo_seeded(
        &stores.document_store,
        &stores.payload_store,
        LargeDemoSeedOptions::new(
            pg_document_id_from_runtime(document_id),
            workspace_id,
            options.seed_large_demo_block_count,
        )
        .force_reseed(options.force_reseed_large_demo),
    )
    .await?;
    if report.skipped_existing {
        eprintln!("[cditor][postgres_seed] document already exists; skip seed");
    } else {
        eprintln!(
            "[cditor][postgres_seed] seeded document={} blocks={} payloads={}",
            report.document_id, report.block_count, report.payload_count
        );
    }
    Ok(())
}

async fn ensure_minimal_document_exists(
    stores: &CditorPostgresStores,
    options: &CditorOptions,
) -> PostgresStorageResult<()> {
    let Some(runtime_document_id) = options.document_id else {
        return Ok(());
    };
    let document_id = pg_document_id_from_runtime(runtime_document_id);
    match stores
        .document_store
        .load_document_metadata(document_id)
        .await
    {
        Ok(_) => return Ok(()),
        Err(PostgresStorageError::NotFound { .. }) => {}
        Err(error) => return Err(error),
    }

    let workspace_id = Uuid::from_u128(options.workspace_id.unwrap_or(1) as u128);
    stores
        .document_store
        .save_document_metadata(&DocumentRow {
            id: document_id,
            workspace_id,
            title: "Untitled".to_owned(),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        })
        .await?;
    let records = vec![BlockIndexRecord::new(
        1,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
        0,
    )];
    stores
        .document_store
        .save_block_index_records(document_id, &records, 1)
        .await?;
    stores
        .payload_store
        .save_block_payloads(
            document_id,
            &[BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "",
            )],
        )
        .await?;
    Ok(())
}

const MIN_INTERACTIVE_COLD_START_PAYLOAD_BLOCKS: usize = 256;

fn cold_start_options(options: &CditorOptions) -> DocumentRuntimeFromStoreOptions {
    DocumentRuntimeFromStoreOptions {
        initial_payload_window_blocks: options
            .payload_window_size
            .max(MIN_INTERACTIVE_COLD_START_PAYLOAD_BLOCKS),
        ..DocumentRuntimeFromStoreOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Cditor;
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
    use cditor_storage_postgres::{DocumentRow, PostgresPoolConfig};
    use sqlx::types::Uuid;

    #[test]
    fn cold_start_plan_requires_document_id_for_postgres_url() {
        let options = Cditor::new()
            .with_postgres_url("postgres://localhost/cditor")
            .into_options();

        assert_eq!(
            CditorColdStartPlan::from_options(&options),
            CditorColdStartPlan::Invalid {
                reason: "PostgreSQL backend requires document_id".to_owned()
            }
        );
    }

    #[test]
    fn cold_start_plan_maps_postgres_url_document_to_pg_id() {
        let options = Cditor::new()
            .with_document_id(42)
            .with_postgres_url("postgres://localhost/cditor")
            .into_options();

        assert_eq!(
            CditorColdStartPlan::from_options(&options),
            CditorColdStartPlan::PostgresUrl {
                document_id: pg_document_id_from_runtime(42),
                url: "postgres://localhost/cditor".to_owned(),
            }
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn cditor_postgres_pool_options_load_runtime_from_store() {
        let database_url = std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned());
        let pool = create_pg_pool(&PostgresPoolConfig::for_tests(database_url))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let stores = CditorPostgresStores::from_pool(pool.clone());
        let runtime_document_id = 190_001;
        let document = DocumentRow {
            id: pg_document_id_from_runtime(runtime_document_id),
            workspace_id: Uuid::from_u128(0x9b00_0000_0000_0000_0000_0000_0000_0001),
            title: "Cditor Cold Start".to_owned(),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        };
        stores
            .document_store
            .save_document_metadata(&document)
            .await
            .unwrap();
        let paragraph = kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph);
        let records = vec![
            BlockIndexRecord::new(1_900_010, None, 0, paragraph, 0),
            BlockIndexRecord::new(1_900_011, None, 0, paragraph, 0),
        ];
        stores
            .document_store
            .save_block_index_records(document.id, &records, 1)
            .await
            .unwrap();
        stores
            .payload_store
            .save_block_payloads(
                document.id,
                &[
                    BlockPayloadRecord::rich_text(
                        1_900_010,
                        RichBlockKind::Paragraph,
                        "cold start first block",
                    ),
                    BlockPayloadRecord::rich_text(
                        1_900_011,
                        RichBlockKind::Paragraph,
                        "cold start second block",
                    ),
                ],
            )
            .await
            .unwrap();

        let options = Cditor::new()
            .with_document_id(runtime_document_id)
            .with_payload_window_size(2)
            .with_postgres_pool(pool)
            .into_options();
        let loaded = load_runtime_from_options(&options).await.unwrap().unwrap();

        assert_eq!(loaded.runtime.document_id, runtime_document_id);
        assert_eq!(loaded.report.payloads_loaded, 2);
        assert_eq!(loaded.runtime.projection_for_window().blocks.len(), 2);
    }
}
