use crate::block::chrome::BlockChromeStyle;
use crate::document::{DocumentBlockGeometry, DocumentLayoutMetrics, DocumentTextGeometry};
use crate::menu_metrics::EditorViewport;
use crate::theme::GuiTheme;
use cditor_core::block::BlockDropTarget;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::TextAlign;
use cditor_runtime::{EditorViewProjection, ViewBlockSnapshot};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DocumentViewportOrigin {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl DocumentViewportOrigin {
    pub(crate) fn from_layout(viewport: EditorViewport, document: DocumentLayoutMetrics) -> Self {
        let page_left = ((viewport.width - document.page_width_px) / 2.0).max(0.0);
        let content_left = ((document.page_width_px - document.content_width_px) / 2.0).max(0.0);
        Self {
            x: f64::from(viewport.window_left + page_left + content_left),
            y: f64::from(viewport.window_top + document.top_inset_px),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct ProjectedBlockRect {
    pub(crate) block_id: BlockId,
    pub(crate) visible_index: usize,
    pub(crate) depth: usize,
    pub(crate) document_top: f64,
    pub(crate) document_bottom: f64,
    pub(crate) indent_px: f32,
    pub(crate) outer_padding_top_px: f32,
    pub(crate) shell_left_px: f32,
    pub(crate) track_right_px: f32,
    pub(crate) gutter_left_px: f32,
    pub(crate) text_origin_x_in_block_px: f64,
    pub(crate) text_origin_y_in_block_px: f64,
    pub(crate) text_width_px: f64,
    /// `None` is reserved for manually constructed test projections. Rendered
    /// projections always carry the block's actual text alignment.
    pub(crate) text_align: Option<TextAlign>,
    /// Code surfaces have a second, block-local vertical scroll transform.
    pub(crate) has_internal_text_scroll: bool,
    pub(crate) supports_children: bool,
}

/// The one placement contract between document projection and local text
/// geometry. Parley snapshots only understand points relative to this origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProjectedTextPlacement {
    pub(crate) window_origin_x_px: f64,
    pub(crate) window_origin_y_px: f64,
    pub(crate) wrap_width_px: f64,
    pub(crate) text_align: TextAlign,
}

/// Cell-local geometry captured from the table projection currently presented
/// by the editor. It deliberately excludes both document and table scroll
/// transforms; those are resolved at interaction time from their live truth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProjectedTableCellRect {
    pub(crate) block_id: BlockId,
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) content_version: u64,
    pub(crate) layout_version: u64,
    pub(crate) text_origin_x_in_table_px: f64,
    pub(crate) text_origin_y_in_table_px: f64,
    pub(crate) text_width_px: f64,
    pub(crate) text_align: TextAlign,
    pub(crate) header: bool,
    pub(crate) projected_horizontal_scroll_offset_px: f64,
}

impl ProjectedTextPlacement {
    pub(crate) fn for_block(
        viewport_origin: DocumentViewportOrigin,
        block: ProjectedBlockRect,
        presented_scroll_top: f64,
        internal_scroll_offset_y_px: f64,
    ) -> Self {
        Self {
            window_origin_x_px: viewport_origin.x + block.text_origin_x_in_block_px,
            window_origin_y_px: viewport_origin.y + block.document_top - presented_scroll_top
                + block.text_origin_y_in_block_px
                + internal_scroll_offset_y_px,
            wrap_width_px: block.text_width_px,
            text_align: block.text_align.unwrap_or(TextAlign::Start),
        }
    }

    pub(crate) fn local_point(self, window_x_px: f64, window_y_px: f64) -> (f64, f64) {
        (
            window_x_px - self.window_origin_x_px,
            window_y_px - self.window_origin_y_px,
        )
    }

    pub(crate) fn for_table_cell(
        viewport_origin: DocumentViewportOrigin,
        block: ProjectedBlockRect,
        cell: ProjectedTableCellRect,
        presented_scroll_top: f64,
        internal_scroll_offset_x_px: f64,
        internal_scroll_offset_y_px: f64,
    ) -> Self {
        let block = Self::for_block(viewport_origin, block, presented_scroll_top, 0.0);
        Self {
            window_origin_x_px: block.window_origin_x_px
                + cell.text_origin_x_in_table_px
                + internal_scroll_offset_x_px,
            window_origin_y_px: block.window_origin_y_px
                + cell.text_origin_y_in_table_px
                + internal_scroll_offset_y_px,
            wrap_width_px: cell.text_width_px,
            text_align: cell.text_align,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParentDropTarget {
    pub(crate) parent_id: BlockId,
    pub(crate) sibling_index: usize,
}

fn source_depth_for_rects(rects: &[ProjectedBlockRect], block_id: BlockId) -> Option<usize> {
    rects
        .iter()
        .find(|rect| rect.block_id == block_id)
        .map(|rect| rect.depth)
}

fn projected_subtree_end(
    rects: &[ProjectedBlockRect],
    source_visible_index: usize,
    source_depth: usize,
) -> usize {
    rects
        .iter()
        .filter(|rect| rect.visible_index > source_visible_index)
        .find(|rect| rect.depth <= source_depth)
        .map(|rect| rect.visible_index)
        .unwrap_or_else(|| {
            rects
                .last()
                .map(|rect| rect.visible_index + 1)
                .unwrap_or(source_visible_index + 1)
        })
}

pub(crate) fn parent_drop_target_from_rects(
    rects: &[ProjectedBlockRect],
    source_block_id: BlockId,
    target: BlockDropTarget,
) -> Option<ParentDropTarget> {
    let source = rects.iter().find(|rect| rect.block_id == source_block_id)?;
    let source_subtree_end = projected_subtree_end(rects, source.visible_index, source.depth);
    let target_position = target
        .insert_before_block_id
        .and_then(|block_id| rects.iter().position(|rect| rect.block_id == block_id))
        .unwrap_or(rects.len());
    let parent = rects.iter().take(target_position).rev().find(|rect| {
        !(rect.visible_index >= source.visible_index && rect.visible_index < source_subtree_end)
            && rect.supports_children
    })?;
    Some(ParentDropTarget {
        parent_id: parent.block_id,
        sibling_index: sibling_index_for_parent_drop_target(rects, parent, target_position),
    })
}

fn sibling_index_for_parent_drop_target(
    rects: &[ProjectedBlockRect],
    parent: &ProjectedBlockRect,
    target_position: usize,
) -> usize {
    let mut sibling_index = 0;
    for rect in rects
        .iter()
        .skip_while(|rect| rect.block_id != parent.block_id)
        .skip(1)
    {
        if rect.depth <= parent.depth {
            return usize::MAX;
        }
        if rect.depth == parent.depth + 1 {
            if rects
                .get(target_position)
                .is_some_and(|target| target.block_id == rect.block_id)
            {
                return sibling_index;
            }
            sibling_index += 1;
        }
    }
    usize::MAX
}

pub(crate) fn drop_target_for_document_y_from_rects(
    rects: &[ProjectedBlockRect],
    source_block_id: BlockId,
    document_y: f64,
) -> Option<BlockDropTarget> {
    let source = rects.iter().find(|rect| rect.block_id == source_block_id)?;
    let source_depth = source_depth_for_rects(rects, source_block_id)?;
    let source_subtree_end = projected_subtree_end(rects, source.visible_index, source_depth);
    let mut last_target = None;
    for rect in rects {
        if rect.visible_index >= source.visible_index && rect.visible_index < source_subtree_end {
            continue;
        }
        let midpoint = rect.document_top + (rect.document_bottom - rect.document_top) / 2.0;
        if document_y < midpoint {
            return Some(BlockDropTarget {
                insert_before_block_id: Some(rect.block_id),
                target_visible_index: rect.visible_index,
            });
        }
        last_target = Some(BlockDropTarget {
            insert_before_block_id: None,
            target_visible_index: rect.visible_index + 1,
        });
    }
    last_target
}

pub(crate) fn projected_block_rects_from_projection(
    projection: &EditorViewProjection,
    document_layout: DocumentLayoutMetrics,
) -> Vec<ProjectedBlockRect> {
    let mut top = projection.before_window_height;
    projection
        .blocks
        .iter()
        .map(|block| {
            let height = block.layout.effective_height();
            let chrome = BlockChromeStyle::from_snapshot(block, GuiTheme::light());
            let text_geometry =
                DocumentTextGeometry::for_block(block, GuiTheme::light(), document_layout);
            let horizontal = chrome.horizontal_geometry();
            let block_geometry = DocumentBlockGeometry::for_block(block, document_layout);
            let rect = ProjectedBlockRect {
                block_id: block.block_id,
                visible_index: block.visible_index,
                depth: block.chrome.list_info.depth,
                document_top: top,
                document_bottom: top + height,
                indent_px: horizontal.indent_px,
                outer_padding_top_px: chrome.outer_padding_top_px,
                shell_left_px: block_geometry.shell_left_px,
                track_right_px: block_geometry.track_right_px(),
                gutter_left_px: block_geometry.shell_left_px + horizontal.gutter_left_px,
                text_origin_x_in_block_px: text_geometry.origin_x_px,
                text_origin_y_in_block_px: text_geometry.origin_y_px,
                text_width_px: text_geometry.width_px,
                text_align: Some(block.attrs.text_align),
                has_internal_text_scroll: false,
                supports_children: cditor_core::block::supports_list_children(&block.kind),
            };
            top += height;
            rect
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FallbackTextMetrics {
    pub(crate) origin_x_in_block_px: f64,
    pub(crate) origin_y_in_block_px: f64,
    pub(crate) width_px: f64,
}

pub(crate) fn fallback_text_metrics_for_block(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    document_layout: DocumentLayoutMetrics,
) -> FallbackTextMetrics {
    let geometry = DocumentTextGeometry::for_block(block, theme, document_layout);
    FallbackTextMetrics {
        origin_x_in_block_px: geometry.origin_x_px,
        origin_y_in_block_px: geometry.origin_y_px,
        width_px: geometry.width_px,
    }
}

#[cfg(test)]
mod viewport_origin_tests {
    use gpui::{Bounds, point, px, size};

    use super::*;

    #[test]
    fn document_viewport_origin_includes_host_offset_centering_and_page_header_space() {
        let viewport = EditorViewport::from_measurement(
            Bounds::new(point(px(240.0), px(80.0)), size(px(1_440.0), px(900.0))),
            size(px(1_440.0), px(900.0)),
        );

        assert_eq!(
            DocumentViewportOrigin::from_layout(viewport, DocumentLayoutMetrics::default()),
            DocumentViewportOrigin { x: 312.0, y: 176.0 }
        );
    }

    #[test]
    fn narrow_document_viewport_origin_does_not_add_phantom_horizontal_inset() {
        let viewport = EditorViewport::from_measurement(
            Bounds::new(point(px(12.0), px(20.0)), size(px(700.0), px(600.0))),
            size(px(700.0), px(600.0)),
        );

        assert_eq!(
            DocumentViewportOrigin::from_layout(
                viewport,
                DocumentLayoutMetrics::for_viewport(700.0)
            ),
            DocumentViewportOrigin { x: 12.0, y: 68.0 }
        );
    }

    #[test]
    fn projected_text_placement_keeps_large_document_offsets_local_and_applies_inner_scroll() {
        let placement = ProjectedTextPlacement::for_block(
            DocumentViewportOrigin { x: 100.0, y: 40.0 },
            ProjectedBlockRect {
                document_top: 20_000_128.25,
                text_origin_x_in_block_px: 32.0,
                text_origin_y_in_block_px: 12.0,
                text_width_px: 300.0,
                text_align: Some(TextAlign::Center),
                ..ProjectedBlockRect::default()
            },
            20_000_000.25,
            -480.5,
        );

        assert_eq!(placement.window_origin_x_px, 132.0);
        assert_eq!(placement.window_origin_y_px, -300.5);
        assert_eq!(placement.wrap_width_px, 300.0);
        assert_eq!(placement.text_align, TextAlign::Center);
        assert_eq!(placement.local_point(197.5, -278.25), (65.5, 22.25));
    }

    #[test]
    fn projected_table_cell_placement_composes_document_and_fractional_table_scroll() {
        let block = ProjectedBlockRect {
            block_id: 7,
            document_top: 20_000_128.25,
            text_origin_x_in_block_px: 32.0,
            text_origin_y_in_block_px: 12.0,
            ..ProjectedBlockRect::default()
        };
        let cell = ProjectedTableCellRect {
            block_id: 7,
            row: 4,
            col: 5,
            content_version: 11,
            layout_version: 13,
            text_origin_x_in_table_px: 610.0,
            text_origin_y_in_table_px: 151.0,
            text_width_px: 180.0,
            text_align: TextAlign::End,
            header: false,
            projected_horizontal_scroll_offset_px: -480.5,
        };

        let placement = ProjectedTextPlacement::for_table_cell(
            DocumentViewportOrigin { x: 100.0, y: 40.0 },
            block,
            cell,
            20_000_000.25,
            -480.5,
            -144.25,
        );

        assert_eq!(placement.window_origin_x_px, 261.5);
        assert_eq!(placement.window_origin_y_px, 186.75);
        assert_eq!(placement.wrap_width_px, 180.0);
        assert_eq!(placement.text_align, TextAlign::End);
        assert_eq!(placement.local_point(281.75, 207.25), (20.25, 20.5));
    }
}
