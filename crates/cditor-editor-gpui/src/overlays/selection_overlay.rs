use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgba};

use crate::block::chrome::BlockHorizontalGeometry;
use crate::document::{DocumentBlockGeometry, DocumentLayoutMetrics};
use crate::theme::{GuiTheme, selection_background_color};
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
    pub content_right_px: f32,
}

pub fn selection_overlay_fragments(
    projection: &EditorViewProjection,
    document_layout: DocumentLayoutMetrics,
) -> Vec<SelectionOverlayFragment> {
    let blocks = &projection.blocks;
    let mut fragments = Vec::new();
    let mut prefix_heights = Vec::with_capacity(blocks.len() + 1);
    prefix_heights.push(0.0);
    for block in blocks {
        prefix_heights
            .push(prefix_heights.last().copied().unwrap_or(0.0) + block.layout.effective_height());
    }

    let selected = |index: usize| blocks[index].selected || blocks[index].selection_overlay;
    let mut start = 0;
    while start < blocks.len() {
        if !selected(start) {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < blocks.len()
            && selected(end)
            && blocks[end].visible_index == blocks[end - 1].visible_index + 1
        {
            end += 1;
        }

        let first = &blocks[start];
        let block_geometry = DocumentBlockGeometry::for_block(first, document_layout);
        fragments.push(SelectionOverlayFragment {
            block_id: first.block_id,
            y: prefix_heights[start],
            height: prefix_heights[end] - prefix_heights[start],
            full_block: blocks[start..end].iter().all(|block| block.selected),
            content_left_px: block_geometry.shell_left_px + selection_content_left_px(0),
            content_right_px: block_geometry.track_right_px(),
        });
        start = end;
    }
    fragments
}

fn selection_content_left_px(depth: usize) -> f32 {
    BlockHorizontalGeometry::for_depth(depth).marker_lane_left_px
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
                .w(px(fragment.content_right_px - fragment.content_left_px))
                .top(px(fragment.y as f32))
                .h(px(fragment.height as f32))
                .bg(rgba(background))
        }))
        .into_any_element()
}

fn selection_overlay_background(theme: GuiTheme) -> u32 {
    (selection_background_color(theme) << 8) | 0xff
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn adjacent_selected_blocks_render_as_one_fragment() {
        let mut runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let selectable = projection
            .blocks
            .iter()
            .filter(|block| !block.kind.is_document_title())
            .collect::<Vec<_>>();
        crate::test_support::select_block_range(
            &mut runtime,
            selectable[0].block_id,
            selectable[2].block_id,
        );
        let projection = runtime.projection_for_window();

        let fragments = selection_overlay_fragments(&projection, DocumentLayoutMetrics::default());

        assert_eq!(fragments.len(), 1);
        assert!(fragments[0].full_block);
    }

    #[test]
    fn whole_cross_block_text_selection_uses_one_content_left_edge() {
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
        crate::test_support::set_document_text_selection(&mut runtime, 1, 0, 3, "last".len());
        let projection = runtime.projection_for_window();

        let fragments = selection_overlay_fragments(&projection, DocumentLayoutMetrics::default());

        assert_eq!(fragments.len(), 1);
        assert!(!fragments[0].full_block);
        let root_content_left = DocumentBlockGeometry::for_kind(
            &RichBlockKind::Paragraph,
            DocumentLayoutMetrics::default(),
        )
        .shell_left_px
            + BlockHorizontalGeometry::for_depth(0).marker_lane_left_px;
        assert_eq!(fragments[0].content_left_px, root_content_left);
    }

    #[test]
    fn selection_overlay_uses_opaque_accent_soft() {
        let theme = GuiTheme::light();

        assert_eq!(
            selection_overlay_background(theme),
            (selection_background_color(theme) << 8) | 0xff
        );
    }

    #[test]
    fn partial_cross_block_text_selection_does_not_create_stripes() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "first"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "last"),
            ],
            720.0,
        );
        crate::test_support::set_document_text_selection(&mut runtime, 1, 2, 2, 2);

        assert!(
            selection_overlay_fragments(
                &runtime.projection_for_window(),
                DocumentLayoutMetrics::default(),
            )
            .is_empty()
        );
    }
}
