use sqlx::PgPool;

use cditor_core::document::BlockIndexRecord;
use cditor_core::layout::BlockLayoutMeta;
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
use cditor_runtime::DocumentRuntime;
use cditor_runtime::document_runtime::{
    DocumentRuntimeColdStartData, DocumentRuntimeColdStartReport, DocumentRuntimeIndexSource,
};
use cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION;
use cditor_storage::layout_cache::{CacheSource, LayoutCacheKey};
use cditor_storage_postgres::types::runtime_document_id_from_pg;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresRuntimeLoadOptions {
    pub viewport_height: u32,
    pub visible_index_version: i64,
    pub initial_payload_window_blocks: usize,
    pub layout_key: LayoutCacheKey,
}

impl Default for PostgresRuntimeLoadOptions {
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
        }
    }
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
        options: PostgresRuntimeLoadOptions,
    ) -> PostgresStorageResult<CditorRuntimeLoadResult> {
        let metadata = self
            .document_store
            .load_document_metadata(document_id)
            .await?;
        let block_attrs = self.document_store.load_block_attrs(document_id).await?;
        let runtime_document_id = runtime_document_id_from_pg(metadata.id).ok_or_else(|| {
            PostgresStorageError::CorruptData {
                message: format!("document id {} is outside runtime namespace", metadata.id),
            }
        })?;
        let structure_version = u64::try_from(metadata.structure_version).map_err(|_| {
            PostgresStorageError::CorruptData {
                message: format!(
                    "document {} has negative structure_version {}",
                    metadata.id, metadata.structure_version
                ),
            }
        })?;

        let snapshot_records = self
            .document_store
            .load_document_index_snapshot(
                document_id,
                options.visible_index_version,
                metadata.structure_version,
            )
            .await?;
        let (mut records, index_source) = match snapshot_records {
            Some(records) => (records, DocumentRuntimeIndexSource::Snapshot),
            None => (
                self.document_store
                    .load_block_index_records(document_id)
                    .await?,
                DocumentRuntimeIndexSource::Blocks,
            ),
        };

        let cached_heights = self
            .layout_store
            .load_block_heights(
                &records.iter().map(|record| record.id).collect::<Vec<_>>(),
                options.layout_key,
            )
            .await?;
        let mut layout_cache_hits = 0usize;
        for record in &mut records {
            if let Some(cached) = cached_heights.get(&record.id) {
                layout_cache_hits += 1;
                record.layout_meta = BlockLayoutMeta {
                    block_id: record.id,
                    estimated_height: cached.height,
                    measured_height: (cached.source == CacheSource::ExactMatch)
                        .then_some(cached.height),
                    width_bucket: options.layout_key.width_bucket,
                    layout_version: options.layout_key.content_version,
                    dirty: cached.source != CacheSource::ExactMatch,
                };
            }
        }

        let initial_window_end = records.len().min(options.initial_payload_window_blocks);
        let initial_block_ids = records
            .iter()
            .take(initial_window_end)
            .map(|record| record.id)
            .collect::<Vec<_>>();
        let loaded_payloads = self
            .payload_store
            .load_block_payloads(&initial_block_ids)
            .await?;
        if !loaded_payloads.missing_block_ids.is_empty() {
            let sample = loaded_payloads
                .missing_block_ids
                .iter()
                .take(5)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(PostgresStorageError::CorruptData {
                message: format!(
                    "document {} is missing {} payloads in its initial window (sample block ids: {sample}); reseed or repair the document",
                    metadata.id,
                    loaded_payloads.missing_block_ids.len(),
                ),
            });
        }

        let (runtime, report) = DocumentRuntime::from_cold_start_data(
            DocumentRuntimeColdStartData {
                document_id: runtime_document_id,
                document_title: metadata.title,
                structure_version,
                records,
                block_attrs,
                initial_payloads: loaded_payloads.records,
                initial_payload_window_end: initial_window_end,
                index_source,
                layout_cache_hits,
            },
            f64::from(options.viewport_height),
        )
        .map_err(|message| PostgresStorageError::CorruptData { message })?;
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

fn cold_start_options(options: &CditorOptions) -> PostgresRuntimeLoadOptions {
    PostgresRuntimeLoadOptions {
        initial_payload_window_blocks: options
            .payload_window_size
            .max(MIN_INTERACTIVE_COLD_START_PAYLOAD_BLOCKS),
        ..PostgresRuntimeLoadOptions::default()
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
