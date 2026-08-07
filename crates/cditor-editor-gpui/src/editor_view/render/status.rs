use gpui::{AnyElement, Entity};

use crate::document::DocumentLayoutMetrics;
use crate::editor_view::CditorV2View;
use crate::persistence::{
    EditorSaveStatus, READONLY_NOTICE_HEIGHT_PX, render_readonly_notice, render_save_failure_notice,
};
use crate::theme::GuiTheme;

impl CditorV2View {
    pub(crate) fn document_layout_metrics(&self, viewport_width_px: f32) -> DocumentLayoutMetrics {
        if self.embedded_composer {
            return DocumentLayoutMetrics::embedded_composer(viewport_width_px);
        }
        let decorations = self
            .ready_session()
            .and_then(|session| session.document_snapshot().ok());
        document_layout_metrics_for_status(
            viewport_width_px,
            decorations
                .as_ref()
                .is_some_and(|snapshot| snapshot.cover.is_some()),
            decorations
                .as_ref()
                .is_some_and(|snapshot| snapshot.icon.is_some()),
            self.status.readonly_reason.is_some(),
        )
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

fn document_layout_metrics_for_status(
    viewport_width_px: f32,
    has_cover: bool,
    has_icon: bool,
    readonly_notice_visible: bool,
) -> DocumentLayoutMetrics {
    let notice_height_px = if readonly_notice_visible {
        READONLY_NOTICE_HEIGHT_PX
    } else {
        0.0
    };
    DocumentLayoutMetrics::for_viewport(viewport_width_px)
        .with_page_decorations(has_cover, has_icon)
        .with_additional_top_inset_px(notice_height_px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readonly_notice_adds_to_the_document_header_space() {
        let editable = document_layout_metrics_for_status(1_200.0, false, false, false);
        let readonly = document_layout_metrics_for_status(1_200.0, false, false, true);

        assert_eq!(editable.top_inset_px, 96.0);
        assert_eq!(
            readonly.top_inset_px,
            editable.top_inset_px + READONLY_NOTICE_HEIGHT_PX
        );
    }

    #[test]
    fn readonly_notice_adds_after_page_decorations() {
        let decorated = document_layout_metrics_for_status(1_200.0, true, true, false);
        let readonly = document_layout_metrics_for_status(1_200.0, true, true, true);

        assert_eq!(decorated.top_inset_px, 300.0);
        assert_eq!(readonly.top_inset_px, 300.0 + READONLY_NOTICE_HEIGHT_PX);
    }
}
