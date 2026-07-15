use gpui::{AppContext, Context};

use cditor_runtime::content::payload_window::{PayloadWindowLoadRequest, PayloadWindowLoadResult};
use cditor_storage::{StorageError, StorageSession, block_on_storage};

use crate::api::{CditorError, CditorEvent, ChangeOrigin};
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::persistence::{
    EditorSaveStatus, STORAGE_VIEWPORT_LOAD_TIMEOUT, mark_dirty_and_schedule_save,
    save_storage_batch,
};

impl CditorV2View {
    pub(crate) fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        self.mark_dirty_with_origin(ChangeOrigin::Local, cx);
    }

    pub(crate) fn mark_dirty_with_origin(&mut self, origin: ChangeOrigin, cx: &mut Context<Self>) {
        let was_dirty = self.dirty;
        self.dirty = true;
        let revision = self
            .ready_runtime()
            .map(|runtime| runtime.note_content_changed())
            .unwrap_or_default();
        mark_dirty_and_schedule_save(&mut self.storage_persistence, &mut self.save_status, cx);
        cx.emit(CditorEvent::ContentChanged { revision, origin });
        if !was_dirty {
            cx.emit(CditorEvent::DirtyChanged { dirty: true });
        }
    }

    pub(crate) fn flush_storage_persistence(&mut self, cx: &mut Context<Self>) {
        if self.readonly {
            self.storage_persistence.clear_scheduled_save();
            return;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        let Some(batch) = self.storage_persistence.begin_batch(runtime) else {
            self.settle_storage_barriers(cx);
            return;
        };
        let revision = batch.revision();
        self.save_status = EditorSaveStatus::Saving;
        cx.emit(CditorEvent::SaveStarted { revision });
        let save_task = cx.background_spawn(async move {
            let result = block_on_storage(save_storage_batch(&batch)).and_then(|result| result);
            (batch, result)
        });
        cx.spawn(async move |view, cx| match save_task.await {
            (request, Ok(outcome)) => {
                let _ = view.update(cx, |view, cx| {
                    let saved_layout_or_structure = outcome.saved_structure_version.is_some();
                    let should_reschedule = view
                        .storage_persistence
                        .finish_success(&request, outcome.saved_structure_version);
                    if let Some(runtime) = view.ready_runtime() {
                        runtime.mark_payload_versions_persisted(&outcome.saved_payload_versions);
                    }
                    if saved_layout_or_structure
                        && !should_reschedule
                        && let Some(runtime) = view.ready_runtime()
                    {
                        runtime.mark_layout_saved();
                    }
                    view.trim_persistent_payload_cache();
                    let became_clean = view.dirty && !should_reschedule;
                    view.dirty = should_reschedule;
                    view.save_status = if view.readonly {
                        EditorSaveStatus::Readonly
                    } else if should_reschedule {
                        EditorSaveStatus::Dirty
                    } else {
                        EditorSaveStatus::Clean
                    };
                    cx.emit(CditorEvent::SaveSucceeded { revision });
                    if became_clean {
                        cx.emit(CditorEvent::DirtyChanged { dirty: false });
                    }
                    if should_reschedule {
                        view.storage_persistence.schedule(cx);
                    }
                    view.settle_storage_barriers(cx);
                    cx.notify();
                });
            }
            (request, Err(message)) => {
                let _ = view.update(cx, |view, cx| {
                    if let Some(runtime) = view.ready_runtime() {
                        runtime.restore_pending_structure_transactions(
                            request.transactions().to_vec(),
                        );
                    }
                    let should_reschedule = view.storage_persistence.finish_failed(&request);
                    view.storage_persistence.fail_barriers(&message);
                    view.dirty = true;
                    view.save_status = EditorSaveStatus::Failed(message.clone());
                    cx.emit(CditorEvent::SaveFailed {
                        revision,
                        error: CditorError::Persistence(message),
                    });
                    if should_reschedule {
                        view.storage_persistence.schedule(cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn settle_storage_barriers(&mut self, cx: &mut Context<Self>) {
        let (save_barriers, flush_barriers) = self.storage_persistence.drain_ready_barriers();
        for barrier in save_barriers {
            barrier.resolve(Ok(()));
        }
        if flush_barriers.is_empty() {
            return;
        }
        let Some(session) = self.storage_persistence.session().cloned() else {
            let error = CditorError::Unsupported(
                "save and flush require a persistent storage backend".to_owned(),
            );
            for barrier in flush_barriers {
                barrier.resolve(Err(error.clone()));
            }
            return;
        };

        self.storage_persistence.begin_backend_flush();
        let flush_task = cx.background_spawn(async move {
            block_on_storage(session.flush())
                .and_then(|result| result.map_err(|error| error.to_string()))
        });
        cx.spawn(async move |view, cx| {
            let result = flush_task.await.map_err(CditorError::Persistence);
            let state_result = result.clone();
            let _ = view.update(cx, |view, cx| {
                view.storage_persistence.finish_backend_flush();
                if let Err(error) = &state_result {
                    view.save_status = EditorSaveStatus::Failed(error.to_string());
                } else if !view.dirty && !view.readonly {
                    view.save_status = EditorSaveStatus::Clean;
                }
                view.settle_storage_barriers(cx);
                cx.notify();
            });
            for barrier in flush_barriers {
                barrier.resolve(result.clone());
            }
        })
        .detach();
    }

    pub(crate) fn load_storage_payload_window(
        &mut self,
        session: StorageSession,
        request: PayloadWindowLoadRequest,
        cx: &mut Context<Self>,
    ) {
        let failed_request = request.clone();
        let load_task = cx.background_spawn(async move {
            block_on_storage(async move {
                let loaded = tokio::time::timeout(
                    STORAGE_VIEWPORT_LOAD_TIMEOUT,
                    session.load_payloads(&request.block_ids),
                )
                .await
                .map_err(|_| StorageError::Timeout {
                    operation: "storage viewport payload load",
                    timeout: STORAGE_VIEWPORT_LOAD_TIMEOUT,
                })??;
                Ok::<_, StorageError>(PayloadWindowLoadResult {
                    request,
                    records: loaded.records,
                    missing_block_ids: loaded.missing_block_ids,
                })
            })
            .and_then(|result| result.map_err(|error| error.to_string()))
        });
        cx.spawn(async move |view, cx| match load_task.await {
            Ok(result) => {
                let _ = view.update(cx, |view, cx| {
                    if let Some(runtime) = view.ready_runtime() {
                        runtime.apply_payload_window_result(result);
                    }
                    view.trim_persistent_payload_cache();
                    cx.notify();
                });
            }
            Err(message) => {
                let _ = view.update(cx, |view, cx| {
                    if let Some(runtime) = view.ready_runtime() {
                        runtime.apply_payload_window_load_error(failed_request, message);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn schedule_storage_payload_window_wake(
        &mut self,
        delay: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        let wake = cx.background_executor().timer(delay);
        cx.spawn(async move |view, cx| {
            wake.await;
            let _ = view.update(cx, |view, cx| {
                view.payload_window_load_scheduler.wake();
                cx.notify();
            });
        })
        .detach();
    }
}

pub(in crate::gui::app) fn save_status_for_mode(readonly: bool) -> EditorSaveStatus {
    if readonly {
        EditorSaveStatus::Readonly
    } else {
        EditorSaveStatus::Clean
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_storage::{
        DocumentStorage, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch,
        StorageBackendKind, StorageCapabilities, StorageResult, StorageSaveBatch,
        StorageSaveOutcome,
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
            StorageCapabilities::SQLITE
        }

        async fn load_document(
            &self,
            _request: LoadDocumentRequest,
        ) -> StorageResult<LoadedDocument> {
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
    fn failed_save_restores_transactions_and_explicit_retry_cleans_document(
        cx: &mut TestAppContext,
    ) {
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
        assert!(runtime.move_block_subtree_before(1, Some(3)).unwrap());
        let session = StorageSession::new(storage.clone(), 1);
        let view = cx.new(|cx| {
            CditorV2View::from_runtime_with_storage_options(
                runtime,
                false,
                false,
                Some(session),
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
            crate::api::SaveStatus::Failed(message) if message.contains("injected")
        ));
        assert_eq!(
            view.read_with(cx, |view, _| {
                view.ready_runtime_ref()
                    .unwrap()
                    .pending_structure_transaction_count()
            }),
            1
        );

        let retry = view.update(cx, |view, cx| view.sdk_save(cx));
        let report = cx.foreground_executor().block_test(retry).unwrap();
        assert_eq!(report.saved_blocks, 3);
        assert_eq!(
            view.read_with(cx, |view, _| view.sdk_save_status()),
            crate::api::SaveStatus::Clean
        );
        assert_eq!(*storage.transaction_counts.lock().unwrap(), vec![1, 1]);
    }
}
