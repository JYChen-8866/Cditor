use gpui::{AppContext, Context};

use cditor_runtime::content::payload_window::{PayloadWindowLoadRequest, PayloadWindowLoadResult};
use cditor_storage_postgres::PostgresPayloadStore;
use cditor_storage_postgres::block_on_postgres;

use crate::api::{CditorError, CditorEvent, ChangeOrigin};
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::persistence::{
    EditorSaveStatus, POSTGRES_VIEWPORT_LOAD_TIMEOUT, mark_dirty_and_schedule_postgres_save,
    save_postgres_batch,
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
        mark_dirty_and_schedule_postgres_save(
            &mut self.postgres_persistence,
            &mut self.save_status,
            cx,
        );
        cx.emit(CditorEvent::ContentChanged { revision, origin });
        if !was_dirty {
            cx.emit(CditorEvent::DirtyChanged { dirty: true });
        }
    }

    pub(crate) fn flush_postgres_persistence(&mut self, cx: &mut Context<Self>) {
        if self.readonly {
            self.postgres_persistence.clear_scheduled_save();
            return;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        let Some(batch) = self.postgres_persistence.begin_batch(runtime) else {
            return;
        };
        let revision = runtime.revision();
        self.save_status = EditorSaveStatus::Saving;
        cx.emit(CditorEvent::SaveStarted { revision });
        let save_task = cx.background_spawn(async move {
            block_on_postgres(save_postgres_batch(batch)).and_then(|result| result)
        });
        cx.spawn(async move |view, cx| match save_task.await {
            Ok(outcome) => {
                let _ = view.update(cx, |view, cx| {
                    let saved_layout_or_structure = outcome.saved_structure_version.is_some();
                    let should_reschedule = view
                        .postgres_persistence
                        .finish_success(outcome.saved_structure_version);
                    if let Some(runtime) = view.ready_runtime() {
                        runtime.mark_payload_versions_persisted(&outcome.saved_payload_versions);
                    }
                    if saved_layout_or_structure
                        && !should_reschedule
                        && let Some(runtime) = view.ready_runtime()
                    {
                        runtime.mark_layout_saved();
                    }
                    view.trim_postgres_payload_cache();
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
                        view.postgres_persistence.schedule(cx);
                    }
                    cx.notify();
                });
            }
            Err(message) => {
                let _ = view.update(cx, |view, cx| {
                    view.postgres_persistence.finish_failed();
                    view.dirty = true;
                    view.save_status = EditorSaveStatus::Failed(message.clone());
                    cx.emit(CditorEvent::SaveFailed {
                        revision,
                        error: CditorError::Persistence(message),
                    });
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn load_postgres_payload_window(
        &mut self,
        pool: sqlx::PgPool,
        request: PayloadWindowLoadRequest,
        cx: &mut Context<Self>,
    ) {
        let failed_request = request.clone();
        let load_task = cx.background_spawn(async move {
            let store = PostgresPayloadStore::new(pool);
            block_on_postgres(async move {
                let loaded = tokio::time::timeout(
                    POSTGRES_VIEWPORT_LOAD_TIMEOUT,
                    store.load_block_payloads(&request.block_ids),
                )
                .await
                .map_err(|_| {
                    cditor_storage_postgres::PostgresStorageError::Timeout {
                        operation: "PostgreSQL viewport payload load",
                        timeout: POSTGRES_VIEWPORT_LOAD_TIMEOUT,
                    }
                })??;
                Ok::<_, cditor_storage_postgres::PostgresStorageError>(PayloadWindowLoadResult {
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
                    view.trim_postgres_payload_cache();
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

    pub(crate) fn schedule_postgres_payload_window_wake(
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
