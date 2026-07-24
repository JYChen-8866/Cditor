use gpui::Context;

use crate::app::CditorV2View;
use crate::persistence::EditorSaveStatus;
use cditor_session::PersistencePipeline;

pub fn schedule_storage_autosave(
    persistence: &mut PersistencePipeline,
    cx: &mut Context<CditorV2View>,
) {
    let Some(delay) = persistence.request_autosave_timer() else {
        return;
    };
    let debounce = cx.background_executor().timer(delay);
    cx.spawn(async move |view, cx| {
        debounce.await;
        let _ = view.update(cx, |view, cx| view.flush_storage_persistence(cx));
    })
    .detach();
}

pub fn mark_dirty_and_schedule_save(
    persistence: &mut PersistencePipeline,
    save_status: &mut EditorSaveStatus,
    cx: &mut Context<CditorV2View>,
) {
    *save_status = EditorSaveStatus::Dirty;
    persistence.mark_dirty();
    schedule_storage_autosave(persistence, cx);
}
