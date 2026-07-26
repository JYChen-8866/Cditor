use cditor_storage::{DocumentStorage, LoadDocumentRequest};
use cditor_storage_sqlite::{SqliteDocumentStorage, SqliteStorageOptions};
use cditor_test_support::{
    StorageContractConfig, run_document_storage_contract, seed_mixed_storage_document,
};
use tempfile::TempDir;

async fn open(temp: &TempDir) -> SqliteDocumentStorage {
    SqliteDocumentStorage::open(SqliteStorageOptions::file(
        temp.path().join("shared-contract.cditor.db"),
    ))
    .await
    .unwrap()
}

#[tokio::test]
async fn sqlite_passes_shared_document_storage_contract() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    run_document_storage_contract(
        &storage,
        StorageContractConfig {
            backend: cditor_storage::StorageBackendKind::Sqlite,
            document_id: 8_001,
            isolated_document_id: 8_002,
        },
    )
    .await;
}

#[tokio::test]
async fn sqlite_accepts_backend_neutral_mixed_test_seed() {
    let temp = TempDir::new().unwrap();
    let storage = open(&temp).await;
    let report = seed_mixed_storage_document(&storage, 8_101, 64)
        .await
        .unwrap();
    let loaded = storage
        .load_document(LoadDocumentRequest {
            document_id: 8_101,
            workspace_id: 1,
            initial_payload_window_blocks: 128,
            visible_index_version: cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION,
            layout_key: cditor_storage::layout_cache::LayoutCacheKey {
                width_bucket: 10,
                exact_width_px: 800,
                content_version: 1,
                attrs_version: 0,
                style_version: 0,
                font_version: 0,
                theme_version: 0,
                scale_factor_milli: 1_000,
            },
            page_policy_version: cditor_core::layout::PAGE_POLICY_VERSION,
        })
        .await
        .unwrap();

    assert_eq!(report.block_count, 64);
    assert_eq!(loaded.records.len(), 64);
    assert_eq!(loaded.initial_payloads.len(), 64);
}
