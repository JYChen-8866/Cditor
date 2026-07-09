use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use cditor_runtime::EditorViewProjection;

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentDebugHeader {
    pub rendered_blocks: usize,
    pub total_visible_blocks: usize,
    pub before_spacer_height: f64,
    pub after_spacer_height: f64,
    pub window_range: std::ops::Range<usize>,
    pub global_scroll_top: f64,
    pub total_height: f64,
    pub last_wheel_delta_y: f64,
    pub focused_block: Option<u64>,
}

impl DocumentDebugHeader {
    pub fn from_projection(
        projection: &EditorViewProjection,
        last_wheel_delta_y: f64,
        focused_block: Option<u64>,
    ) -> Self {
        Self {
            rendered_blocks: projection.blocks.len(),
            total_visible_blocks: projection.total_visible_blocks,
            before_spacer_height: projection.before_window_height,
            after_spacer_height: projection.after_window_height,
            window_range: projection.render_window.block_range.clone(),
            global_scroll_top: projection.scroll.global_scroll_top,
            total_height: projection.scroll.model_total_height,
            last_wheel_delta_y,
            focused_block,
        }
    }

    pub fn render(&self, theme: GuiTheme) -> AnyElement {
        div()
            .flex_none()
            .px_4()
            .py_2()
            .text_size(px(12.0))
            .text_color(rgb(theme.muted))
            .child(format!(
                "rendered_blocks={} total_visible_blocks={} before_spacer={:.1} after_spacer={:.1} window={:?} global_scroll_top={:.1} total_height={:.1} last_wheel_delta_y={:.1} focused={:?}",
                self.rendered_blocks,
                self.total_visible_blocks,
                self.before_spacer_height,
                self.after_spacer_height,
                self.window_range,
                self.global_scroll_top,
                self.total_height,
                self.last_wheel_delta_y,
                self.focused_block,
            ))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn debug_header_projects_runtime_view_state() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();

        let header =
            DocumentDebugHeader::from_projection(&projection, 12.0, runtime.focused_block_id());

        assert_eq!(header.rendered_blocks, projection.blocks.len());
        assert_eq!(header.total_visible_blocks, projection.total_visible_blocks);
        assert_eq!(header.last_wheel_delta_y, 12.0);
    }
}
