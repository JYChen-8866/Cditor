use crate::gui::GuiTheme;
use crate::gui::block::chrome::{BLOCK_GUTTER_WIDTH_PX, BlockChromeStyle};
use crate::gui::block::code::{V1_CODE_CONTENT_PADDING_TOP_PX, V1_CODE_CONTENT_PADDING_X_PX};
use crate::gui::document::DEFAULT_DOCUMENT_CONTENT_WIDTH_PX;
use cditor_core::block::BlockDropTarget;
use cditor_core::ids::BlockId;
use cditor_runtime::{EditorViewProjection, ViewBlockSnapshot};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::gui::app) struct FallbackViewportOrigin {
    pub(in crate::gui::app) x: f64,
    pub(in crate::gui::app) y: f64,
}

pub(in crate::gui::app) struct ProjectedBlockRect {
    pub(in crate::gui::app) block_id: BlockId,
    pub(in crate::gui::app) visible_index: usize,
    pub(in crate::gui::app) depth: usize,
    pub(in crate::gui::app) document_top: f64,
    pub(in crate::gui::app) document_bottom: f64,
    pub(in crate::gui::app) indent_px: f32,
    pub(in crate::gui::app) text_origin_x_in_block_px: f64,
    pub(in crate::gui::app) text_origin_y_in_block_px: f64,
    pub(in crate::gui::app) text_width_px: f64,
    pub(in crate::gui::app) supports_children: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gui::app) struct ParentDropTarget {
    pub(in crate::gui::app) parent_id: BlockId,
    pub(in crate::gui::app) sibling_index: usize,
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

pub(in crate::gui::app) fn parent_drop_target_from_rects(
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

pub(in crate::gui::app) fn drop_target_for_document_y_from_rects(
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

pub(in crate::gui::app) fn projected_block_rects_from_projection(
    projection: &EditorViewProjection,
) -> Vec<ProjectedBlockRect> {
    let mut top = projection.before_window_height;
    projection
        .blocks
        .iter()
        .map(|block| {
            let height = block.layout.effective_height();
            let text_metrics = fallback_text_metrics_for_block(block, GuiTheme::light());
            let rect = ProjectedBlockRect {
                block_id: block.block_id,
                visible_index: block.visible_index,
                depth: block.chrome.list_info.depth,
                document_top: top,
                document_bottom: top + height,
                indent_px: block.chrome.list_info.depth as f32
                    * crate::gui::block::chrome::BLOCK_INDENT_STEP_PX,
                text_origin_x_in_block_px: text_metrics.origin_x_in_block_px,
                text_origin_y_in_block_px: text_metrics.origin_y_in_block_px,
                text_width_px: text_metrics.width_px,
                supports_children: cditor_core::block::supports_list_children(&block.kind),
            };
            top += height;
            rect
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::gui::app) struct FallbackTextMetrics {
    pub(in crate::gui::app) origin_x_in_block_px: f64,
    pub(in crate::gui::app) origin_y_in_block_px: f64,
    pub(in crate::gui::app) width_px: f64,
}

pub(in crate::gui::app) fn fallback_text_metrics_for_block(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
) -> FallbackTextMetrics {
    let chrome = BlockChromeStyle::from_snapshot(block, theme);
    let border_left = if chrome.quote_bar.is_some() { 4.0 } else { 1.0 };
    let code_x = if matches!(
        block.kind,
        cditor_core::rich_text::RichBlockKind::Code { .. }
    ) {
        f64::from(V1_CODE_CONTENT_PADDING_X_PX)
    } else {
        0.0
    };
    let code_y = if matches!(
        block.kind,
        cditor_core::rich_text::RichBlockKind::Code { .. }
    ) {
        f64::from(V1_CODE_CONTENT_PADDING_TOP_PX)
    } else {
        0.0
    };
    let origin_x = 8.0
        + f64::from(chrome.indent_px)
        + f64::from(BLOCK_GUTTER_WIDTH_PX)
        + 8.0
        + border_left
        + f64::from(chrome.content_padding_left_px)
        + f64::from(chrome.prefix_width_px)
        + code_x;
    let origin_y = 4.0 + 1.0 + f64::from(chrome.content_padding_y_px) + code_y;
    let width = (f64::from(DEFAULT_DOCUMENT_CONTENT_WIDTH_PX)
        - origin_x
        - 8.0
        - 1.0
        - f64::from(chrome.content_padding_right_px)
        - code_x)
        .max(1.0);
    FallbackTextMetrics {
        origin_x_in_block_px: origin_x,
        origin_y_in_block_px: origin_y,
        width_px: width,
    }
}
