use super::*;
use crate::CditorStorageExt;
use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::{ChangeOrigin, EditOperation, EditTransaction, EditTransactionKind};
use cditor_core::layout::{HeightConfidence, PageLayout, PageLayoutIndex};
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
use cditor_sdk::Cditor;
use cditor_session::{SessionColdStartRequest, open_editor_session, save_storage_batch};
use cditor_storage::{
    StorageBackendKind, StorageDocumentMetadata, StoragePageLayoutSnapshot, StorageSaveBatch,
};
use cditor_storage_sqlite::{SqliteDocumentStorage, SqliteStorageOptions};
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
async fn sqlite_newer_document_schema_loads_session_in_readonly_mode() {
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
        .execute(storage.test_pool())
        .await
        .unwrap();

    let mut progress_events = Vec::new();
    let loaded = load_session_from_storage(Arc::new(storage), 42, 1, options, &mut |progress| {
        progress_events.push(progress)
    })
    .await
    .unwrap();

    let opened = loaded.prepared.into_opened();
    let snapshot = opened.session.snapshot().unwrap();
    assert_eq!(snapshot.document_id, 42);
    assert_eq!(snapshot.block_count, 1);
    assert!(snapshot.readonly);
    assert_eq!(
        loaded.schema_access,
        DocumentSchemaAccess::ReadOnlyNewerMajor {
            written_major: newer_major,
            supported_major: CURRENT_DOCUMENT_FORMAT.major,
        }
    );
    assert!(
        progress_events
            .windows(2)
            .all(|events| events[0].completed_units <= events[1].completed_units)
    );
    assert_eq!(progress_events.last().unwrap().percentage(), 100);
    assert!(
        progress_events
            .iter()
            .any(|progress| progress.message == "已读取首屏内容")
    );
}

#[tokio::test]
async fn sqlite_cold_start_replays_durable_emergency_log_into_dirty_session() {
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
            BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, block_id.to_string())
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

    let loaded = load_session_from_storage(storage.clone(), 42, 1, options.clone(), &mut |_| {})
        .await
        .unwrap();

    let opened = loaded.prepared.into_opened();
    assert_eq!(
        opened
            .emergency_recovery
            .as_ref()
            .map(|report| report.replayed_transactions),
        Some(1)
    );
    assert_eq!(
        opened.session.block_plain_text(100).unwrap().as_deref(),
        Some("100 recovered")
    );
    assert_eq!(
        opened
            .session
            .persistence_runtime_snapshot()
            .unwrap()
            .pending_structure_transactions,
        1
    );
    assert_eq!(
        storage.load_emergency_transactions(42).await.unwrap().len(),
        1
    );

    let request = opened
        .session
        .capture_storage_save()
        .unwrap()
        .expect("recovery save");
    let outcome = save_storage_batch(&request).await.unwrap();
    opened
        .session
        .apply_storage_save_success(&request, &outcome)
        .unwrap();
    assert!(
        storage
            .load_emergency_transactions(42)
            .await
            .unwrap()
            .is_empty()
    );

    let reopened = load_session_from_storage(storage.clone(), 42, 1, options, &mut |_| {})
        .await
        .unwrap();
    let reopened = reopened.prepared.into_opened();
    assert_eq!(
        reopened
            .emergency_recovery
            .as_ref()
            .map_or(0, |report| report.replayed_transactions),
        0
    );
    let persisted = storage.load_payloads(42, &[100]).await.unwrap();
    assert_eq!(persisted.records[0].plain_text(), "100 recovered");
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

    let cached = cached_page_layout(&loaded, &options);
    assert!(cached.is_some());
    let result = open_editor_session(SessionColdStartRequest {
        data: cold_start_data(loaded.clone()),
        viewport_height: 720.0,
        cached_page_layout: cached,
        readonly: false,
        emergency_log_entries: Vec::new(),
        emergency_payloads: Vec::new(),
    })
    .unwrap();
    assert!(result.report.page_layout_cache_hit);

    let mut invalid = loaded;
    invalid.page_layout_snapshot.as_mut().unwrap().pages[0].last_block_id = 999;
    assert!(cached_page_layout(&invalid, &options).is_none());
}
