use gpui::Context;

use crate::app::CditorV2View;

pub fn schedule_storage_autosave(view: &CditorV2View, cx: &mut Context<CditorV2View>) {
    let Some(delay) = view
        .ready_session()
        .and_then(|session| session.request_autosave_timer().ok().flatten())
    else {
        return;
    };
    let debounce = cx.background_executor().timer(delay);
    cx.spawn(async move |view, cx| {
        debounce.await;
        let _ = view.update(cx, |view, cx| view.flush_storage_persistence(cx));
    })
    .detach();
}
