use gpui::{AppContext, Context};

use cditor_runtime::{MainThreadWorkKind, WorkCost};
use cditor_session::{
    HistoryActionSnapshot, HistoryDirection, SessionTaskAdmission, SessionTaskKind,
    UndoBlobWriteResult, run_storage_flush_with_timeout, run_storage_save_with_timeout,
    run_undo_blob_delete, run_undo_blob_read, run_undo_blob_write,
};

use crate::editor_view::CditorV2View;
use crate::persistence::{EditorSaveStatus, PersistencePipelineError, schedule_storage_autosave};
use cditor_core::edit::ChangeOrigin;
use cditor_editor_protocol::command::CommandSource;
use cditor_sdk::CditorError;
use cditor_sdk::event::CditorEvent;

impl CditorV2View {
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
        let hydrate_task = cx
            .background_spawn(async move { run_undo_blob_read(storage_request, token.timeout()) });
        cx.spawn(async move |view, cx| {
            let result = hydrate_task.await;
            let _ = view.update(cx, |view, cx| {
                view.enqueue_main_thread_apply(
                    MainThreadWorkKind::WindowSwap,
                    reference.snapshot_id,
                    None,
                    WorkCost {
                        sync_ms: 0.3,
                        async_results: 1,
                        ..WorkCost::ZERO
                    },
                    move |view, cx| {
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
                                    .apply_hydrated_history(
                                        &reference,
                                        transaction,
                                        source,
                                        direction,
                                    )
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
                    },
                    cx,
                );
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
        self.status.save_status = EditorSaveStatus::DirtyMemory;
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
                view.enqueue_main_thread_apply(
                    MainThreadWorkKind::PersistenceCallback,
                    job.snapshot_id,
                    None,
                    WorkCost {
                        sync_ms: 0.05,
                        async_results: 1,
                        ..WorkCost::ZERO
                    },
                    move |view, cx| {
                        if let Some(session_handle) = view.ready_session()
                            && session_handle.complete_session_task(token).unwrap_or(false)
                        {
                            let result = result
                                .map_or(UndoBlobWriteResult::Failed, UndoBlobWriteResult::Stored);
                            let _ = session_handle.apply_undo_blob_write_result(job, result);
                        }
                        cx.notify();
                    },
                    cx,
                );
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
                view.enqueue_main_thread_apply(
                    MainThreadWorkKind::PersistenceCallback,
                    0,
                    None,
                    WorkCost {
                        sync_ms: 0.05,
                        async_results: 1,
                        ..WorkCost::ZERO
                    },
                    move |view, cx| {
                        if let Some(session_handle) = view.ready_session()
                            && session_handle.complete_session_task(token).unwrap_or(false)
                        {
                            let _ = session_handle.finish_undo_blob_cleanup(failed);
                        }
                        cx.notify();
                    },
                    cx,
                );
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
        self.status.save_status = EditorSaveStatus::SavingLocal;
        cx.emit(CditorEvent::SaveStarted { revision });
        let save_task = cx.background_spawn(async move {
            let result = run_storage_save_with_timeout(&batch, save_token.timeout());
            (batch, result)
        });
        cx.spawn(async move |view, cx| match save_task.await {
            (request, Ok(outcome)) => {
                let _ = view.update(cx, |view, cx| {
                    view.enqueue_main_thread_apply(
                        MainThreadWorkKind::PersistenceCallback,
                        revision,
                        None,
                        WorkCost {
                            sync_ms: 0.15,
                            async_results: 1,
                            ..WorkCost::ZERO
                        },
                        move |view, cx| {
                            let Some(session) = view.ready_session().cloned() else {
                                return;
                            };
                            if !session.complete_session_task(save_token).unwrap_or(false) {
                                return;
                            }
                            let should_reschedule = session
                                .apply_storage_save_success(&request, &outcome)
                                .is_ok_and(|apply| apply.should_reschedule);
                            view.schedule_persistent_payload_cache_trim(cx);
                            let became_clean = view.status.dirty && !should_reschedule;
                            view.status.dirty = should_reschedule;
                            view.status.save_status = if view.status.readonly {
                                EditorSaveStatus::Readonly
                            } else if should_reschedule {
                                EditorSaveStatus::DirtyMemory
                            } else {
                                EditorSaveStatus::LocallySaved
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
                        },
                        cx,
                    );
                });
            }
            (request, Err(failure)) => {
                let _ = view.update(cx, |view, cx| {
                    view.enqueue_main_thread_apply(
                        MainThreadWorkKind::PersistenceCallback,
                        revision,
                        None,
                        WorkCost {
                            sync_ms: 0.1,
                            async_results: 1,
                            ..WorkCost::ZERO
                        },
                        move |view, cx| {
                            let Some(session) = view.ready_session().cloned() else {
                                return;
                            };
                            if !session.complete_session_task(save_token).unwrap_or(false) {
                                return;
                            }
                            let should_reschedule = session
                                .apply_storage_save_failure(&request, &failure)
                                .unwrap_or(false);
                            view.status.dirty = true;
                            view.status.save_status =
                                EditorSaveStatus::FailedLocal(failure.clone());
                            cx.emit(CditorEvent::SaveFailed {
                                revision,
                                error: CditorError::Persistence(failure.to_string()),
                            });
                            if should_reschedule {
                                schedule_storage_autosave(view, cx);
                            }
                            cx.notify();
                        },
                        cx,
                    );
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
            run_storage_flush_with_timeout(flush_request, flush_token.timeout())
        });
        cx.spawn(async move |view, cx| {
            let result = flush_task.await.map_err(PersistencePipelineError::Storage);
            let _ = view.update(cx, |view, cx| {
                view.enqueue_main_thread_apply(
                    MainThreadWorkKind::PersistenceCallback,
                    0,
                    None,
                    WorkCost {
                        sync_ms: 0.08,
                        async_results: 1,
                        ..WorkCost::ZERO
                    },
                    move |view, cx| {
                        if let Some(session) = view.ready_session() {
                            if !session.complete_session_task(flush_token).unwrap_or(false) {
                                return;
                            }
                            let _ = session.finish_storage_flush();
                        }
                        if let Err(error) = &result {
                            view.status.save_status = match error {
                                PersistencePipelineError::Storage(failure) => {
                                    EditorSaveStatus::FailedLocal(failure.clone())
                                }
                                _ => EditorSaveStatus::Failed(error.to_string()),
                            };
                        } else if !view.status.dirty && !view.status.readonly {
                            view.status.save_status = EditorSaveStatus::LocallySaved;
                        }
                        view.settle_storage_barriers(cx);
                        for barrier in flush_barriers {
                            barrier.resolve(result.clone());
                        }
                        cx.notify();
                    },
                    cx,
                );
            });
        })
        .detach();
    }
}

pub(crate) fn save_status_for_mode(readonly: bool) -> EditorSaveStatus {
    if readonly {
        EditorSaveStatus::Readonly
    } else {
        EditorSaveStatus::LocallySaved
    }
}

#[cfg(test)]
#[path = "persistence_bridge_tests.rs"]
mod tests;
