use gpui::{AnyElement, Entity};

use crate::document::DocumentLayoutMetrics;
use crate::editor_view::CditorV2View;
use crate::persistence::{
    EditorSaveStatus, READONLY_NOTICE_HEIGHT_PX, render_readonly_notice, render_save_failure_notice,
};
use crate::theme::GuiTheme;

impl CditorV2View {
    pub(crate) fn document_layout_metrics(&self, viewport_width_px: f32) -> DocumentLayoutMetrics {
        let top_inset_px = document_top_inset_px(self.status.readonly_reason.is_some());
        DocumentLayoutMetrics::for_viewport(viewport_width_px).with_top_inset_px(top_inset_px)
    }

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

const fn document_top_inset_px(readonly_notice_visible: bool) -> f32 {
    if readonly_notice_visible {
        READONLY_NOTICE_HEIGHT_PX
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_top_inset_exists_only_while_the_readonly_notice_is_visible() {
        assert_eq!(document_top_inset_px(false), 0.0);
        assert_eq!(document_top_inset_px(true), READONLY_NOTICE_HEIGHT_PX);
    }
}
