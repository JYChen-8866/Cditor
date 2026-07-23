use gpui::{AppContext, Context};

use cditor_runtime::SelectionMaterializationRequest;
use cditor_runtime::content::payload_window::{PayloadWindowLoadRequest, PayloadWindowLoadResult};
use cditor_session::{
    HistoryActionSnapshot, HistoryDirection, UndoBlobWriteResult, apply_payload_window_error,
    apply_payload_window_result, project_apply_undo_blob_write_result,
    project_begin_undo_blob_cleanup, project_begin_undo_blob_spill,
    project_finish_undo_blob_cleanup, project_history_action, project_hydrated_history_action,
    project_persistence_save_failure, project_persistence_save_success,
    project_selection_materialization_result,
};
use cditor_storage::{StorageError, StorageSession, block_on_storage};

use crate::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::input::GuiInputCommand;
use crate::persistence::{
    EditorSaveStatus, STORAGE_VIEWPORT_LOAD_TIMEOUT, mark_dirty_and_schedule_save,
    save_storage_batch,
};
use cditor_api::CditorError;
use cditor_api::event::CditorEvent;
use cditor_core::edit::ChangeOrigin;
use cditor_editor_protocol::command::CommandSource;
#[cfg(test)]
use cditor_editor_protocol::command::{CommandEnvelope, EditorCommand};

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
        let Some(session) = self.storage_persistence.session().cloned() else {
            return false;
        };
        self.selection_materialization_in_flight = Some((request.clone(), command));
        let load_request = request.clone();
        let load_task = cx.background_spawn(async move {
            block_on_storage(async move {
                let loaded = tokio::time::timeout(
                    STORAGE_VIEWPORT_LOAD_TIMEOUT,
                    session.load_payloads(&load_request.block_ids),
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
                        view.ready_runtime().is_some_and(|runtime| {
                            project_selection_materialization_result(
                                runtime,
                                &request,
                                records,
                                &missing_block_ids,
                            )
                            .replay_ready
                        })
                    }
                    Ok((records, missing_block_ids)) => {
                        if let Some(runtime) = view.ready_runtime() {
                            project_selection_materialization_result(
                                runtime,
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
        let result = project_history_action(
            self.ready_runtime().ok_or(CditorError::NotReady)?,
            source,
            direction,
        );
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
        let session = self.storage_persistence.session().cloned().ok_or_else(|| {
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
            let result = block_on_storage(session.load_undo_blob(&reference))
                .and_then(|result| result.map_err(|error| error.to_string()));
            (reference, result)
        });
        cx.spawn(async move |view, cx| {
            let (reference, result) = hydrate_task.await;
            let _ = view.update(cx, |view, cx| {
                view.history_hydration_in_flight = None;
                let replay = match result {
                    Ok(transaction) => match view.ready_runtime() {
                        Some(runtime) => {
                            let direction = if redo {
                                HistoryDirection::Redo
                            } else {
                                HistoryDirection::Undo
                            };
                            project_hydrated_history_action(
                                runtime,
                                &reference,
                                transaction,
                                source,
                                direction,
                            )
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
                            snapshot_id: reference.snapshot_id,
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
            .ready_runtime()
            .map(|runtime| runtime.note_content_changed())
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
        mark_dirty_and_schedule_save(&mut self.storage_persistence, &mut self.save_status, cx);
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
        let Some(session) = self.storage_persistence.session().cloned() else {
            return;
        };
        let Some(job) = self.ready_runtime().and_then(project_begin_undo_blob_spill) else {
            return;
        };
        self.undo_spill_in_flight = true;
        let spill_task = cx.background_spawn(async move {
            let result = block_on_storage(session.write_undo_blob(
                job.snapshot_id,
                job.block_count,
                &job.transaction,
            ))
            .and_then(|result| result.map_err(|error| error.to_string()));
            (job, result)
        });
        cx.spawn(async move |view, cx| {
            let (job, result) = spill_task.await;
            let _ = view.update(cx, |view, cx| {
                view.undo_spill_in_flight = false;
                if let Some(runtime) = view.ready_runtime() {
                    let result =
                        result.map_or(UndoBlobWriteResult::Failed, UndoBlobWriteResult::Stored);
                    project_apply_undo_blob_write_result(runtime, job, result);
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
        let Some(session) = self.storage_persistence.session().cloned() else {
            return;
        };
        let references = self
            .ready_runtime()
            .map(project_begin_undo_blob_cleanup)
            .unwrap_or_default();
        if references.is_empty() {
            return;
        }
        self.undo_cleanup_in_flight = true;
        let cleanup_task = cx.background_spawn(async move {
            let mut failed = Vec::new();
            for reference in references {
                let result = block_on_storage(session.delete_undo_blob(reference.snapshot_id))
                    .and_then(|result| result.map_err(|error| error.to_string()));
                if result.is_err() {
                    failed.push(reference);
                }
            }
            failed
        });
        cx.spawn(async move |view, cx| {
            let failed = cleanup_task.await;
            let _ = view.update(cx, |view, cx| {
                view.undo_cleanup_in_flight = false;
                if let Some(runtime) = view.ready_runtime() {
                    project_finish_undo_blob_cleanup(runtime, failed);
                }
                cx.notify();
            });
        })
        .detach();
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
                        project_persistence_save_success(
                            runtime,
                            &outcome,
                            saved_layout_or_structure,
                            should_reschedule,
                        );
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
                        project_persistence_save_failure(runtime, request.transactions().to_vec());
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
            block_on_storage(async move {
                session.flush().await?;
                session.prune_undo_blobs(100).await?;
                Ok::<(), StorageError>(())
            })
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
                        apply_payload_window_result(runtime, result);
                    }
                    view.trim_persistent_payload_cache();
                    cx.notify();
                });
            }
            Err(message) => {
                let _ = view.update(cx, |view, cx| {
                    if let Some(runtime) = view.ready_runtime() {
                        apply_payload_window_error(runtime, failed_request, message);
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
            cditor_api::document::SaveStatus::Failed(message) if message.contains("injected")
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
            cditor_api::document::SaveStatus::Clean
        );
        assert_eq!(*storage.transaction_counts.lock().unwrap(), vec![1, 1]);
    }
}
