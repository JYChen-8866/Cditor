use gpui::{AppContext, Context};

use cditor_storage_postgres::block_on_postgres;

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::persistence::{
    EditorSaveStatus, mark_dirty_and_schedule_postgres_save, save_postgres_batch,
};

impl CditorV2View {
    pub(crate) fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        mark_dirty_and_schedule_postgres_save(
            &mut self.postgres_persistence,
            &mut self.save_status,
            cx,
        );
    }

    pub(crate) fn flush_postgres_persistence(&mut self, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        let Some(batch) = self.postgres_persistence.begin_batch(runtime) else {
            return;
        };
        self.save_status = EditorSaveStatus::Saving;
        let save_task = cx.background_spawn(async move {
            block_on_postgres(save_postgres_batch(batch)).and_then(|result| result)
        });
        cx.spawn(async move |view, cx| match save_task.await {
            Ok(saved_structure_version) => {
                let _ = view.update(cx, |view, cx| {
                    let saved_layout_or_structure = saved_structure_version.is_some();
                    let should_reschedule = view
                        .postgres_persistence
                        .finish_success(saved_structure_version);
                    if saved_layout_or_structure
                        && !should_reschedule
                        && let Some(runtime) = view.ready_runtime()
                    {
                        runtime.mark_layout_saved();
                    }
                    view.save_status = save_status_for_mode(view.readonly);
                    if should_reschedule {
                        view.postgres_persistence.schedule(cx);
                    }
                    cx.notify();
                });
            }
            Err(message) => {
                let _ = view.update(cx, |view, cx| {
                    view.postgres_persistence.finish_failed();
                    view.save_status = EditorSaveStatus::Failed(message);
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }
}

pub(in crate::gui::app) fn save_status_for_mode(readonly: bool) -> EditorSaveStatus {
    if readonly {
        EditorSaveStatus::Readonly
    } else {
        EditorSaveStatus::Clean
    }
}
