use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::{
    EditOperation, EditTransaction, EditTransactionKind, UndoGroupingPolicy, UndoPayload, UndoStack,
};
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
use cditor_storage::{StorageError, StorageSession};
use cditor_storage_sqlite::{SqliteDocumentStorage, SqliteStorageOptions};
use std::sync::Arc;
use tempfile::TempDir;

async fn open_store(dir: &TempDir) -> SqliteDocumentStorage {
    SqliteDocumentStorage::open(SqliteStorageOptions::file(dir.path().join("undo.db")))
        .await
        .expect("open sqlite store")
}

fn large_transaction(id: u64, block_count: usize) -> EditTransaction {
    EditTransaction::paste_blocks(
        id,
        id,
        0,
        (0..block_count)
            .map(|index| BlockIndexRecord::new(index as u64 + 1, None, 0, 1, 0))
            .collect(),
    )
}

fn large_payload_transaction(id: u64, block_count: usize) -> EditTransaction {
    let text = "x".repeat(2_048);
    let blocks = (0..block_count)
        .map(|index| BlockIndexRecord::new(index as u64 + 1, None, 0, 1, 0))
        .collect::<Vec<_>>();
    let payloads = blocks
        .iter()
        .map(|block| {
            BlockPayloadRecord::rich_text(
                block.id,
                RichBlockKind::Paragraph,
                format!("{}:{text}", block.id),
            )
        })
        .collect();
    EditTransaction::new(
        id,
        EditTransactionKind::Paste,
        id,
        vec![EditOperation::InsertBlocks {
            index: 0,
            blocks,
            payloads,
        }],
        vec![EditOperation::DeleteBlockRange {
            range: 0..block_count,
        }],
    )
}

#[tokio::test]
async fn pending_range_snapshot_spills_roundtrips_and_hydrates_without_data_loss() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let transaction = large_transaction(77, 32);
    let mut undo = UndoStack::new(UndoGroupingPolicy {
        typing_merge_window_ms: 1_000,
        inline_block_snapshot_limit: 4,
    });
    undo.record_transaction(transaction.clone());
    let reference = store
        .spill_next_undo_snapshot(7, &mut undo)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reference.block_count, 32);
    assert!(reference.encoded_len > 0);
    assert_eq!(reference.checksum.len(), 64);
    assert!(matches!(
        &undo.last_undo_step().unwrap().payload,
        UndoPayload::ExternalBlob(current) if current == &reference
    ));

    assert!(
        store
            .hydrate_undo_snapshot(7, &mut undo, &reference)
            .await
            .unwrap()
    );
    assert_eq!(
        undo.pending_externalization().unwrap().transaction,
        &transaction
    );
}

#[tokio::test]
async fn checksum_mismatch_rejects_corrupt_blob_before_decode() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let transaction = large_transaction(88, 8);
    let reference = store.write_undo_blob(9, 1, 8, &transaction).await.unwrap();
    sqlx::query("UPDATE undo_blobs SET payload = X'00010203' WHERE snapshot_id = 1")
        .execute(store.pool())
        .await
        .unwrap();

    let error = store
        .load_undo_blob(9, &reference)
        .await
        .expect_err("tampered blob must be rejected");
    assert!(matches!(error, StorageError::CorruptData(_)));
}

#[tokio::test]
async fn failed_spill_restores_pending_snapshot_for_retry() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let transaction = large_transaction(90, 8);
    let mut undo = UndoStack::new(UndoGroupingPolicy {
        typing_merge_window_ms: 1_000,
        inline_block_snapshot_limit: 1,
    });
    undo.record_transaction(transaction.clone());
    store.pool().close().await;

    assert!(store.spill_next_undo_snapshot(9, &mut undo).await.is_err());
    assert_eq!(
        undo.pending_externalization().unwrap().transaction,
        &transaction
    );
}

#[tokio::test]
async fn pruning_and_explicit_delete_bound_blob_lifetime_per_document() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let mut references = Vec::new();
    for snapshot_id in 1..=3 {
        references.push(
            store
                .write_undo_blob(11, snapshot_id, 2, &large_transaction(100 + snapshot_id, 2))
                .await
                .unwrap(),
        );
    }
    let other_document = store
        .write_undo_blob(12, 1, 2, &large_transaction(200, 2))
        .await
        .unwrap();

    assert_eq!(store.prune_undo_blobs(11, 1).await.unwrap(), 2);
    assert!(matches!(
        store.load_undo_blob(11, &references[0]).await,
        Err(StorageError::NotFound { .. })
    ));
    assert!(store.load_undo_blob(11, &references[2]).await.is_ok());
    assert!(store.load_undo_blob(12, &other_document).await.is_ok());
    assert!(store.delete_undo_blob(11, 3).await.unwrap());
    assert!(!store.delete_undo_blob(11, 3).await.unwrap());
}

#[tokio::test]
async fn storage_session_dispatches_external_undo_blob_roundtrip() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(open_store(&dir).await);
    let session = StorageSession::new(store, 19);
    let transaction = large_transaction(301, 6);

    let reference = session.write_undo_blob(4, 6, &transaction).await.unwrap();
    assert_eq!(reference.snapshot_id, 4);
    assert_eq!(
        session.load_undo_blob(&reference).await.unwrap(),
        transaction
    );
    assert!(session.delete_undo_blob(4).await.unwrap());
    assert!(matches!(
        session.load_undo_blob(&reference).await,
        Err(StorageError::NotFound { .. })
    ));
}

#[tokio::test]
async fn multi_megabyte_paste_shares_memory_and_spills_within_budget() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let transaction = large_payload_transaction(401, 2_000);
    let mut undo = UndoStack::new(UndoGroupingPolicy {
        typing_merge_window_ms: 1_000,
        inline_block_snapshot_limit: 16,
    });
    undo.record_transaction(transaction.clone());
    let candidate = undo.pending_externalization().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &candidate.transaction.ops,
        &transaction.ops
    ));
    assert_eq!(std::sync::Arc::strong_count(&transaction.ops), 2);

    let started = std::time::Instant::now();
    let reference = store
        .spill_next_undo_snapshot(21, &mut undo)
        .await
        .unwrap()
        .unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert!(reference.encoded_len > 4_000_000);
    assert!(undo.pending_externalization().is_none());

    let hydrate_started = std::time::Instant::now();
    assert!(
        store
            .hydrate_undo_snapshot(21, &mut undo, &reference)
            .await
            .unwrap()
    );
    assert!(hydrate_started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(
        undo.pending_externalization().unwrap().transaction,
        &transaction
    );
}
