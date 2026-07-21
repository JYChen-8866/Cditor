use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::{EditTransaction, EditTransactionKind};
use cditor_core::layout::{
    HeightConfidence, PAGE_POLICY_VERSION, PageLayout, PageLayoutIndex, PagePolicy,
};
use cditor_core::rich_text::{
    BlockAttrs, BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind,
};
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{
    DOCUMENT_INDEX_VISIBLE_VERSION, DocumentStorage, LoadDocumentRequest,
    StoragePageLayoutSnapshot, StorageSaveBatch,
};
use cditor_storage_sqlite::{SqliteDocumentStorage, SqliteStorageOptions};
use tempfile::TempDir;

fn request(document_id: u64) -> LoadDocumentRequest {
    LoadDocumentRequest {
        document_id,
        workspace_id: 1,
        initial_payload_window_blocks: 128,
        visible_index_version: cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION,
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
        page_policy_version: cditor_core::layout::PAGE_POLICY_VERSION,
    }
}

async fn open(temp: &TempDir) -> SqliteDocumentStorage {
    SqliteDocumentStorage::open(SqliteStorageOptions::file(
        temp.path().join("workspace.cditor.db"),
    ))
    .await
    .unwrap()
}

fn page_snapshot(
    records: &[BlockIndexRecord],
    structure_version: u64,
    layout_key: LayoutCacheKey,
    height: f64,
) -> StoragePageLayoutSnapshot {
    let page_layout = PageLayoutIndex::from_cached_pages(
        vec![PageLayout {
            page_index: 0,
            block_start: 0,
            block_count: records.len(),
            height,
            measured_ratio: 1.0,
            confidence: HeightConfidence::Exact,
            max_error_hint: 0.0,
            dirty: false,
        }],
        PagePolicy::default(),
        records.len(),
    )
    .unwrap();
    StoragePageLayoutSnapshot::from_page_layout(
        DOCUMENT_INDEX_VISIBLE_VERSION,
        structure_version,
        layout_key,
        PAGE_POLICY_VERSION,
        &page_layout,
        &records.iter().map(|record| record.id).collect::<Vec<_>>(),
    )
    .unwrap()
}

#[tokio::test]
async fn sqlite_migrates_with_required_connection_pragmas() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;

    let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(storage.pool())
        .await
        .unwrap();

    assert_eq!(journal.to_ascii_lowercase(), "wal");
    assert_eq!(foreign_keys, 1);
    assert_eq!(synchronous, 2);
}

#[tokio::test]
async fn sqlite_respects_create_if_missing() {
    let temp = TempDir::new().unwrap();
    let options =
        SqliteStorageOptions::file(temp.path().join("missing.cditor.db")).create_if_missing(false);
    let error = SqliteDocumentStorage::open(options).await.unwrap_err();
    assert!(error.to_string().contains("does not exist"));
}

#[tokio::test]
async fn sqlite_rejects_invalid_public_pool_configuration() {
    let temp = TempDir::new().unwrap();
    let mut options = SqliteStorageOptions::file(temp.path().join("invalid.cditor.db"));
    options.max_connections = 0;
    let error = SqliteDocumentStorage::open(options).await.unwrap_err();
    assert!(error.to_string().contains("max_connections"));
}

#[tokio::test]
async fn sqlite_document_round_trips_across_reopen() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(42)).await.unwrap();
    assert_eq!(loaded.records.len(), 1);
    assert_eq!(loaded.initial_payloads[0].plain_text(), "");

    let first_block_id = loaded.records[0].id;
    let mut payload =
        BlockPayloadRecord::rich_text(first_block_id, RichBlockKind::Paragraph, "saved in sqlite");
    payload.content_version = 2;
    let outcome = storage
        .commit(StorageSaveBatch {
            document_id: 42,
            layout_key: None,
            payloads: vec![payload],
            index_records: Vec::new(),
            structure_version: loaded.metadata.structure_version,
            transactions: Vec::new(),
            block_attrs: vec![(first_block_id, BlockAttrs::default())],
            page_layout_snapshot: None,
        })
        .await
        .unwrap();
    assert_eq!(outcome.saved_payload_versions, vec![(first_block_id, 2)]);
    storage.flush().await.unwrap();
    storage.pool().close().await;

    let reopened = open(&temp).await;
    let loaded = reopened.load_document(request(42)).await.unwrap();
    assert_eq!(loaded.initial_payloads[0].plain_text(), "saved in sqlite");
    assert_eq!(loaded.initial_payloads[0].content_version, 2);
}

#[tokio::test]
async fn sqlite_commit_is_atomic_when_a_payload_references_a_missing_block() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(7)).await.unwrap();
    let existing = loaded.records[0];
    let inserted = BlockIndexRecord::new(
        99,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
        0,
    );
    let missing_payload =
        BlockPayloadRecord::rich_text(100, RichBlockKind::Paragraph, "must roll back");
    let index_records = vec![existing, inserted];
    let layout_key = request(7).layout_key;
    let page_layout_snapshot = page_snapshot(&index_records, 2, layout_key, 77.0);

    let result = storage
        .commit(StorageSaveBatch {
            document_id: 7,
            layout_key: Some(layout_key),
            payloads: vec![missing_payload],
            index_records,
            structure_version: 2,
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: Some(page_layout_snapshot),
        })
        .await;
    assert!(result.is_err());

    let reloaded = storage.load_document(request(7)).await.unwrap();
    assert_eq!(reloaded.metadata.structure_version, 1);
    assert_eq!(reloaded.records.len(), 1);
    assert_eq!(reloaded.records[0].id, existing.id);
    let saved_pages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM page_layout")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(saved_pages, 0);
}

#[tokio::test]
async fn sqlite_transaction_log_preserves_full_edit_operations() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(11)).await.unwrap();
    let block_id = loaded.records[0].id;
    let edit = EditTransaction::insert_text(9, 123, block_id, 0, "A");

    storage
        .commit(StorageSaveBatch {
            document_id: 11,
            layout_key: None,
            payloads: Vec::new(),
            index_records: loaded.records,
            structure_version: 2,
            transactions: vec![edit.clone()],
            block_attrs: Vec::new(),
            page_layout_snapshot: None,
        })
        .await
        .unwrap();

    let json: String = sqlx::query_scalar(
        "SELECT transaction_json FROM edit_transactions WHERE transaction_id = '9'",
    )
    .fetch_one(storage.pool())
    .await
    .unwrap();
    let decoded: EditTransaction = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, edit);
    assert_eq!(decoded.kind, EditTransactionKind::Typing);
}

#[tokio::test]
async fn sqlite_database_can_hold_multiple_documents_with_local_block_ids() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;

    let first = storage.load_document(request(1)).await.unwrap();
    let second = storage.load_document(request(2)).await.unwrap();

    assert_eq!(first.records[0].id, 1);
    assert_eq!(second.records[0].id, 1);
    assert_eq!(first.initial_payloads[0].plain_text(), "");
    assert_eq!(second.initial_payloads[0].plain_text(), "");
}

#[tokio::test]
async fn sqlite_large_index_loads_only_the_initial_payload_window() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    storage.load_document(request(50)).await.unwrap();
    let paragraph = RichBlockKind::Paragraph;
    let records = (1..=10_000)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&paragraph),
                0,
            )
        })
        .collect::<Vec<_>>();
    let payloads = (1..=128)
        .map(|block_id| {
            BlockPayloadRecord::rich_text(block_id, paragraph.clone(), format!("block {block_id}"))
        })
        .collect::<Vec<_>>();
    storage
        .commit(StorageSaveBatch {
            document_id: 50,
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

    let loaded = storage.load_document(request(50)).await.unwrap();
    assert_eq!(loaded.records.len(), 10_000);
    assert_eq!(loaded.initial_payloads.len(), 128);
    assert_eq!(loaded.initial_payload_window_end, 128);
    assert!(loaded.index_from_snapshot);
}

#[tokio::test]
async fn sqlite_index_snapshot_hits_exact_version_and_stale_version_falls_back() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(60)).await.unwrap();
    let mut records = loaded.records;
    records.push(BlockIndexRecord::new(
        2,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
        0,
    ));
    storage
        .commit(StorageSaveBatch {
            document_id: 60,
            layout_key: None,
            payloads: vec![BlockPayloadRecord::rich_text(
                2,
                RichBlockKind::Paragraph,
                "snapshot",
            )],
            index_records: records,
            structure_version: 2,
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: None,
        })
        .await
        .unwrap();

    let exact = storage.load_document(request(60)).await.unwrap();
    assert!(exact.index_from_snapshot);
    assert_eq!(exact.records.len(), 2);

    sqlx::query("UPDATE documents SET structure_version = 3")
        .execute(storage.pool())
        .await
        .unwrap();
    let stale = storage.load_document(request(60)).await.unwrap();
    assert!(!stale.index_from_snapshot);
    assert_eq!(stale.records.len(), 2);
}

#[tokio::test]
async fn sqlite_corrupt_index_snapshot_is_rebuilt_from_blocks() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(61)).await.unwrap();
    storage
        .commit(StorageSaveBatch {
            document_id: 61,
            layout_key: None,
            payloads: Vec::new(),
            index_records: loaded.records,
            structure_version: 2,
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: None,
        })
        .await
        .unwrap();
    sqlx::query(
        "UPDATE document_index_snapshot SET snapshot_json = '{\"format\":\"block_index_json_v1\",\"records\":[]}'",
    )
    .execute(storage.pool())
    .await
    .unwrap();

    let rebuilt = storage.load_document(request(61)).await.unwrap();
    assert!(!rebuilt.index_from_snapshot);
    assert_eq!(rebuilt.records.len(), 1);
}

#[tokio::test]
async fn sqlite_versioned_layout_cache_survives_reopen_and_degrades_stale_keys() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(70)).await.unwrap();
    let mut record = loaded.records[0];
    record.layout_meta.estimated_height = 64.0;
    record.layout_meta.measured_height = Some(96.0);
    record.layout_meta.width_bucket = request(70).layout_key.width_bucket;
    record.layout_meta.layout_version = 4;
    record.layout_meta.dirty = true;
    let layout_key = request(70).layout_key;
    storage
        .commit(StorageSaveBatch {
            document_id: 70,
            layout_key: Some(layout_key),
            payloads: Vec::new(),
            index_records: vec![record],
            structure_version: 2,
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: None,
        })
        .await
        .unwrap();
    storage.flush().await.unwrap();
    storage.pool().close().await;

    let reopened = open(&temp).await;
    let exact = reopened.load_document(request(70)).await.unwrap();
    assert!(exact.index_from_snapshot);
    assert_eq!(exact.layout_cache_hits, 1);
    assert_eq!(exact.records[0].layout_meta.measured_height, Some(96.0));
    assert!(!exact.records[0].layout_meta.dirty);

    let mut stale_request = request(70);
    stale_request.layout_key.font_version = 9;
    let stale = reopened.load_document(stale_request).await.unwrap();
    assert_eq!(stale.layout_cache_hits, 1);
    assert_eq!(stale.records[0].layout_meta.measured_height, None);
    assert_eq!(stale.records[0].layout_meta.estimated_height, 96.0);
    assert!(stale.records[0].layout_meta.dirty);
}

#[tokio::test]
async fn sqlite_page_layout_cache_hits_exact_context_and_survives_reopen() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(80)).await.unwrap();
    let records = loaded.records;
    let layout_key = request(80).layout_key;
    let page_layout_snapshot = page_snapshot(&records, 2, layout_key, 123.0);
    storage
        .commit(StorageSaveBatch {
            document_id: 80,
            layout_key: Some(layout_key),
            payloads: Vec::new(),
            index_records: records.clone(),
            structure_version: 2,
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: Some(page_layout_snapshot),
        })
        .await
        .unwrap();

    let exact = storage.load_document(request(80)).await.unwrap();
    let page_layout = exact
        .page_layout_snapshot
        .unwrap()
        .to_page_layout_index(
            DOCUMENT_INDEX_VISIBLE_VERSION,
            2,
            layout_key,
            PAGE_POLICY_VERSION,
            PagePolicy::default(),
            &[records[0].id],
        )
        .unwrap();
    assert_eq!(page_layout.total_height(), 123.0);

    let mut stale_request = request(80);
    stale_request.layout_key.font_version = 99;
    assert!(
        storage
            .load_document(stale_request)
            .await
            .unwrap()
            .page_layout_snapshot
            .is_none()
    );

    storage.flush().await.unwrap();
    storage.pool().close().await;
    let reopened = open(&temp).await;
    assert!(
        reopened
            .load_document(request(80))
            .await
            .unwrap()
            .page_layout_snapshot
            .is_some()
    );
}

#[tokio::test]
async fn sqlite_corrupt_page_coverage_is_treated_as_a_cache_miss() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(81)).await.unwrap();
    let layout_key = request(81).layout_key;
    let snapshot = page_snapshot(&loaded.records, 2, layout_key, 91.0);
    storage
        .commit(StorageSaveBatch {
            document_id: 81,
            layout_key: Some(layout_key),
            payloads: Vec::new(),
            index_records: loaded.records,
            structure_version: 2,
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: Some(snapshot),
        })
        .await
        .unwrap();
    sqlx::query("UPDATE page_layout SET block_start_index = 1")
        .execute(storage.pool())
        .await
        .unwrap();

    let loaded = storage.load_document(request(81)).await.unwrap();
    assert!(loaded.page_layout_snapshot.is_none());
}

#[tokio::test]
async fn sqlite_corrupt_page_boundary_fails_common_projection_validation() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(82)).await.unwrap();
    let block_id = loaded.records[0].id;
    let layout_key = request(82).layout_key;
    let snapshot = page_snapshot(&loaded.records, 2, layout_key, 92.0);
    storage
        .commit(StorageSaveBatch {
            document_id: 82,
            layout_key: Some(layout_key),
            payloads: Vec::new(),
            index_records: loaded.records,
            structure_version: 2,
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: Some(snapshot),
        })
        .await
        .unwrap();
    sqlx::query("UPDATE page_layout SET last_block_id = ?")
        .bind(uuid::Uuid::from_u128(
            0x2000_0000_0000_0000_0000_0000_0000_0999,
        ))
        .execute(storage.pool())
        .await
        .unwrap();

    let snapshot = storage
        .load_document(request(82))
        .await
        .unwrap()
        .page_layout_snapshot
        .unwrap();
    assert!(
        snapshot
            .to_page_layout_index(
                DOCUMENT_INDEX_VISIBLE_VERSION,
                2,
                layout_key,
                PAGE_POLICY_VERSION,
                PagePolicy::default(),
                &[block_id],
            )
            .is_err()
    );
}

#[tokio::test]
async fn sqlite_stale_page_structure_and_policy_versions_are_cache_misses() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let loaded = storage.load_document(request(83)).await.unwrap();
    let layout_key = request(83).layout_key;
    let snapshot = page_snapshot(&loaded.records, 2, layout_key, 93.0);
    storage
        .commit(StorageSaveBatch {
            document_id: 83,
            layout_key: Some(layout_key),
            payloads: Vec::new(),
            index_records: loaded.records,
            structure_version: 2,
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: Some(snapshot),
        })
        .await
        .unwrap();

    let mut policy_mismatch = request(83);
    policy_mismatch.page_policy_version += 1;
    assert!(
        storage
            .load_document(policy_mismatch)
            .await
            .unwrap()
            .page_layout_snapshot
            .is_none()
    );

    sqlx::query("UPDATE documents SET structure_version = 3 WHERE id = ?")
        .bind(uuid::Uuid::from_u128(
            0x1000_0000_0000_0000_0000_0000_0000_0053,
        ))
        .execute(storage.pool())
        .await
        .unwrap();
    assert!(
        storage
            .load_document(request(83))
            .await
            .unwrap()
            .page_layout_snapshot
            .is_none()
    );
}
