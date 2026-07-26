use cditor_core::edit::{
    AssetEditOperation, AssetSnapshot, AssetState, EditOperation, EditTransaction,
    EditTransactionKind,
};
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{
    AssetUploadMutation, DocumentStorage, LoadDocumentRequest, ProvisionalAssetRequest,
    StorageError, StorageSaveBatch,
};
use cditor_storage_sqlite::{SqliteDocumentStorage, SqliteStorageOptions};
use tempfile::TempDir;

async fn open_store(dir: &TempDir) -> SqliteDocumentStorage {
    SqliteDocumentStorage::open(SqliteStorageOptions::file(dir.path().join("assets.db")))
        .await
        .unwrap()
}

async fn ensure_workspace(store: &SqliteDocumentStorage) {
    store
        .load_document(LoadDocumentRequest {
            document_id: 7,
            workspace_id: 1,
            initial_payload_window_blocks: 1,
            visible_index_version: cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION,
            layout_key: LayoutCacheKey {
                width_bucket: 1,
                exact_width_px: 800,
                content_version: 1,
                attrs_version: 0,
                style_version: 0,
                font_version: 0,
                theme_version: 0,
                scale_factor_milli: 1000,
            },
            page_policy_version: cditor_core::layout::PAGE_POLICY_VERSION,
        })
        .await
        .unwrap();
}

fn request(asset_id: u64, hash_byte: char) -> ProvisionalAssetRequest {
    ProvisionalAssetRequest {
        workspace_id: 1,
        asset: AssetSnapshot {
            asset_id,
            file_name: "image.png".to_owned(),
            media_type: "image/png".to_owned(),
            size_bytes: 10,
            source: format!("file:///tmp/{asset_id}.png"),
            checksum: Some(std::iter::repeat_n(hash_byte, 64).collect()),
            state: AssetState::LocalPending,
        },
    }
}

#[tokio::test]
async fn provisional_upload_enforces_session_progress_and_completion_invariants() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    ensure_workspace(&store).await;
    let created = store.create_provisional_asset(request(20, 'a')).await.unwrap();
    assert_eq!(created.asset.state, AssetState::LocalPending);
    assert_eq!(store.pending_asset_uploads(1, 10).await.unwrap().len(), 1);

    let uploading = store
        .update_asset_upload(
            20,
            AssetUploadMutation::Begin {
                upload_session_id: "upload-1".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(uploading.asset.state, AssetState::Uploading);
    assert_eq!(uploading.attempt_count, 1);
    assert!(
        store
            .update_asset_upload(
                20,
                AssetUploadMutation::Progress {
                    upload_session_id: "stale-upload".to_owned(),
                    uploaded_bytes: 5,
                },
            )
            .await
            .is_err()
    );
    let progress = store
        .update_asset_upload(
            20,
            AssetUploadMutation::Progress {
                upload_session_id: "upload-1".to_owned(),
                uploaded_bytes: 5,
            },
        )
        .await
        .unwrap();
    assert_eq!(progress.uploaded_bytes, 5);
    for invalid in [4, 11] {
        assert!(
            store
                .update_asset_upload(
                    20,
                    AssetUploadMutation::Progress {
                        upload_session_id: "upload-1".to_owned(),
                        uploaded_bytes: invalid,
                    },
                )
                .await
                .is_err()
        );
    }
    assert!(
        store
            .update_asset_upload(
                20,
                AssetUploadMutation::Complete {
                    upload_session_id: "upload-1".to_owned(),
                    canonical_asset_id: 200,
                    remote_object_key: "objects/a".to_owned(),
                    public_url: None,
                },
            )
            .await
            .is_err()
    );
    store
        .update_asset_upload(
            20,
            AssetUploadMutation::Progress {
                upload_session_id: "upload-1".to_owned(),
                uploaded_bytes: 10,
            },
        )
        .await
        .unwrap();
    let ready = store
        .update_asset_upload(
            20,
            AssetUploadMutation::Complete {
                upload_session_id: "upload-1".to_owned(),
                canonical_asset_id: 200,
                remote_object_key: "objects/a".to_owned(),
                public_url: Some("https://cdn.example/a".to_owned()),
            },
        )
        .await
        .unwrap();
    assert_eq!(ready.asset.state, AssetState::Ready);
    assert_eq!(ready.canonical_asset_id, Some(200));
    assert!(store.pending_asset_uploads(1, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_upload_retries_with_a_new_session_and_persists_across_reopen() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    ensure_workspace(&store).await;
    store.create_provisional_asset(request(21, 'b')).await.unwrap();
    store
        .update_asset_upload(
            21,
            AssetUploadMutation::Begin {
                upload_session_id: "first".to_owned(),
            },
        )
        .await
        .unwrap();
    let failed = store
        .update_asset_upload(
            21,
            AssetUploadMutation::Fail {
                upload_session_id: Some("first".to_owned()),
                error: "network unavailable".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(failed.asset.state, AssetState::Failed);
    assert_eq!(failed.last_error.as_deref(), Some("network unavailable"));
    let retrying = store
        .update_asset_upload(
            21,
            AssetUploadMutation::Begin {
                upload_session_id: "second".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(retrying.attempt_count, 2);
    assert_eq!(retrying.uploaded_bytes, 0);
    assert!(retrying.last_error.is_none());
    store.pool().close().await;

    let reopened = open_store(&dir).await;
    let persisted = reopened.asset_manifest(21).await.unwrap().unwrap();
    assert_eq!(persisted.upload_session_id.as_deref(), Some("second"));
    assert_eq!(persisted.asset.state, AssetState::Uploading);
}

#[tokio::test]
async fn content_hash_deduplicates_and_invalid_provisional_input_is_rejected() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    ensure_workspace(&store).await;
    let first = store.create_provisional_asset(request(30, 'c')).await.unwrap();
    let deduplicated = store.create_provisional_asset(request(31, 'c')).await.unwrap();
    assert_eq!(deduplicated.asset.asset_id, first.asset.asset_id);

    let mut invalid = request(32, 'd');
    invalid.asset.checksum = Some("not-sha256".to_owned());
    let error = store.create_provisional_asset(invalid).await.unwrap_err();
    assert!(matches!(error, StorageError::CorruptData(_)));
}

#[tokio::test]
async fn asset_operations_atomically_materialize_and_detach_block_references() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    ensure_workspace(&store).await;
    let asset = request(40, 'e').asset;
    let attach = EditTransaction::new(
        1,
        EditTransactionKind::ExplicitCommand,
        2,
        vec![EditOperation::Asset(AssetEditOperation::Attach {
            block_id: 1,
            asset: asset.clone(),
        })],
        vec![EditOperation::Asset(AssetEditOperation::Detach {
            block_id: 1,
            asset: asset.clone(),
        })],
    );
    store
        .commit(StorageSaveBatch {
            document_id: 7,
            layout_key: None,
            payloads: vec![],
            index_records: vec![],
            structure_version: 1,
            transactions: vec![attach],
            block_attrs: vec![],
            page_layout_snapshot: None,
        })
        .await
        .unwrap();
    assert!(store.asset_manifest(40).await.unwrap().is_some());
    assert_eq!(store.asset_references(40).await.unwrap().len(), 1);

    let detach = EditTransaction::new(
        2,
        EditTransactionKind::ExplicitCommand,
        3,
        vec![EditOperation::Asset(AssetEditOperation::Detach {
            block_id: 1,
            asset: asset.clone(),
        })],
        vec![EditOperation::Asset(AssetEditOperation::Attach {
            block_id: 1,
            asset,
        })],
    );
    store
        .commit(StorageSaveBatch {
            document_id: 7,
            layout_key: None,
            payloads: vec![],
            index_records: vec![],
            structure_version: 1,
            transactions: vec![detach],
            block_attrs: vec![],
            page_layout_snapshot: None,
        })
        .await
        .unwrap();
    assert!(store.asset_references(40).await.unwrap().is_empty());
    assert!(store.asset_manifest(40).await.unwrap().is_some());
}
