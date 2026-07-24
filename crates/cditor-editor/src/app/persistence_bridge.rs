use gpui::{AppContext, Context};

use cditor_runtime::SelectionMaterializationRequest;
use cditor_runtime::content::payload_window::{PayloadWindowLoadRequest, PayloadWindowLoadResult};
use cditor_session::{
    HistoryActionSnapshot, HistoryDirection, PayloadStorageRequest, UndoBlobWriteResult,
    execute_payload_load, execute_storage_flush, execute_undo_blob_read, run_undo_blob_delete,
    run_undo_blob_write,
};
use cditor_storage::{StorageError, block_on_storage};

use crate::app::cditor_v2_view::CditorV2View;
use crate::input::GuiInputCommand;
use crate::persistence::{
    EditorSaveStatus, PersistencePipelineError, STORAGE_VIEWPORT_LOAD_TIMEOUT, save_storage_batch,
    schedule_storage_autosave,
};
use cditor_api::CditorError;
use cditor_api::event::CditorEvent;
use cditor_core::edit::ChangeOrigin;
use cditor_editor_protocol::command::CommandSource;
#[cfg(test)]
use cditor_editor_protocol::command::{CommandEnvelope, EditorCommand};
#[cfg(test)]
use cditor_storage::StorageSession;

impl CditorV2View {
    pub(crate) fn schedule_selection_materialization(
        &mut self,
        command: GuiInputCommand,
        request: SelectionMaterializationRequest,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.selection_materialization_in_flight.is_some() {
            return true;
        }
        let Some(storage_request) = self
            .ready_session()
            .and_then(|session| session.payload_storage_request().ok().flatten())
        else {
            return false;
        };
        self.selection_materialization_in_flight = Some((request.clone(), command));
        let load_request = request.clone();
        let load_task = cx.background_spawn(async move {
            block_on_storage(async move {
                let loaded = tokio::time::timeout(
                    STORAGE_VIEWPORT_LOAD_TIMEOUT,
                    execute_payload_load(storage_request, &load_request.block_ids),
                )
                .await
                .map_err(|_| StorageError::Timeout {
                    operation: "selection payload materialization",
                    timeout: STORAGE_VIEWPORT_LOAD_TIMEOUT,
                })??;
                Ok::<_, StorageError>((loaded.records, loaded.missing_block_ids))
            })
            .and_then(|result| result.map_err(|error| error.to_string()))
        });
        cx.spawn(async move |view, cx| {
            let result = load_task.await;
            let _ = view.update(cx, |view, cx| {
                let still_current_operation = view
                    .selection_materialization_in_flight
                    .as_ref()
                    .is_some_and(|(pending, pending_command)| {
                        pending == &request && *pending_command == command
                    });
                if !still_current_operation {
                    return;
                }
                view.selection_materialization_in_flight = None;
                let should_replay = match result {
                    Ok((records, missing_block_ids)) if missing_block_ids.is_empty() => {
                        view.ready_session().is_some_and(|session| {
                            session
                                .apply_selection_materialization_result(
                                    &request,
                                    records,
                                    &missing_block_ids,
                                )
                                .is_ok_and(|snapshot| snapshot.replay_ready)
                        })
                    }
                    Ok((records, missing_block_ids)) => {
                        if let Some(session) = view.ready_session() {
                            let _ = session.apply_selection_materialization_result(
                                &request,
                                records,
                                &missing_block_ids,
                            );
                        }
                        false
                    }
                    Err(_) => false,
                };
                if should_replay {
                    view.apply_input_command(command, cx);
                }
                view.trim_persistent_payload_cache();
                cx.notify();
            });
        })
        .detach();
        true
    }

    pub(crate) fn execute_history_action(
        &mut self,
        source: CommandSource,
        redo: bool,
        cx: &mut Context<Self>,
    ) -> Result<bool, CditorError> {
        if self.readonly {
            return Err(CditorError::Readonly);
        }
        let direction = if redo {
            HistoryDirection::Redo
        } else {
            HistoryDirection::Undo
        };
        let result = self
            .ready_session()
            .ok_or(CditorError::NotReady)?
            .apply_history(source, direction);
        let origin = if redo {
            ChangeOrigin::Redo
        } else {
            ChangeOrigin::Undo
        };
        match result {
            Ok(HistoryActionSnapshot::Applied(snapshot)) => {
                let changed = snapshot.outcome.changed();
                if changed {
                    self.mark_dirty_at_revision(origin, snapshot.revision, cx);
                    cx.notify();
                }
                Ok(changed)
            }
            Ok(HistoryActionSnapshot::HydrationRequired { reference, .. }) => {
                self.schedule_history_hydration(reference, source, redo, cx)?;
                Ok(false)
            }
            Err(error) => Err(CditorError::Internal(error.message)),
        }
    }

    fn schedule_history_hydration(
        &mut self,
        reference: cditor_core::edit::ExternalUndoBlobRef,
        source: CommandSource,
        redo: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), CditorError> {
        let key = (reference.snapshot_id, redo);
        if self.history_hydration_in_flight == Some(key) {
            return Ok(());
        }
        if self.history_hydration_in_flight.is_some() {
            return Err(CditorError::Internal(
                "another undo or redo hydration is already running".to_owned(),
            ));
        }
        let storage_request = self
            .ready_session()
            .ok_or(CditorError::NotReady)?
            .undo_blob_read_request(reference.clone())
            .map_err(|error| CditorError::Internal(error.to_string()))?
            .ok_or_else(|| {
                CditorError::Unsupported(
                    "external undo hydration requires persistent storage".to_owned(),
                )
            })?;
        self.history_hydration_in_flight = Some(key);
        cx.emit(CditorEvent::HistoryHydrationStarted {
            snapshot_id: reference.snapshot_id,
            redo,
        });
        let hydrate_task = cx.background_spawn(async move {
            block_on_storage(execute_undo_blob_read(storage_request))
                .and_then(|result| result.map_err(|error| error.to_string()))
        });
        cx.spawn(async move |view, cx| {
            let result = hydrate_task.await;
            let _ = view.update(cx, |view, cx| {
                view.history_hydration_in_flight = None;
                let replay = match result {
                    Ok((reference, transaction)) => match view.ready_session() {
                        Some(session_handle) => {
                            let direction = if redo {
                                HistoryDirection::Redo
                            } else {
                                HistoryDirection::Undo
                            };
                            session_handle
                                .apply_hydrated_history(&reference, transaction, source, direction)
                                .map(|snapshot| (snapshot.outcome.changed(), snapshot.revision))
                                .map_err(|error| error.message)
                        }
                        None => Err("editor runtime is no longer ready".to_owned()),
                    },
                    Err(error) => Err(error),
                };
                match replay {
                    Ok((true, revision)) => {
                        let origin = if redo {
                            ChangeOrigin::Redo
                        } else {
                            ChangeOrigin::Undo
                        };
                        view.mark_dirty_at_revision(origin, revision, cx);
                        cx.emit(CditorEvent::HistoryHydrationSucceeded {
                            snapshot_id: key.0,
                            redo,
                        });
                    }
                    Ok((false, _)) => cx.emit(CditorEvent::HistoryHydrationFailed {
                        snapshot_id: reference.snapshot_id,
                        redo,
                        error: CditorError::Internal(
                            "hydrated history action did not apply".to_owned(),
                        ),
                    }),
                    Err(error) => cx.emit(CditorEvent::HistoryHydrationFailed {
                        snapshot_id: reference.snapshot_id,
                        redo,
                        error: CditorError::Persistence(error),
                    }),
                }
                cx.notify();
            });
        })
        .detach();
        Ok(())
    }

    pub(crate) fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        self.mark_dirty_with_origin(ChangeOrigin::User, cx);
    }

    pub(crate) fn mark_dirty_with_origin(&mut self, origin: ChangeOrigin, cx: &mut Context<Self>) {
        let revision = self
            .ready_session()
            .and_then(|session| session.record_content_changed().ok())
            .map(|snapshot| snapshot.revision)
            .unwrap_or_default();
        self.mark_dirty_at_revision(origin, revision, cx);
    }

    pub(crate) fn mark_dirty_at_revision(
        &mut self,
        origin: ChangeOrigin,
        revision: u64,
        cx: &mut Context<Self>,
    ) {
        let was_dirty = self.dirty;
        self.dirty = true;
        self.save_status = EditorSaveStatus::Dirty;
        if let Some(session) = self.ready_session() {
            let _ = session.mark_persistence_dirty();
        }
        schedule_storage_autosave(self, cx);
        self.schedule_external_undo_spill(cx);
        self.schedule_external_undo_cleanup(cx);
        cx.emit(CditorEvent::ContentChanged { revision, origin });
        if !was_dirty {
            cx.emit(CditorEvent::DirtyChanged { dirty: true });
        }
    }

    fn schedule_external_undo_spill(&mut self, cx: &mut Context<Self>) {
        if self.undo_spill_in_flight {
            return;
        }
        let Some(request) = self
            .ready_session()
            .and_then(|session| session.undo_blob_write_request().ok().flatten())
        else {
            return;
        };
        self.undo_spill_in_flight = true;
        let spill_task = cx.background_spawn(async move { run_undo_blob_write(request) });
        cx.spawn(async move |view, cx| {
            let (job, result) = spill_task.await;
            let _ = view.update(cx, |view, cx| {
                view.undo_spill_in_flight = false;
                if let Some(session_handle) = view.ready_session() {
                    let result =
                        result.map_or(UndoBlobWriteResult::Failed, UndoBlobWriteResult::Stored);
                    let _ = session_handle.apply_undo_blob_write_result(job, result);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn schedule_external_undo_cleanup(&mut self, cx: &mut Context<Self>) {
        if self.undo_cleanup_in_flight {
            return;
        }
        let Some(request) = self
            .ready_session()
            .and_then(|session| session.undo_blob_delete_request().ok().flatten())
        else {
            return;
        };
        self.undo_cleanup_in_flight = true;
        let cleanup_task = cx.background_spawn(async move { run_undo_blob_delete(request) });
        cx.spawn(async move |view, cx| {
            let failed = cleanup_task.await;
            let _ = view.update(cx, |view, cx| {
                view.undo_cleanup_in_flight = false;
                if let Some(session_handle) = view.ready_session() {
                    let _ = session_handle.finish_undo_blob_cleanup(failed);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn flush_storage_persistence(&mut self, cx: &mut Context<Self>) {
        if self.readonly {
            if let Some(session) = self.ready_session() {
                let _ = session.clear_scheduled_save();
            }
            return;
        }
        let Some(session) = self.ready_session().cloned() else {
            return;
        };
        let Ok(Some(batch)) = session.capture_storage_save() else {
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
                    let should_reschedule = view
                        .ready_session()
                        .and_then(|session| {
                            session.apply_storage_save_success(&request, &outcome).ok()
                        })
                        .is_some_and(|apply| apply.should_reschedule);
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
                        schedule_storage_autosave(view, cx);
                    }
                    view.settle_storage_barriers(cx);
                    cx.notify();
                });
            }
            (request, Err(message)) => {
                let _ = view.update(cx, |view, cx| {
                    let should_reschedule = view
                        .ready_session()
                        .and_then(|session| {
                            session.apply_storage_save_failure(&request, &message).ok()
                        })
                        .unwrap_or(false);
                    view.dirty = true;
                    view.save_status = EditorSaveStatus::Failed(message.clone());
                    cx.emit(CditorEvent::SaveFailed {
                        revision,
                        error: CditorError::Persistence(message),
                    });
                    if should_reschedule {
                        schedule_storage_autosave(view, cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn settle_storage_barriers(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.ready_session().cloned() else {
            return;
        };
        let Ok((save_barriers, flush_barriers)) = session.drain_ready_persistence_barriers() else {
            return;
        };
        for barrier in save_barriers {
            barrier.resolve(Ok(()));
        }
        if flush_barriers.is_empty() {
            return;
        }
        let Ok(Some(flush_request)) = session.begin_storage_flush() else {
            let error = PersistencePipelineError::Unavailable(
                "save and flush require a persistent storage backend".to_owned(),
            );
            for barrier in flush_barriers {
                barrier.resolve(Err(error.clone()));
            }
            return;
        };

        let flush_task = cx.background_spawn(async move {
            block_on_storage(execute_storage_flush(flush_request)).and_then(|result| result)
        });
        cx.spawn(async move |view, cx| {
            let result = flush_task.await.map_err(PersistencePipelineError::Storage);
            let state_result = result.clone();
            let _ = view.update(cx, |view, cx| {
                if let Some(session) = view.ready_session() {
                    let _ = session.finish_storage_flush();
                }
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
        storage_request: PayloadStorageRequest,
        request: PayloadWindowLoadRequest,
        cx: &mut Context<Self>,
    ) {
        let failed_request = request.clone();
        let load_task = cx.background_spawn(async move {
            block_on_storage(async move {
                let loaded = tokio::time::timeout(
                    STORAGE_VIEWPORT_LOAD_TIMEOUT,
                    execute_payload_load(storage_request, &request.block_ids),
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
                    if let Some(session) = view.ready_session() {
                        let _ = session.apply_payload_window_result(result);
                    }
                    view.trim_persistent_payload_cache();
                    cx.notify();
                });
            }
            Err(message) => {
                let _ = view.update(cx, |view, cx| {
                    if let Some(session) = view.ready_session() {
                        let _ = session.apply_payload_window_error(failed_request, message);
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

pub(in crate::app) fn save_status_for_mode(readonly: bool) -> EditorSaveStatus {
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
            StorageCapabilities {
                emergency_log: false,
                ..StorageCapabilities::SQLITE
            }
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
}
