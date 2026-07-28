use std::time::Duration;

use gpui::Context;

use super::DrafftBoardView;

const PERSISTENCE_QUIET_PERIOD: Duration = Duration::from_millis(250);

impl DrafftBoardView {
    pub(super) fn observe_document_change(&mut self, cx: &mut Context<Self>) {
        let revision = self.board.document_revision();
        if revision == self.last_observed_document_revision {
            return;
        }
        self.last_observed_document_revision = revision;
        self.persistence_epoch = self.persistence_epoch.wrapping_add(1);
        self.schedule_persistence(cx);
    }

    fn schedule_persistence(&mut self, cx: &mut Context<Self>) {
        if self.persistence_scheduled || self.read_only || self.on_change.is_none() {
            return;
        }
        self.persistence_scheduled = true;
        let epoch = self.persistence_epoch;
        let tick = cx.background_executor().timer(PERSISTENCE_QUIET_PERIOD);
        cx.spawn(async move |view, cx| {
            let _ = tick.await;
            let _ = view.update(cx, |view, cx| {
                view.persistence_scheduled = false;
                if view.persistence_epoch != epoch {
                    view.schedule_persistence(cx);
                } else {
                    view.flush_current_scene(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn flush_current_scene(&mut self, cx: &mut Context<Self>) {
        self.last_observed_document_revision = self.board.document_revision();
        let Ok(scene_json) = self.board.document_json() else {
            return;
        };
        if scene_json == self.last_emitted_scene_json {
            return;
        }
        self.last_emitted_scene_json = scene_json.clone();
        if let Some(on_change) = self.on_change.clone() {
            on_change(scene_json, cx);
        }
    }
}
