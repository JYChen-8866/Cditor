use gpui::{AnyElement, FontWeight, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::block::paragraph::NOTION_PARAGRAPH_TEXT_SIZE_PX;
use crate::gui::block::skeleton::render_block_skeleton;
use cditor_core::layout::block_metrics::NOTION_BODY_LINE_HEIGHT_PX;
use cditor_core::layout::text_line_height_for_kind;
use cditor_core::rich_text::RichBlockKind;
use cditor_runtime::ViewBlockSnapshot;

const EMPTY_LINE_PLACEHOLDER_TEXT_SIZE_PX: f32 = NOTION_PARAGRAPH_TEXT_SIZE_PX;
const EMPTY_LINE_PLACEHOLDER_LINE_HEIGHT_PX: f32 = NOTION_BODY_LINE_HEIGHT_PX as f32;
const EMPTY_LINE_PLACEHOLDER_LEFT_PX: f32 = 0.0;

pub fn render_placeholder(block: &ViewBlockSnapshot, theme: GuiTheme) -> AnyElement {
    render_block_skeleton(block, theme)
}

pub fn render_empty_ai_hint(kind: &RichBlockKind, theme: GuiTheme) -> AnyElement {
    div()
        .absolute()
        .left(px(EMPTY_LINE_PLACEHOLDER_LEFT_PX))
        .top(px(empty_line_placeholder_top_px(kind)))
        .h(px(EMPTY_LINE_PLACEHOLDER_LINE_HEIGHT_PX))
        .flex()
        .items_center()
        .text_size(px(EMPTY_LINE_PLACEHOLDER_TEXT_SIZE_PX))
        .font_weight(FontWeight::NORMAL)
        .text_color(rgb(theme.muted))
        .child("按 space（空格）以启用 AI，或按“/”启用命令")
        .into_any_element()
}

fn empty_line_placeholder_top_px(kind: &RichBlockKind) -> f32 {
    ((text_line_height_for_kind(kind) as f32 - EMPTY_LINE_PLACEHOLDER_LINE_HEIGHT_PX) / 2.0)
        .max(0.0)
}

pub fn render_loading(block: &ViewBlockSnapshot, theme: GuiTheme) -> AnyElement {
    render_block_skeleton(block, theme)
}

pub fn render_error(message: &str, theme: GuiTheme) -> AnyElement {
    div()
        .text_size(gpui::px(13.0))
        .text_color(rgb(theme.danger))
        .child(format!("Unable to load block: {message}"))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_line_placeholder_uses_regular_paragraph_typography() {
        assert_eq!(
            EMPTY_LINE_PLACEHOLDER_TEXT_SIZE_PX,
            NOTION_PARAGRAPH_TEXT_SIZE_PX
        );
        assert_eq!(
            EMPTY_LINE_PLACEHOLDER_LINE_HEIGHT_PX,
            NOTION_BODY_LINE_HEIGHT_PX as f32
        );
        assert_eq!(EMPTY_LINE_PLACEHOLDER_LEFT_PX, 0.0);
    }

    #[test]
    fn empty_line_placeholder_centers_its_body_line_box_inside_each_heading_line() {
        assert_eq!(
            empty_line_placeholder_top_px(&RichBlockKind::Paragraph),
            0.0
        );
        assert_eq!(
            empty_line_placeholder_top_px(&RichBlockKind::Heading { level: 1 }),
            7.5
        );
        assert_eq!(
            empty_line_placeholder_top_px(&RichBlockKind::Heading { level: 2 }),
            4.0
        );
        assert_eq!(
            empty_line_placeholder_top_px(&RichBlockKind::Heading { level: 3 }),
            1.0
        );
    }
}
