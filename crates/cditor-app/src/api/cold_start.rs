use std::path::PathBuf;
use std::sync::Arc;

use sqlx::PgPool;

use cditor_core::layout::{PAGE_POLICY_VERSION, PagePolicy};
use cditor_runtime::DocumentRuntime;
use cditor_runtime::document_runtime::{
    DocumentRuntimeColdStartData, DocumentRuntimeColdStartReport, DocumentRuntimeIndexSource,
};
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{
    DOCUMENT_INDEX_VISIBLE_VERSION, DocumentStorage, LoadDocumentRequest, LoadedDocument,
    StorageResult, StorageSession,
};
use cditor_storage_postgres::types::runtime_document_id_from_pg;
use cditor_storage_postgres::{
    LargeDemoSeedOptions, PgDocumentId, PostgresDocumentStorage, PostgresDocumentStore,
    PostgresLayoutCacheStore, PostgresPayloadStore, PostgresPoolConfig, PostgresStorageResult,
    create_pg_pool, ensure_large_mixed_demo_seeded, pg_document_id_from_runtime, run_migrations,
};
use cditor_storage_sqlite::SqliteDocumentStorage;

use super::options::{CditorBackend, CditorOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CditorColdStartPlan {
    Demo,
    LargeDemo,
    Memory,
    Sqlite {
        document_id: cditor_core::ids::DocumentId,
        path: PathBuf,
    },
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

impl CditorColdStartPlan {
    pub fn from_options(options: &CditorOptions) -> Self {
        match &options.backend {
            CditorBackend::Demo => Self::Demo,
            CditorBackend::LargeDemo => Self::LargeDemo,
            CditorBackend::Memory => Self::Memory,
            CditorBackend::Sqlite { options: sqlite } => match options.document_id {
                Some(document_id) => Self::Sqlite {
                    document_id,
                    path: sqlite.path.clone(),
                },
                None => Self::Invalid {
                    reason: "SQLite backend requires document_id".to_owned(),
                },
            },
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

    pub fn persistent_label(&self) -> Option<String> {
        match self {
            Self::Sqlite { document_id, .. } => Some(format!("SQLite document {document_id}")),
            Self::PostgresUrl { document_id, .. } | Self::PostgresPool { document_id } => {
                Some(format!("PostgreSQL document {document_id}"))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CditorPostgresStores {
    pub pool: PgPool,
    pub document_store: PostgresDocumentStore,
    pub payload_store: PostgresPayloadStore,
    pub layout_store: PostgresLayoutCacheStore,
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
        options: StorageRuntimeLoadOptions,
    ) -> StorageResult<CditorRuntimeLoadResult> {
        let document_id = runtime_document_id_from_pg(document_id).ok_or_else(|| {
            cditor_storage::StorageError::CorruptData(format!(
                "document id {document_id} is outside runtime namespace"
            ))
        })?;
        let storage: Arc<dyn DocumentStorage> =
            Arc::new(PostgresDocumentStorage::from_pool(self.pool.clone()));
        load_runtime_from_storage(storage, document_id, 1, options).await
    }
}

#[derive(Debug)]
pub struct CditorRuntimeLoadResult {
    pub runtime: DocumentRuntime,
    pub report: DocumentRuntimeColdStartReport,
    pub storage_session: StorageSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRuntimeLoadOptions {
    pub viewport_height: u32,
    pub visible_index_version: i64,
    pub initial_payload_window_blocks: usize,
    pub layout_key: LayoutCacheKey,
    pub page_policy_version: u64,
}

pub type PostgresRuntimeLoadOptions = StorageRuntimeLoadOptions;

impl Default for StorageRuntimeLoadOptions {
    fn default() -> Self {
        Self {
            viewport_height: 720,
            visible_index_version: DOCUMENT_INDEX_VISIBLE_VERSION,
            initial_payload_window_blocks: 64,
            layout_key: LayoutCacheKey {
                width_bucket: 10,
                exact_width_px: 800,
                content_version: 1,
                attrs_version: 0,
                style_version: 0,
                font_version: 0,
                theme_version: 0,
                scale_factor_milli: 1000,
            },
            page_policy_version: PAGE_POLICY_VERSION,
        }
    }
}

pub async fn load_runtime_from_options(
    options: &CditorOptions,
) -> StorageResult<Option<CditorRuntimeLoadResult>> {
    let document_id = match options.document_id {
        Some(document_id) => document_id,
        None => return Ok(None),
    };
    let storage: Arc<dyn DocumentStorage> = match &options.backend {
        CditorBackend::Demo
        | CditorBackend::LargeDemo
        | CditorBackend::Memory
        | CditorBackend::Cloud { .. } => return Ok(None),
        CditorBackend::Sqlite { options } => {
            Arc::new(SqliteDocumentStorage::open(options.clone()).await?)
        }
        CditorBackend::PostgresUrl { url } => {
            let storage = PostgresDocumentStorage::from_url(url.clone()).await?;
            seed_postgres_if_requested(storage.pool(), options).await?;
            Arc::new(storage)
        }
        CditorBackend::PostgresPool { pool } => {
            if options.seed_large_demo_to_postgres {
                run_migrations(pool).await.map_err(|error| {
                    cditor_storage::StorageError::Backend {
                        backend: cditor_storage::StorageBackendKind::Postgres,
                        message: error.to_string(),
                    }
                })?;
            }
            seed_postgres_if_requested(pool, options).await?;
            Arc::new(PostgresDocumentStorage::from_pool(pool.clone()))
        }
    };
    load_runtime_from_storage(
        storage,
        document_id,
        options.workspace_id.unwrap_or(1),
        cold_start_options(options),
    )
    .await
    .map(Some)
}

async fn load_runtime_from_storage(
    storage: Arc<dyn DocumentStorage>,
    document_id: cditor_core::ids::DocumentId,
    workspace_id: u64,
    options: StorageRuntimeLoadOptions,
) -> StorageResult<CditorRuntimeLoadResult> {
    let loaded = storage
        .load_document(LoadDocumentRequest {
            document_id,
            workspace_id,
            initial_payload_window_blocks: options.initial_payload_window_blocks,
            visible_index_version: options.visible_index_version,
            layout_key: options.layout_key,
            page_policy_version: options.page_policy_version,
        })
        .await?;
    let viewport_height = options.viewport_height;
    let (runtime, report) = runtime_from_loaded(loaded, viewport_height, &options)?;
    Ok(CditorRuntimeLoadResult {
        storage_session: StorageSession::new(storage, runtime.document_id)
            .with_layout_key(options.layout_key),
        runtime,
        report,
    })
}

fn runtime_from_loaded(
    loaded: LoadedDocument,
    viewport_height: u32,
    options: &StorageRuntimeLoadOptions,
) -> StorageResult<(DocumentRuntime, DocumentRuntimeColdStartReport)> {
    let page_layout_snapshot = loaded.page_layout_snapshot.clone();
    let (mut runtime, mut report) = DocumentRuntime::from_cold_start_data(
        DocumentRuntimeColdStartData {
            document_id: loaded.metadata.document_id,
            document_title: loaded.metadata.title,
            structure_version: loaded.metadata.structure_version,
            records: loaded.records,
            block_attrs: loaded.block_attrs,
            initial_payloads: loaded.initial_payloads,
            initial_payload_window_end: loaded.initial_payload_window_end,
            index_source: if loaded.index_from_snapshot {
                DocumentRuntimeIndexSource::Snapshot
            } else {
                DocumentRuntimeIndexSource::Blocks
            },
            layout_cache_hits: loaded.layout_cache_hits,
        },
        f64::from(viewport_height),
    )
    .map_err(cditor_storage::StorageError::CorruptData)?;

    if let Some(snapshot) = page_layout_snapshot
        && let Ok(page_layout) = snapshot.to_page_layout_index(
            options.visible_index_version,
            runtime.structure_version(),
            options.layout_key,
            options.page_policy_version,
            PagePolicy::default(),
            &runtime.visible_index.visible_block_ids,
        )
        && runtime.apply_cached_page_layout(page_layout).is_ok()
    {
        report.page_layout_cache_hit = true;
    }
    Ok((runtime, report))
}

async fn seed_postgres_if_requested(pool: &PgPool, options: &CditorOptions) -> StorageResult<()> {
    if !options.seed_large_demo_to_postgres {
        return Ok(());
    }
    let document_id = options.document_id.ok_or_else(|| {
        cditor_storage::StorageError::InvalidConfiguration(
            "PostgreSQL seed requires document_id".to_owned(),
        )
    })?;
    let stores = CditorPostgresStores::from_pool(pool.clone());
    ensure_large_mixed_demo_seeded(
        &stores.document_store,
        &stores.payload_store,
        LargeDemoSeedOptions::new(
            pg_document_id_from_runtime(document_id),
            options.workspace_id.unwrap_or(1),
            options.seed_large_demo_block_count,
        )
        .force_reseed(options.force_reseed_large_demo),
    )
    .await
    .map_err(|error| cditor_storage::StorageError::Backend {
        backend: cditor_storage::StorageBackendKind::Postgres,
        message: error.to_string(),
    })?;
    Ok(())
}

const MIN_INTERACTIVE_COLD_START_PAYLOAD_BLOCKS: usize = 256;

fn cold_start_options(options: &CditorOptions) -> StorageRuntimeLoadOptions {
    StorageRuntimeLoadOptions {
        initial_payload_window_blocks: options
            .payload_window_size
            .max(MIN_INTERACTIVE_COLD_START_PAYLOAD_BLOCKS),
        ..StorageRuntimeLoadOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Cditor;
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::layout::{HeightConfidence, PageLayout, PageLayoutIndex};
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
    use cditor_storage::{StorageBackendKind, StorageDocumentMetadata, StoragePageLayoutSnapshot};
    use cditor_storage_postgres::DocumentRow;
    use sqlx::types::Uuid;

    #[test]
    fn cold_start_plan_requires_document_id_for_persistent_backends() {
        let postgres = Cditor::new()
            .with_postgres_url("postgres://localhost/cditor")
            .into_options();
        assert!(matches!(
            CditorColdStartPlan::from_options(&postgres),
            CditorColdStartPlan::Invalid { .. }
        ));

        let sqlite = Cditor::new().with_sqlite_path("test.db").into_options();
        assert_eq!(
            CditorColdStartPlan::from_options(&sqlite),
            CditorColdStartPlan::Invalid {
                reason: "SQLite backend requires document_id".to_owned()
            }
        );
    }

    #[test]
    fn cold_start_plan_maps_sqlite_path() {
        let options = Cditor::new()
            .with_document_id(42)
            .with_sqlite_path("workspace.cditor.db")
            .into_options();
        assert_eq!(
            CditorColdStartPlan::from_options(&options),
            CditorColdStartPlan::Sqlite {
                document_id: 42,
                path: PathBuf::from("workspace.cditor.db")
            }
        );
    }

    #[test]
    fn cold_start_applies_valid_page_cache_and_falls_back_on_boundary_mismatch() {
        let options = StorageRuntimeLoadOptions::default();
        let records = vec![BlockIndexRecord::new(
            101,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            0,
        )];
        let page_layout = PageLayoutIndex::from_cached_pages(
            vec![PageLayout {
                page_index: 0,
                block_start: 0,
                block_count: 1,
                height: 321.0,
                measured_ratio: 1.0,
                confidence: HeightConfidence::Exact,
                max_error_hint: 0.0,
                dirty: false,
            }],
            PagePolicy::default(),
            1,
        )
        .unwrap();
        let snapshot = StoragePageLayoutSnapshot::from_page_layout(
            options.visible_index_version,
            4,
            options.layout_key,
            options.page_policy_version,
            &page_layout,
            &[101],
        )
        .unwrap();
        let loaded = LoadedDocument {
            metadata: StorageDocumentMetadata {
                document_id: 99,
                workspace_id: 1,
                title: "Cached".to_owned(),
                structure_version: 4,
                content_version: 1,
                layout_version: 1,
                schema_version: 1,
            },
            records: records.clone(),
            block_attrs: Vec::new(),
            initial_payloads: vec![BlockPayloadRecord::rich_text(
                101,
                RichBlockKind::Paragraph,
                "cached",
            )],
            initial_payload_window_end: 1,
            index_from_snapshot: true,
            layout_cache_hits: 1,
            page_layout_snapshot: Some(snapshot.clone()),
        };

        let (mut runtime, report) = runtime_from_loaded(loaded.clone(), 720, &options).unwrap();
        assert!(report.page_layout_cache_hit);
        assert_eq!(runtime.page_layout.total_height(), 321.0);
        assert_eq!(
            runtime.scroll.model_total_height,
            runtime.page_layout.total_height() + runtime.down_placer_height()
        );
        runtime.sync_viewport_height(800.0).unwrap();
        assert_eq!(
            runtime.scroll.model_total_height,
            runtime.page_layout.total_height() + runtime.down_placer_height()
        );

        let mut invalid = loaded;
        invalid.page_layout_snapshot.as_mut().unwrap().pages[0].last_block_id = 999;
        let (runtime, report) = runtime_from_loaded(invalid, 720, &options).unwrap();
        assert!(!report.page_layout_cache_hit);
        assert_ne!(runtime.page_layout.total_height(), 321.0);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_pool_options_load_runtime_through_storage_adapter() {
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
            workspace_id: Uuid::from_u128(1),
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
        let records = vec![BlockIndexRecord::new(
            1_900_010,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            0,
        )];
        stores
            .document_store
            .save_block_index_records(document.id, &records, 1)
            .await
            .unwrap();
        stores
            .payload_store
            .save_block_payloads(
                document.id,
                &[BlockPayloadRecord::rich_text(
                    1_900_010,
                    RichBlockKind::Paragraph,
                    "cold start",
                )],
            )
            .await
            .unwrap();

        let options = Cditor::new()
            .with_document_id(runtime_document_id)
            .with_payload_window_size(1)
            .with_postgres_pool(pool)
            .into_options();
        let loaded = load_runtime_from_options(&options).await.unwrap().unwrap();
        assert_eq!(loaded.runtime.document_id, runtime_document_id);
        assert_eq!(loaded.report.payloads_loaded, 1);
        assert_eq!(
            loaded.storage_session.backend_kind(),
            StorageBackendKind::Postgres
        );
    }
}
