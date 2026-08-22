use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgba};

use crate::block::chrome::BlockHorizontalGeometry;
use crate::document::{DocumentBlockGeometry, DocumentLayoutMetrics};
use crate::theme::GuiTheme;
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
    let mut fragments = Vec::new();
    // Overlay geometry is RenderWindow-local. The surface applies the single
    // f64 global -> local origin translation before converting to GPUI f32.
    let mut block_y = 0.0;
    for block in &projection.blocks {
        let height = block.layout.effective_height();
        let block_geometry = DocumentBlockGeometry::for_block(block, document_layout);
        // A cross-block selection is one contiguous document-level highlight.
        // Keep its left edge at the root content surface instead of applying
        // each list item's indentation, which would create stepped stripes
        // through nested blocks.
        let content_left = block_geometry.shell_left_px + selection_content_left_px(0);
        if block.selected || block.selection_overlay {
            fragments.push(SelectionOverlayFragment {
                block_id: block.block_id,
                y: block_y,
                height,
                full_block: block.selected,
                content_left_px: content_left,
                content_right_px: block_geometry.track_right_px(),
            });
        }
        block_y += height;
    }
    fragments
}

pub fn action_selection_overlay_fragment(
    projection: &EditorViewProjection,
    document_layout: DocumentLayoutMetrics,
    action_block_id: Option<BlockId>,
) -> Option<SelectionOverlayFragment> {
    let action_block_id = action_block_id?;
    let source_index = projection
        .blocks
        .iter()
        .position(|block| block.block_id == action_block_id)?;
    let source = &projection.blocks[source_index];
    let source_depth = source.chrome.list_info.depth;
    let subtree_end = projection.blocks[source_index + 1..]
        .iter()
        .position(|block| block.chrome.list_info.depth <= source_depth)
        .map(|offset| source_index + 1 + offset)
        .unwrap_or(projection.blocks.len());
    let y = projection.blocks[..source_index]
        .iter()
        .map(|block| block.layout.effective_height())
        .sum();
    let height = projection.blocks[source_index..subtree_end]
        .iter()
        .map(|block| block.layout.effective_height())
        .sum();
    let block_geometry = DocumentBlockGeometry::for_block(source, document_layout);

    Some(SelectionOverlayFragment {
        block_id: source.block_id,
        y,
        height,
        full_block: true,
        content_left_px: block_geometry.shell_left_px + selection_content_left_px(0),
        content_right_px: block_geometry.track_right_px(),
    })
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
    (theme.action_accent << 8) | 0x33
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn selection_overlay_uses_projection_fragments_not_entities() {
        let mut runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let first = projection.blocks.first().unwrap().block_id;
        let last = projection.blocks.last().unwrap().block_id;
        crate::test_support::select_block_range(&mut runtime, first, last);
        let mut projection = runtime.projection_for_window();
        projection.before_window_height = 20_000_000.25;

        let fragments = selection_overlay_fragments(&projection, DocumentLayoutMetrics::default());

        assert_eq!(fragments.len(), projection.blocks.len());
        assert!(fragments.iter().all(|fragment| fragment.full_block));
        assert_eq!(fragments[0].y, 0.0);
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

        assert_eq!(fragments.len(), 3);
        assert!(fragments.iter().all(|fragment| !fragment.full_block));
        let root_content_left = DocumentBlockGeometry::for_kind(
            &cditor_core::rich_text::RichBlockKind::Paragraph,
            DocumentLayoutMetrics::default(),
        )
        .shell_left_px
            + BlockHorizontalGeometry::for_depth(0).marker_lane_left_px;
        assert_eq!(fragments[0].content_left_px, root_content_left);
        assert_eq!(fragments[1].content_left_px, root_content_left);
        assert_eq!(fragments[2].content_left_px, root_content_left);
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
        crate::test_support::set_document_text_selection(&mut runtime, 1, 2, 2, 2);

        assert!(
            selection_overlay_fragments(
                &runtime.projection_for_window(),
                DocumentLayoutMetrics::default(),
            )
            .is_empty()
        );
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
        crate::test_support::set_document_text_selection(&mut runtime, 1, 1, 1, 3);

        assert!(
            selection_overlay_fragments(
                &runtime.projection_for_window(),
                DocumentLayoutMetrics::default(),
            )
            .is_empty()
        );
    }

    #[test]
    fn parent_action_selection_is_one_contiguous_subtree_fragment() {
        let mut first = cditor_core::rich_text::RichBlockRecord::paragraph(1, "parent");
        first.children = vec![2];
        let mut child = cditor_core::rich_text::RichBlockRecord::paragraph(2, "child");
        child.parent_id = Some(1);
        child.depth = 1;
        child.children = vec![3];
        let mut grandchild = cditor_core::rich_text::RichBlockRecord::paragraph(3, "grandchild");
        grandchild.parent_id = Some(2);
        grandchild.depth = 2;
        let next = cditor_core::rich_text::RichBlockRecord::paragraph(4, "next");
        let mut document = cditor_core::rich_text::RichTextDocument::empty(1);
        document.root_blocks = vec![1, 4];
        document.blocks = vec![first, child, grandchild, next];
        let runtime = DocumentRuntime::from_rich_text_document(document, 720.0);
        let projection = runtime.projection_for_window();

        let fragment = action_selection_overlay_fragment(
            &projection,
            DocumentLayoutMetrics::default(),
            Some(1),
        )
        .unwrap();
        let expected_height: f64 = projection.blocks[..3]
            .iter()
            .map(|block| block.layout.effective_height())
            .sum();

        assert_eq!(fragment.y, 0.0);
        assert_eq!(fragment.height, expected_height);
        assert_eq!(fragment.block_id, 1);
        assert_eq!(
            fragment.content_left_px,
            DocumentBlockGeometry::for_block(
                &projection.blocks[0],
                DocumentLayoutMetrics::default()
            )
            .shell_left_px
                + BlockHorizontalGeometry::for_depth(0).marker_lane_left_px
        );
    }
}
