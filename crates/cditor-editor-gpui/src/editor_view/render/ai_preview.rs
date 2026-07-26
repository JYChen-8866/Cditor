use gpui::{Bounds, point, px, size};

use crate::document::{DEFAULT_DOCUMENT_TOP_INSET_PX, DocumentLayoutMetrics};
use crate::interaction::geometry::fallback_text_metrics_for_block;
use crate::theme::GuiTheme;
use cditor_runtime::EditorViewProjection;

pub(super) fn projected_ai_preview_block_anchor(
    projection: &EditorViewProjection,
    theme: GuiTheme,
    document_layout: DocumentLayoutMetrics,
    viewport_width: f32,
) -> Option<Bounds<gpui::Pixels>> {
    let preview = projection.ai_preview.as_ref()?;
    let mut document_top = projection.before_window_height;
    projection.blocks.iter().find_map(|block| {
        let block_height = block.layout.effective_height();
        let result = (block.block_id == preview.block_id).then(|| {
            let metrics = fallback_text_metrics_for_block(block, theme, document_layout);
            ai_preview_block_anchor(
                document_top,
                block_height,
                metrics.origin_x_in_block_px,
                metrics.width_px,
                viewport_width,
                projection.scroll.global_scroll_top,
            )
        });
        document_top += block_height;
        result
    })
}

pub(super) fn ai_preview_block_anchor(
    document_top: f64,
    block_height: f64,
    text_origin_x: f64,
    text_width: f64,
    viewport_width: f32,
    scroll_top: f64,
) -> Bounds<gpui::Pixels> {
    let document_layout = DocumentLayoutMetrics::for_viewport(viewport_width);
    let page_left = ((viewport_width - document_layout.page_width_px) / 2.0).max(0.0);
    let top = (document_top - scroll_top) as f32 + DEFAULT_DOCUMENT_TOP_INSET_PX;
    let height = block_height.max(24.0) as f32;
    Bounds::new(
        point(px(page_left + text_origin_x as f32), px(top)),
        size(px(text_width as f32), px(height)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_panel_anchor_tracks_projected_block_after_scroll() {
        let anchor = ai_preview_block_anchor(920.0, 48.0, 42.0, 760.0, 1200.0, 600.0);
        assert_eq!(f32::from(anchor.left()), 42.0);
        assert_eq!(f32::from(anchor.top()), 352.0);
        assert_eq!(f32::from(anchor.bottom()), 400.0);
    }
}
