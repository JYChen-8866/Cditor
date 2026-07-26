use cditor_core::edit::{ChangeOrigin, EditOperation, EditTransaction, EditTransactionKind};
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
use cditor_runtime::DocumentRuntime;
use cditor_session::{PersistencePipeline, prepare_editor_session_from_rebuild_plan};
use cditor_storage::{
    DocumentStorage, LoadDocumentRequest, MATERIALIZED_CHECKPOINT_FORMAT,
    MATERIALIZED_CHECKPOINT_VERSION, StorageError, StorageSaveBatch,
};
use cditor_storage_sqlite::{SqliteDocumentStorage, SqliteStorageOptions};
use tempfile::TempDir;

async fn open_store(dir: &TempDir) -> SqliteDocumentStorage {
    SqliteDocumentStorage::open(SqliteStorageOptions::file(dir.path().join("checkpoint.db")))
        .await
        .unwrap()
}

fn load_request(document_id: u64) -> LoadDocumentRequest {
    LoadDocumentRequest {
        document_id,
        workspace_id: 1,
        initial_payload_window_blocks: 16,
        visible_index_version: cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION,
        layout_key: cditor_storage::layout_cache::LayoutCacheKey {
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

fn insert_transaction(id: u64, offset: usize, text: &str) -> EditTransaction {
    EditTransaction::new(
        id,
        EditTransactionKind::ExplicitCommand,
        id + 1,
        vec![EditOperation::InsertText {
            block_id: 1,
            offset,
            text: text.to_owned(),
        }],
        vec![],
    )
    .with_origin(ChangeOrigin::User)
}

async fn commit_runtime(
    store: &SqliteDocumentStorage,
    runtime: &DocumentRuntime,
    transaction: EditTransaction,
) {
    store
        .commit(StorageSaveBatch {
            document_id: 7,
            layout_key: None,
            payloads: runtime.loaded_payload_records_snapshot(),
            index_records: runtime.index_records_snapshot(),
            structure_version: runtime.structure_version(),
            transactions: vec![transaction],
            block_attrs: vec![],
            page_layout_snapshot: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn checkpoint_and_following_operations_rebuild_materialized_session_state() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    store.load_document(load_request(7)).await.unwrap();

    let mut live = DocumentRuntime::from_payloads(
        7,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "",
        )],
        720.0,
    );
    let baseline = insert_transaction(1, 0, "base");
    live.apply_external_transaction(&baseline, ChangeOrigin::User)
        .unwrap();
    commit_runtime(&store, &live, baseline).await;

    let checkpoint = store.create_materialized_checkpoint(7).await.unwrap();
    assert_eq!(checkpoint.format, MATERIALIZED_CHECKPOINT_FORMAT);
    assert_eq!(checkpoint.version, MATERIALIZED_CHECKPOINT_VERSION);
    assert!(checkpoint.journal_sequence > 0);

    let following = insert_transaction(2, 4, " recovered");
    live.apply_external_transaction(&following, ChangeOrigin::User)
        .unwrap();
    commit_runtime(&store, &live, following).await;

    let plan = store
        .load_materialized_rebuild_plan(7)
        .await
        .unwrap()
        .expect("checkpoint rebuild plan");
    assert_eq!(plan.operations.len(), 1);
    assert_eq!(plan.operations[0].transaction_id, 2);
    let rebuilt = prepare_editor_session_from_rebuild_plan(
        plan,
        720.0,
        PersistencePipeline::disabled(),
    )
    .unwrap();
    assert_eq!(
        rebuilt
            .session
            .into_handle()
            .block_plain_text(1)
            .unwrap()
            .as_deref(),
        Some("base recovered")
    );
}

#[tokio::test]
async fn rebuild_plan_rejects_a_damaged_checkpoint_before_replay() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    store.load_document(load_request(7)).await.unwrap();
    store.create_materialized_checkpoint(7).await.unwrap();
    sqlx::query("UPDATE runtime_snapshots SET snapshot_json = snapshot_json || ' '")
        .execute(store.pool())
        .await
        .unwrap();

    let error = store.load_materialized_rebuild_plan(7).await.unwrap_err();
    assert!(matches!(error, StorageError::CorruptData(_)));
}

#[tokio::test]
async fn document_without_checkpoint_has_no_rebuild_plan() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    store.load_document(load_request(7)).await.unwrap();
    assert!(
        store
            .load_materialized_rebuild_plan(7)
            .await
            .unwrap()
            .is_none()
    );
}
