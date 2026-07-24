use std::sync::Arc;

use cditor_api::options::{CditorBackend, CditorOptions};
use cditor_core::layout::{PAGE_POLICY_VERSION, PagePolicy};
use cditor_core::schema::CURRENT_DOCUMENT_FORMAT;
use cditor_runtime::DocumentRuntime;
use cditor_runtime::document_runtime::{
    DocumentRuntimeColdStartData, DocumentRuntimeColdStartReport, DocumentRuntimeIndexSource,
};
use cditor_session::{
    EmergencyRecoveryDecision, EmergencyRecoveryPlan, plan_emergency_recovery,
    project_emergency_recovery,
};

use crate::storage_recovery::hydrate_emergency_payloads;
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{
    DOCUMENT_INDEX_VISIBLE_VERSION, DocumentStorage, LoadDocumentRequest, LoadedDocument,
    StorageBackendKind, StorageResult, StorageSession,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CditorColdStartPlan {
    Demo,
    LargeDemo,
    Memory,
    Persistent {
        document_id: cditor_core::ids::DocumentId,
        label: String,
        timeout: std::time::Duration,
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
            CditorBackend::Persistent { provider } => match options.document_id {
                Some(document_id) => Self::Persistent {
                    document_id,
                    label: provider.label().to_owned(),
                    timeout: provider.open_timeout(),
                },
                None => Self::Invalid {
                    reason: format!("{} backend requires document_id", provider.label()),
                },
            },
            CditorBackend::Cloud { endpoint } => Self::Cloud {
                endpoint: endpoint.clone(),
            },
        }
    }

    pub fn persistent_label(&self) -> Option<String> {
        match self {
            Self::Persistent {
                document_id, label, ..
            } => Some(format!("{label} document {document_id}")),
            _ => None,
        }
    }

    pub fn timeout(&self) -> std::time::Duration {
        match self {
            Self::Persistent { timeout, .. } => *timeout,
            _ => std::time::Duration::from_secs(90),
        }
    }
}

#[derive(Debug)]
pub struct CditorRuntimeLoadResult {
    pub runtime: DocumentRuntime,
    pub report: DocumentRuntimeColdStartReport,
    pub storage_session: StorageSession,
    pub schema_access: DocumentSchemaAccess,
    pub recovered_transactions: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSchemaAccess {
    ReadWrite,
    ReadOnlyNewerMajor {
        written_major: u64,
        supported_major: u32,
    },
    ReadOnlyNewerOperationMajor {
        written_major: u32,
        supported_major: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRuntimeLoadOptions {
    pub viewport_height: u32,
    pub visible_index_version: i64,
    pub initial_payload_window_blocks: usize,
    pub layout_key: LayoutCacheKey,
    pub page_policy_version: u64,
}

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
        CditorBackend::Persistent { provider } => provider.open().await?,
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
    let backend_kind = storage.backend_kind();
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
    let mut schema_access = document_schema_access(loaded.metadata.schema_version, backend_kind)?;
    let emergency_decision = if storage.capabilities().emergency_log {
        plan_emergency_recovery(storage.load_emergency_transactions(document_id).await?)
            .map_err(cditor_storage::StorageError::CorruptData)?
    } else {
        EmergencyRecoveryDecision::Replay(EmergencyRecoveryPlan {
            transactions: Vec::new(),
            affected_block_ids: Vec::new(),
            through_sequence: None,
        })
    };
    let recovery_plan = match emergency_decision {
        EmergencyRecoveryDecision::Replay(plan)
            if matches!(schema_access, DocumentSchemaAccess::ReadWrite) =>
        {
            Some(plan)
        }
        EmergencyRecoveryDecision::Replay(_) => None,
        EmergencyRecoveryDecision::ReadOnlyNewerMajor { written_major, .. } => {
            schema_access = DocumentSchemaAccess::ReadOnlyNewerOperationMajor {
                written_major,
                supported_major: cditor_core::schema::SchemaDomain::Operation
                    .current_version()
                    .major,
            };
            None
        }
    };
    let viewport_height = options.viewport_height;
    let (mut runtime, report) = runtime_from_loaded(loaded, viewport_height, &options)?;
    let recovered_transactions = if let Some(plan) = recovery_plan {
        hydrate_emergency_payloads(storage.as_ref(), &mut runtime, &plan).await?;
        Some(
            project_emergency_recovery(&mut runtime, plan)
                .map(|report| report.replayed_transactions)
                .map_err(cditor_storage::StorageError::CorruptData)?,
        )
    } else {
        None
    }
    .unwrap_or(0);
    Ok(CditorRuntimeLoadResult {
        storage_session: StorageSession::new(storage, runtime.document_id())
            .with_layout_key(options.layout_key),
        runtime,
        report,
        schema_access,
        recovered_transactions,
    })
}

fn document_schema_access(
    stored_major: u64,
    backend: StorageBackendKind,
) -> StorageResult<DocumentSchemaAccess> {
    let supported_major = CURRENT_DOCUMENT_FORMAT.major;
    match stored_major.cmp(&u64::from(supported_major)) {
        std::cmp::Ordering::Equal => Ok(DocumentSchemaAccess::ReadWrite),
        std::cmp::Ordering::Greater => Ok(DocumentSchemaAccess::ReadOnlyNewerMajor {
            written_major: stored_major,
            supported_major,
        }),
        std::cmp::Ordering::Less => Err(cditor_storage::StorageError::Migration {
            backend,
            message: format!(
                "document schema v{stored_major} must be migrated to v{supported_major} before opening"
            ),
        }),
    }
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
            runtime.visible_block_ids(),
        )
        && runtime.apply_cached_page_layout(page_layout).is_ok()
    {
        report.page_layout_cache_hit = true;
    }
    Ok((runtime, report))
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
    use crate::Cditor;
    use crate::CditorStorageExt;
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::edit::{ChangeOrigin, EditOperation, EditTransaction, EditTransactionKind};
    use cditor_core::layout::{HeightConfidence, PageLayout, PageLayoutIndex};
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
    use cditor_session::{
        PersistencePipeline, project_persistence_save_success, save_storage_batch,
    };
    use cditor_storage::{
        StorageBackendKind, StorageDocumentMetadata, StoragePageLayoutSnapshot, StorageSaveBatch,
    };
    use cditor_storage_postgres::{
        DocumentRow, PostgresDocumentStorage, PostgresDocumentStore, PostgresPayloadStore,
        PostgresPoolConfig, create_pg_pool, pg_document_id_from_runtime, run_migrations,
    };
    use cditor_storage_sqlite::{SqliteDocumentStorage, SqliteStorageOptions};
    use sqlx::types::Uuid;
    use tempfile::TempDir;

    #[test]
    fn cold_start_plan_requires_document_id_for_persistent_backends() {
        let postgres = Cditor::new()
            .with_postgres_url("postgres://localhost/cditor")
            .into_options();
        assert!(matches!(
            CditorColdStartPlan::from_options(&postgres),
            CditorColdStartPlan::Invalid { .. }
        ));
    }

    #[test]
    fn document_schema_access_separates_writable_newer_and_migration_modes() {
        assert_eq!(
            document_schema_access(
                u64::from(CURRENT_DOCUMENT_FORMAT.major),
                StorageBackendKind::Sqlite,
            )
            .unwrap(),
            DocumentSchemaAccess::ReadWrite
        );
        assert_eq!(
            document_schema_access(
                u64::from(CURRENT_DOCUMENT_FORMAT.major) + 1,
                StorageBackendKind::Sqlite,
            )
            .unwrap(),
            DocumentSchemaAccess::ReadOnlyNewerMajor {
                written_major: u64::from(CURRENT_DOCUMENT_FORMAT.major) + 1,
                supported_major: CURRENT_DOCUMENT_FORMAT.major,
            }
        );
        assert!(
            document_schema_access(
                u64::from(CURRENT_DOCUMENT_FORMAT.major) - 1,
                StorageBackendKind::Sqlite,
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn sqlite_newer_document_schema_loads_runtime_in_readonly_mode() {
        let temp = TempDir::new().unwrap();
        let storage = SqliteDocumentStorage::open(SqliteStorageOptions::file(
            temp.path().join("newer-schema.cditor.db"),
        ))
        .await
        .unwrap();
        let options = StorageRuntimeLoadOptions::default();
        storage
            .load_document(LoadDocumentRequest {
                document_id: 42,
                workspace_id: 1,
                initial_payload_window_blocks: options.initial_payload_window_blocks,
                visible_index_version: options.visible_index_version,
                layout_key: options.layout_key,
                page_policy_version: options.page_policy_version,
            })
            .await
            .unwrap();
        let newer_major = u64::from(CURRENT_DOCUMENT_FORMAT.major) + 1;
        sqlx::query("UPDATE documents SET schema_version = ?")
            .bind(i64::try_from(newer_major).unwrap())
            .execute(storage.pool())
            .await
            .unwrap();

        let loaded = load_runtime_from_storage(Arc::new(storage), 42, 1, options)
            .await
            .unwrap();

        assert_eq!(loaded.runtime.document_id(), 42);
        assert_eq!(loaded.runtime.document_block_count(), 1);
        assert_eq!(
            loaded.schema_access,
            DocumentSchemaAccess::ReadOnlyNewerMajor {
                written_major: newer_major,
                supported_major: CURRENT_DOCUMENT_FORMAT.major,
            }
        );
    }

    #[tokio::test]
    async fn sqlite_cold_start_replays_durable_emergency_log_into_dirty_runtime() {
        let temp = TempDir::new().unwrap();
        let storage = Arc::new(
            SqliteDocumentStorage::open(SqliteStorageOptions::file(
                temp.path().join("emergency-recovery.cditor.db"),
            ))
            .await
            .unwrap(),
        );
        let options = StorageRuntimeLoadOptions::default();
        storage
            .load_document(LoadDocumentRequest {
                document_id: 42,
                workspace_id: 1,
                initial_payload_window_blocks: options.initial_payload_window_blocks,
                visible_index_version: options.visible_index_version,
                layout_key: options.layout_key,
                page_policy_version: options.page_policy_version,
            })
            .await
            .unwrap();
        let records = (1..=100)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
            })
            .collect::<Vec<_>>();
        let payloads = (1..=100)
            .map(|block_id| {
                BlockPayloadRecord::rich_text(
                    block_id,
                    RichBlockKind::Paragraph,
                    block_id.to_string(),
                )
            })
            .collect::<Vec<_>>();
        storage
            .commit(StorageSaveBatch {
                document_id: 42,
                layout_key: None,
                payloads,
                index_records: records,
                structure_version: 2,
                transactions: Vec::new(),
                block_attrs: Vec::new(),
                page_layout_snapshot: None,
            })
            .await
            .unwrap();
        let transaction = EditTransaction::new(
            10,
            EditTransactionKind::Typing,
            10,
            vec![EditOperation::InsertText {
                block_id: 100,
                offset: 3,
                text: " recovered".to_owned(),
            }],
            vec![EditOperation::DeleteText {
                block_id: 100,
                range: 3..13,
            }],
        )
        .with_origin(ChangeOrigin::User);
        storage
            .append_emergency_transactions(42, &[transaction])
            .await
            .unwrap();

        let loaded = load_runtime_from_storage(storage.clone(), 42, 1, options.clone())
            .await
            .unwrap();

        assert_eq!(loaded.recovered_transactions, 1);
        assert_eq!(
            loaded
                .runtime
                .block_payload_record(100)
                .unwrap()
                .plain_text(),
            "100 recovered"
        );
        assert_eq!(loaded.runtime.pending_structure_transaction_count(), 1);
        assert_eq!(
            storage.load_emergency_transactions(42).await.unwrap().len(),
            1
        );

        let mut runtime = loaded.runtime;
        let mut pipeline = PersistencePipeline::for_session(loaded.storage_session, None);
        pipeline.mark_loaded_structure_version(runtime.structure_version());
        pipeline.mark_dirty();
        let request = pipeline.begin_batch(&mut runtime).expect("recovery save");
        let outcome = save_storage_batch(&request).await.unwrap();
        assert!(!pipeline.finish_success(&request, outcome.saved_structure_version));
        project_persistence_save_success(&mut runtime, &outcome, true, false);
        assert!(
            storage
                .load_emergency_transactions(42)
                .await
                .unwrap()
                .is_empty()
        );

        let reopened = load_runtime_from_storage(storage, 42, 1, options)
            .await
            .unwrap();
        assert_eq!(reopened.recovered_transactions, 0);
        let persisted = reopened
            .storage_session
            .load_payloads(&[100])
            .await
            .unwrap();
        assert_eq!(
            persisted.records.first().unwrap().plain_text(),
            "100 recovered"
        );
    }

    #[test]
    fn cold_start_plan_maps_persistent_provider() {
        let options = Cditor::new()
            .with_document_id(42)
            .with_sqlite_path("workspace.cditor.db")
            .into_options();
        assert_eq!(
            CditorColdStartPlan::from_options(&options),
            CditorColdStartPlan::Persistent {
                document_id: 42,
                label: "SQLite".to_owned(),
                timeout: std::time::Duration::from_secs(90),
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
        assert_eq!(runtime.page_layout_total_height(), 321.0);
        assert_eq!(
            runtime.model_total_height(),
            runtime.page_layout_total_height() + runtime.down_placer_height()
        );
        runtime.sync_viewport_height(800.0).unwrap();
        assert_eq!(
            runtime.model_total_height(),
            runtime.page_layout_total_height() + runtime.down_placer_height()
        );

        let mut invalid = loaded;
        invalid.page_layout_snapshot.as_mut().unwrap().pages[0].last_block_id = 999;
        let (runtime, report) = runtime_from_loaded(invalid, 720, &options).unwrap();
        assert!(!report.page_layout_cache_hit);
        assert_ne!(runtime.page_layout_total_height(), 321.0);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn injected_storage_loads_runtime_through_backend_neutral_host() {
        let database_url = std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned());
        let pool = create_pg_pool(&PostgresPoolConfig::for_tests(database_url))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let document_store = PostgresDocumentStore::new(pool.clone());
        let payload_store = PostgresPayloadStore::new(pool.clone());
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
        document_store
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
        document_store
            .save_block_index_records(document.id, &records, 1)
            .await
            .unwrap();
        payload_store
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
            .with_storage(
                Arc::new(PostgresDocumentStorage::from_pool(pool)),
                "PostgreSQL",
            )
            .into_options();
        let loaded = load_runtime_from_options(&options).await.unwrap().unwrap();
        assert_eq!(loaded.runtime.document_id(), runtime_document_id);
        assert_eq!(loaded.report.payloads_loaded, 1);
        assert_eq!(
            loaded.storage_session.backend_kind(),
            StorageBackendKind::Postgres
        );
    }
}
