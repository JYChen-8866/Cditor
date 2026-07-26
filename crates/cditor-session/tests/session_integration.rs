use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use cditor_core::{
    document::BlockIndexRecord,
    rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind},
};
use cditor_editor_protocol::{
    ProtocolErrorCode,
    command::{CommandEnvelope, CommandSource, EditorCommand},
    query::{CommandQuery, QueryResult},
};
use cditor_runtime::document_runtime::{DocumentRuntimeColdStartData, DocumentRuntimeIndexSource};
use cditor_session::{
    PersistencePipeline, SessionColdStartRequest, open_editor_session,
    prepare_editor_session_with_persistence, save_storage_batch,
};
use cditor_storage::{
    DocumentStorage, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch, StorageBackendKind,
    StorageCapabilities, StorageError, StorageResult, StorageSaveBatch, StorageSaveOutcome,
};

fn cold_start_request(readonly: bool) -> SessionColdStartRequest {
    SessionColdStartRequest {
        data: DocumentRuntimeColdStartData {
            document_id: 91,
            document_title: "Headless integration".to_owned(),
            structure_version: 1,
            records: vec![BlockIndexRecord::new(
                1,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )],
            block_attrs: Vec::new(),
            initial_payloads: vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "first",
            )],
            initial_payload_window_end: 1,
            index_source: DocumentRuntimeIndexSource::Blocks,
            layout_cache_hits: 0,
        },
        viewport_height: 720.0,
        cached_page_layout: None,
        readonly,
        emergency_log_entries: Vec::new(),
        emergency_payloads: Vec::new(),
    }
}

fn command(command: EditorCommand) -> CommandEnvelope {
    CommandEnvelope::new(command, CommandSource::Automation)
}

#[test]
fn prepared_session_opens_into_one_headless_owner_with_typed_policy() {
    let prepared = prepare_editor_session_with_persistence(
        cold_start_request(false),
        PersistencePipeline::disabled(),
    )
    .unwrap();
    let session = prepared.into_opened().session;
    let clone = session.clone();
    let before = session.snapshot().unwrap();

    session
        .dispatch(command(EditorCommand::FocusBlock { block_id: 1 }))
        .unwrap();
    clone
        .dispatch(command(EditorCommand::InsertParagraphAfterFocused))
        .unwrap();

    let after = session.snapshot().unwrap();
    assert_eq!(session.id(), clone.id());
    assert_eq!(before.session_id, after.session_id);
    assert_eq!(after.block_count, 2);
    assert!(after.revision > before.revision);

    session.set_readonly(true).unwrap();
    let error = clone
        .dispatch(command(EditorCommand::InsertParagraphAfterFocused))
        .unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::Readonly);
    let QueryResult::DocumentSummary(summary) =
        session.query(CommandQuery::DocumentSummary).unwrap()
    else {
        panic!("expected document summary")
    };
    assert!(summary.readonly);
}

#[derive(Debug, Default)]
struct ProbeStorage {
    fail_commit: AtomicBool,
    committed_batches: Mutex<Vec<StorageSaveBatch>>,
}

#[async_trait]
impl DocumentStorage for ProbeStorage {
    fn backend_kind(&self) -> StorageBackendKind {
        StorageBackendKind::Custom
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::POSTGRES
    }

    async fn load_document(&self, _request: LoadDocumentRequest) -> StorageResult<LoadedDocument> {
        Err(StorageError::InvalidConfiguration(
            "integration probe does not load documents".to_owned(),
        ))
    }

    async fn load_payloads(
        &self,
        _document_id: u64,
        _block_ids: &[u64],
    ) -> StorageResult<LoadedPayloadBatch> {
        Err(StorageError::InvalidConfiguration(
            "integration probe does not load payloads".to_owned(),
        ))
    }

    async fn commit(&self, batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
        if self.fail_commit.load(Ordering::SeqCst) {
            return Err(StorageError::Backend {
                backend: StorageBackendKind::Custom,
                message: "injected commit failure".to_owned(),
            });
        }
        let outcome = StorageSaveOutcome {
            saved_structure_version: batch.saved_structure_version(),
            saved_payload_versions: batch
                .payloads
                .iter()
                .map(|payload| (payload.block_id, payload.content_version))
                .collect(),
        };
        self.committed_batches.lock().unwrap().push(batch);
        Ok(outcome)
    }
}

#[tokio::test(flavor = "current_thread")]
async fn persistence_failure_restores_transaction_and_retry_commits() {
    let storage = Arc::new(ProbeStorage::default());
    storage.fail_commit.store(true, Ordering::SeqCst);
    let pipeline = PersistencePipeline::for_session(
        cditor_session::DocumentPersistence::new(storage.clone(), 91),
        None,
    );
    let prepared =
        prepare_editor_session_with_persistence(cold_start_request(false), pipeline).unwrap();
    let session = prepared.into_opened().session;

    session
        .dispatch(command(EditorCommand::FocusBlock { block_id: 1 }))
        .unwrap();
    session
        .dispatch(command(EditorCommand::InsertParagraphAfterFocused))
        .unwrap();
    session.mark_persistence_dirty().unwrap();

    let failed_request = session
        .capture_storage_save()
        .unwrap()
        .expect("dirty session must produce a save request");
    let error = save_storage_batch(&failed_request).await.unwrap_err();
    assert!(error.to_string().contains("injected commit failure"));
    session
        .apply_storage_save_failure(&failed_request, &error)
        .unwrap();

    storage.fail_commit.store(false, Ordering::SeqCst);
    let retry = session
        .capture_storage_save()
        .unwrap()
        .expect("failed transaction must be restored for retry");
    assert_eq!(
        retry.transactions().len(),
        failed_request.transactions().len()
    );
    let outcome = save_storage_batch(&retry).await.unwrap();
    session
        .apply_storage_save_success(&retry, &outcome)
        .unwrap();

    assert_eq!(storage.committed_batches.lock().unwrap().len(), 1);
    assert!(
        !session
            .persistence_snapshot()
            .unwrap()
            .has_unpersisted_changes
    );
    assert!(session.capture_storage_save().unwrap().is_none());
}

#[test]
fn readonly_cold_start_is_enforced_without_a_ui_host() {
    let session = open_editor_session(cold_start_request(true))
        .unwrap()
        .session;
    let error = session
        .dispatch(command(EditorCommand::InsertParagraphAfterFocused))
        .unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::Readonly);
}
