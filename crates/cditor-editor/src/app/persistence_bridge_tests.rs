use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
use cditor_storage::{
    DocumentStorage, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch, StorageBackendKind,
    StorageCapabilities, StorageResult, StorageSaveBatch, StorageSaveOutcome,
};
use gpui::{AppContext, TestAppContext};

use super::*;

#[derive(Debug, Default)]
struct FailFirstStorage {
    attempts: AtomicUsize,
    transaction_counts: Mutex<Vec<usize>>,
}

#[async_trait]
impl DocumentStorage for FailFirstStorage {
    fn backend_kind(&self) -> StorageBackendKind {
        StorageBackendKind::Custom
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities {
            emergency_log: false,
            ..StorageCapabilities::SQLITE
        }
    }

    async fn load_document(&self, _request: LoadDocumentRequest) -> StorageResult<LoadedDocument> {
        unreachable!("the persistence test starts from an in-memory runtime")
    }

    async fn load_payloads(
        &self,
        _document_id: cditor_core::ids::DocumentId,
        _block_ids: &[cditor_core::ids::BlockId],
    ) -> StorageResult<LoadedPayloadBatch> {
        unreachable!("the persistence test does not load payload windows")
    }

    async fn commit(&self, batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
        self.transaction_counts
            .lock()
            .unwrap()
            .push(batch.transactions.len());
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(cditor_storage::StorageError::Backend {
                backend: StorageBackendKind::Custom,
                message: "injected first-save failure".to_owned(),
            });
        }
        Ok(StorageSaveOutcome {
            saved_structure_version: batch.saved_structure_version(),
            saved_payload_versions: batch
                .payloads
                .iter()
                .map(|payload| (payload.block_id, payload.content_version))
                .collect(),
        })
    }
}

#[gpui::test]
fn failed_save_restores_transactions_and_explicit_retry_cleans_document(cx: &mut TestAppContext) {
    let storage = Arc::new(FailFirstStorage::default());
    let mut runtime = cditor_runtime::DocumentRuntime::from_payloads(
        1,
        (1..=3)
            .map(|block_id| {
                BlockPayloadRecord::rich_text(
                    block_id,
                    RichBlockKind::Paragraph,
                    block_id.to_string(),
                )
            })
            .collect(),
        720.0,
    );
    assert!(
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::MoveBlockBefore {
                    block_id: 1,
                    before_block_id: Some(3),
                },
                CommandSource::Sdk,
            ))
            .unwrap()
            .changed()
    );
    let session = StorageSession::new(storage.clone(), 1);
    let view = cx.new(|cx| {
        CditorV2View::from_runtime_with_persistence_options(
            runtime,
            false,
            false,
            Some(cditor_session::PersistencePipeline::for_session(
                session,
                Some(std::time::Duration::ZERO),
            )),
            cx,
        )
    });

    view.update(cx, |view, cx| {
        view.mark_dirty(cx);
        view.flush_storage_persistence(cx);
    });
    cx.run_until_parked();
    assert!(matches!(
        view.read_with(cx, |view, _| view.sdk_save_status()),
        cditor_api::document::SaveStatus::Failed(message) if message.contains("injected")
    ));
    assert_eq!(
        view.read_with(cx, |view, _| {
            view.ready_session()
                .unwrap()
                .persistence_runtime_snapshot()
                .unwrap()
                .pending_structure_transactions
        }),
        1
    );

    let retry = view.update(cx, |view, cx| view.sdk_save(cx));
    let report = cx.foreground_executor().block_test(retry).unwrap();
    assert_eq!(report.saved_blocks, 3);
    assert_eq!(
        view.read_with(cx, |view, _| view.sdk_save_status()),
        cditor_api::document::SaveStatus::Clean
    );
    assert_eq!(*storage.transaction_counts.lock().unwrap(), vec![1, 1]);
}
