use gpui::{AnyElement, Entity};

use crate::editor_view::CditorV2View;
use crate::persistence::{EditorSaveStatus, render_readonly_notice, render_save_failure_notice};
use crate::theme::GuiTheme;

impl CditorV2View {
    pub(super) fn render_status_overlays(
        &self,
        theme: GuiTheme,
        view: Entity<Self>,
    ) -> Vec<AnyElement> {
        let mut overlays = Vec::with_capacity(2);
        if let Some(reason) = self.status.readonly_reason.as_ref() {
            overlays.push(render_readonly_notice(reason, theme));
        }
        if let EditorSaveStatus::FailedLocal(failure) = &self.status.save_status {
            overlays.push(render_save_failure_notice(failure, theme, view));
        }
        overlays
    }
}
