use gpui::{AppContext, Context};

use cditor_runtime::SelectionMaterializationRequest;
use cditor_runtime::content::payload_window::{PayloadWindowLoadRequest, PayloadWindowLoadResult};
use cditor_session::{
    HistoryActionSnapshot, HistoryDirection, PayloadStorageRequest, SessionTaskAdmission,
    SessionTaskKind, SessionTaskToken, UndoBlobWriteResult, execute_payload_load,
    execute_storage_flush, execute_undo_blob_read, run_undo_blob_delete, run_undo_blob_write,
};
use cditor_storage::{StorageError, block_on_storage};

use crate::editor_view::CditorV2View;
use crate::input::GuiInputCommand;
use crate::persistence::{
    EditorSaveStatus, PersistencePipelineError, save_storage_batch, schedule_storage_autosave,
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
        let Some(session) = self.ready_session().cloned() else {
            return false;
        };
        let Some(storage_request) = session.payload_storage_request().ok().flatten() else {
            return false;
        };
        let task_key = request.document_id
            ^ request.structure_version.rotate_left(13)
            ^ request.payload_window_generation.rotate_left(29);
        let token = match session
            .begin_session_task(SessionTaskKind::SelectionMaterialization, task_key)
            .ok()
        {
            Some(SessionTaskAdmission::Started(token)) => token,
            Some(SessionTaskAdmission::Duplicate | SessionTaskAdmission::Busy) => return true,
            None => return false,
        };
        let load_request = request.clone();
        let load_task = cx.background_spawn(async move {
            block_on_storage(async move {
                let loaded = tokio::time::timeout(
                    token.timeout(),
                    execute_payload_load(storage_request, &load_request.block_ids),
                )
                .await
                .map_err(|_| StorageError::Timeout {
                    operation: "selection payload materialization",
                    timeout: token.timeout(),
                })??;
                Ok::<_, StorageError>((loaded.records, loaded.missing_block_ids))
            })
            .and_then(|result| result.map_err(|error| error.to_string()))
        });
        cx.spawn(async move |view, cx| {
            let result = load_task.await;
            let _ = view.update(cx, |view, cx| {
                let Some(session) = view.ready_session().cloned() else {
                    return;
                };
                if !session.complete_session_task(token).unwrap_or(false) {
                    return;
                }
                let should_replay = match result {
                    Ok((records, missing_block_ids)) if missing_block_ids.is_empty() => session
                        .apply_selection_materialization_result(
                            &request,
                            records,
                            &missing_block_ids,
                        )
                        .is_ok_and(|snapshot| snapshot.replay_ready),
                    Ok((records, missing_block_ids)) => {
                        let _ = session.apply_selection_materialization_result(
                            &request,
                            records,
                            &missing_block_ids,
                        );
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
        if self.status.readonly {
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
        let session = self.ready_session().cloned().ok_or(CditorError::NotReady)?;
        let token = match session
            .begin_session_task(
                SessionTaskKind::HistoryHydration,
                reference.snapshot_id.rotate_left(1) ^ u64::from(redo),
            )
            .map_err(|error| CditorError::Internal(error.to_string()))?
        {
            SessionTaskAdmission::Started(token) => token,
            SessionTaskAdmission::Duplicate => return Ok(()),
            SessionTaskAdmission::Busy => {
                return Err(CditorError::Internal(
                    "another undo or redo hydration is already running".to_owned(),
                ));
            }
        };
        let storage_request = session
            .undo_blob_read_request(reference.clone())
            .map_err(|error| CditorError::Internal(error.to_string()))?;
        let Some(storage_request) = storage_request else {
            let _ = session.complete_session_task(token);
            return Err(CditorError::Unsupported(
                "external undo hydration requires persistent storage".to_owned(),
            ));
        };
        cx.emit(CditorEvent::HistoryHydrationStarted {
            snapshot_id: reference.snapshot_id,
            redo,
        });
        let hydrate_task = cx.background_spawn(async move {
            block_on_storage(async move {
                tokio::time::timeout(token.timeout(), execute_undo_blob_read(storage_request))
                    .await
                    .map_err(|_| StorageError::Timeout {
                        operation: "history hydration",
                        timeout: token.timeout(),
                    })?
            })
            .and_then(|result| result.map_err(|error| error.to_string()))
        });
        cx.spawn(async move |view, cx| {
            let result = hydrate_task.await;
            let _ = view.update(cx, |view, cx| {
                let Some(current_session) = view.ready_session().cloned() else {
                    return;
                };
                if !current_session
                    .complete_session_task(token)
                    .unwrap_or(false)
                {
                    return;
                }
                let replay = match result {
                    Ok((reference, transaction)) => {
                        let direction = if redo {
                            HistoryDirection::Redo
                        } else {
                            HistoryDirection::Undo
                        };
                        current_session
                            .apply_hydrated_history(&reference, transaction, source, direction)
                            .map(|snapshot| (snapshot.outcome.changed(), snapshot.revision))
                            .map_err(|error| error.message)
                    }
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
        let was_dirty = self.status.dirty;
        self.status.dirty = true;
        self.status.save_status = EditorSaveStatus::Dirty;
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
        let Some(session) = self.ready_session().cloned() else {
            return;
        };
        let token = match session
            .begin_session_task(SessionTaskKind::UndoSpill, 0)
            .ok()
        {
            Some(SessionTaskAdmission::Started(token)) => token,
            _ => return,
        };
        let Some(request) = session.undo_blob_write_request().ok().flatten() else {
            let _ = session.complete_session_task(token);
            return;
        };
        let spill_task =
            cx.background_spawn(async move { run_undo_blob_write(request, token.timeout()) });
        cx.spawn(async move |view, cx| {
            let (job, result) = spill_task.await;
            let _ = view.update(cx, |view, cx| {
                if let Some(session_handle) = view.ready_session()
                    && session_handle.complete_session_task(token).unwrap_or(false)
                {
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
        let Some(session) = self.ready_session().cloned() else {
            return;
        };
        let token = match session
            .begin_session_task(SessionTaskKind::UndoCleanup, 0)
            .ok()
        {
            Some(SessionTaskAdmission::Started(token)) => token,
            _ => return,
        };
        let Some(request) = session.undo_blob_delete_request().ok().flatten() else {
            let _ = session.complete_session_task(token);
            return;
        };
        let cleanup_task =
            cx.background_spawn(async move { run_undo_blob_delete(request, token.timeout()) });
        cx.spawn(async move |view, cx| {
            let failed = cleanup_task.await;
            let _ = view.update(cx, |view, cx| {
                if let Some(session_handle) = view.ready_session()
                    && session_handle.complete_session_task(token).unwrap_or(false)
                {
                    let _ = session_handle.finish_undo_blob_cleanup(failed);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn flush_storage_persistence(&mut self, cx: &mut Context<Self>) {
        if self.status.readonly {
            if let Some(session) = self.ready_session() {
                let _ = session.clear_scheduled_save();
            }
            return;
        }
        let Some(session) = self.ready_session().cloned() else {
            return;
        };
        let save_token = match session
            .begin_session_task(SessionTaskKind::PersistenceSave, 0)
            .ok()
        {
            Some(SessionTaskAdmission::Started(token)) => token,
            _ => return,
        };
        let Ok(Some(batch)) = session.capture_storage_save() else {
            let _ = session.complete_session_task(save_token);
            self.settle_storage_barriers(cx);
            return;
        };
        let revision = batch.revision();
        self.status.save_status = EditorSaveStatus::Saving;
        cx.emit(CditorEvent::SaveStarted { revision });
        let save_task = cx.background_spawn(async move {
            let result = block_on_storage(async {
                tokio::time::timeout(save_token.timeout(), save_storage_batch(&batch))
                    .await
                    .map_err(|_| {
                        format!("storage save timed out after {:?}", save_token.timeout())
                    })?
            })
            .and_then(|result| result);
            (batch, result)
        });
        cx.spawn(async move |view, cx| match save_task.await {
            (request, Ok(outcome)) => {
                let _ = view.update(cx, |view, cx| {
                    let Some(session) = view.ready_session().cloned() else {
                        return;
                    };
                    if !session.complete_session_task(save_token).unwrap_or(false) {
                        return;
                    }
                    let should_reschedule = view
                        .ready_session()
                        .and_then(|session| {
                            session.apply_storage_save_success(&request, &outcome).ok()
                        })
                        .is_some_and(|apply| apply.should_reschedule);
                    view.trim_persistent_payload_cache();
                    let became_clean = view.status.dirty && !should_reschedule;
                    view.status.dirty = should_reschedule;
                    view.status.save_status = if view.status.readonly {
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
                    let Some(session) = view.ready_session().cloned() else {
                        return;
                    };
                    if !session.complete_session_task(save_token).unwrap_or(false) {
                        return;
                    }
                    let should_reschedule = view
                        .ready_session()
                        .and_then(|session| {
                            session.apply_storage_save_failure(&request, &message).ok()
                        })
                        .unwrap_or(false);
                    view.status.dirty = true;
                    view.status.save_status = EditorSaveStatus::Failed(message.clone());
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
        let flush_token = match session
            .begin_session_task(SessionTaskKind::StorageFlush, 0)
            .ok()
        {
            Some(SessionTaskAdmission::Started(token)) => token,
            _ => {
                let error = PersistencePipelineError::Cancelled;
                for barrier in flush_barriers {
                    barrier.resolve(Err(error.clone()));
                }
                return;
            }
        };
        let Ok(Some(flush_request)) = session.begin_storage_flush() else {
            let _ = session.complete_session_task(flush_token);
            let error = PersistencePipelineError::Unavailable(
                "save and flush require a persistent storage backend".to_owned(),
            );
            for barrier in flush_barriers {
                barrier.resolve(Err(error.clone()));
            }
            return;
        };

        let flush_task = cx.background_spawn(async move {
            block_on_storage(async move {
                tokio::time::timeout(flush_token.timeout(), execute_storage_flush(flush_request))
                    .await
                    .map_err(|_| {
                        format!("storage flush timed out after {:?}", flush_token.timeout())
                    })?
            })
            .and_then(|result| result)
        });
        cx.spawn(async move |view, cx| {
            let result = flush_task.await.map_err(PersistencePipelineError::Storage);
            let state_result = result.clone();
            let _ = view.update(cx, |view, cx| {
                if let Some(session) = view.ready_session() {
                    if !session.complete_session_task(flush_token).unwrap_or(false) {
                        return;
                    }
                    let _ = session.finish_storage_flush();
                }
                if let Err(error) = &state_result {
                    view.status.save_status = EditorSaveStatus::Failed(error.to_string());
                } else if !view.status.dirty && !view.status.readonly {
                    view.status.save_status = EditorSaveStatus::Clean;
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
        token: SessionTaskToken,
        request: PayloadWindowLoadRequest,
        cx: &mut Context<Self>,
    ) {
        let failed_request = request.clone();
        let load_task = cx.background_spawn(async move {
            block_on_storage(async move {
                let loaded = tokio::time::timeout(
                    token.timeout(),
                    execute_payload_load(storage_request, &request.block_ids),
                )
                .await
                .map_err(|_| StorageError::Timeout {
                    operation: "storage viewport payload load",
                    timeout: token.timeout(),
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
                        let _ = session.complete_payload_window_task(token, Ok(result));
                    }
                    view.trim_persistent_payload_cache();
                    cx.notify();
                });
            }
            Err(message) => {
                let _ = view.update(cx, |view, cx| {
                    if let Some(session) = view.ready_session() {
                        let _ = session
                            .complete_payload_window_task(token, Err((failed_request, message)));
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
                if let Some(session) = view.ready_session() {
                    let _ = session.wake_payload_window_task();
                }
                cx.notify();
            });
        })
        .detach();
    }
}

pub(crate) fn save_status_for_mode(readonly: bool) -> EditorSaveStatus {
    if readonly {
        EditorSaveStatus::Readonly
    } else {
        EditorSaveStatus::Clean
    }
}

#[cfg(test)]
#[path = "persistence_bridge_tests.rs"]
mod tests;
