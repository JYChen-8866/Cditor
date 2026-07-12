use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgba};

use crate::gui::GuiTheme;
use crate::gui::block::chrome::{
    BLOCK_GUTTER_WIDTH_PX, BLOCK_INDENT_STEP_PX, BLOCK_ROW_GAP_PX, BLOCK_SHELL_OUTER_PADDING_X_PX,
};
use cditor_core::ids::BlockId;
use cditor_runtime::EditorViewProjection;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionOverlayFragment {
    pub block_id: BlockId,
    pub y: f64,
    pub height: f64,
    pub full_block: bool,
    /// Left edge of block content after shell padding, indent, gutter, and row gap.
    pub content_left_px: f32,
}

pub fn selection_overlay_fragments(
    projection: &EditorViewProjection,
) -> Vec<SelectionOverlayFragment> {
    let mut fragments = Vec::new();
    let mut block_y = projection.before_window_height;
    for block in &projection.blocks {
        let height = block.layout.effective_height();
        let content_left = selection_content_left_px(block.chrome.list_info.depth);
        if block.selected || block.selection_overlay {
            fragments.push(SelectionOverlayFragment {
                block_id: block.block_id,
                y: block_y,
                height,
                full_block: block.selected,
                content_left_px: content_left,
            });
        }
        block_y += height;
    }
    fragments
}

fn selection_content_left_px(depth: usize) -> f32 {
    BLOCK_SHELL_OUTER_PADDING_X_PX
        + BLOCK_GUTTER_WIDTH_PX
        + BLOCK_ROW_GAP_PX
        + depth as f32 * BLOCK_INDENT_STEP_PX
}

pub fn render_selection_overlay(
    fragments: &[SelectionOverlayFragment],
    theme: GuiTheme,
) -> AnyElement {
    let background = selection_overlay_background(theme);
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .children(fragments.iter().map(|fragment| {
            div()
                .absolute()
                .left(px(fragment.content_left_px))
                .right_0()
                .top(px(fragment.y as f32))
                .h(px(fragment.height as f32))
                .bg(rgba(background))
        }))
        .into_any_element()
}

fn selection_overlay_background(theme: GuiTheme) -> u32 {
    (theme.action_accent << 8) | 0x33
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn selection_overlay_uses_projection_fragments_not_entities() {
        let mut runtime = DocumentRuntime::demo();
        runtime.select_all_visible_blocks();
        let projection = runtime.projection_for_window();

        let fragments = selection_overlay_fragments(&projection);

        assert_eq!(fragments.len(), projection.blocks.len());
        assert!(fragments.iter().all(|fragment| fragment.full_block));
    }

    #[test]
    fn selection_overlay_uses_translucent_theme_accent() {
        let theme = GuiTheme::light();

        assert_eq!(
            selection_overlay_background(theme),
            (theme.action_accent << 8) | 0x33
        );
    }

    #[test]
    fn whole_cross_block_text_selection_creates_contiguous_content_fragments() {
        let mut first = cditor_core::rich_text::RichBlockRecord::paragraph(1, "first");
        first.children = vec![2];
        let mut middle = cditor_core::rich_text::RichBlockRecord::paragraph(2, "middle");
        middle.parent_id = Some(1);
        middle.depth = 1;
        middle.children = vec![3];
        let mut last = cditor_core::rich_text::RichBlockRecord::paragraph(3, "last");
        last.parent_id = Some(2);
        last.depth = 2;
        let mut document = cditor_core::rich_text::RichTextDocument::empty(1);
        document.root_blocks = vec![1];
        document.blocks = vec![first, middle, last];
        let mut runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
        runtime
            .set_document_text_selection(1, 0, 3, "last".len())
            .unwrap();
        let projection = runtime.projection_for_window();

        let fragments = selection_overlay_fragments(&projection);

        assert_eq!(fragments.len(), 3);
        assert!(fragments.iter().all(|fragment| !fragment.full_block));
        let root_content_left =
            BLOCK_SHELL_OUTER_PADDING_X_PX + BLOCK_GUTTER_WIDTH_PX + BLOCK_ROW_GAP_PX;
        assert_eq!(fragments[0].content_left_px, root_content_left);
        assert_eq!(
            fragments[1].content_left_px,
            root_content_left + BLOCK_INDENT_STEP_PX
        );
        assert_eq!(
            fragments[2].content_left_px,
            root_content_left + BLOCK_INDENT_STEP_PX * 2.0
        );
        assert_eq!(fragments[0].y + fragments[0].height, fragments[1].y);
        assert_eq!(fragments[1].y + fragments[1].height, fragments[2].y);
    }

    #[test]
    fn partial_cross_block_text_selection_does_not_create_stripes() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                cditor_core::rich_text::BlockPayloadRecord::rich_text(
                    1,
                    cditor_core::rich_text::RichBlockKind::Paragraph,
                    "first",
                ),
                cditor_core::rich_text::BlockPayloadRecord::rich_text(
                    2,
                    cditor_core::rich_text::RichBlockKind::Paragraph,
                    "last",
                ),
            ],
            720.0,
        );
        runtime.set_document_text_selection(1, 2, 2, 2).unwrap();

        assert!(selection_overlay_fragments(&runtime.projection_for_window()).is_empty());
    }

    #[test]
    fn single_block_text_selection_does_not_create_a_group_overlay() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
                1,
                cditor_core::rich_text::RichBlockKind::Paragraph,
                "text",
            )],
            720.0,
        );
        runtime.set_document_text_selection(1, 1, 1, 3).unwrap();

        assert!(selection_overlay_fragments(&runtime.projection_for_window()).is_empty());
    }
}
