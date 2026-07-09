use cditor_core::block::BlockDropTarget;

use crate::gui::block::chrome::BLOCK_GUTTER_WIDTH_PX;
use crate::gui::document::DEFAULT_DOCUMENT_PAGE_WIDTH_PX;

const GUTTER_DRAG_AUTO_SCROLL_EDGE_PX: f64 = 40.0;
const GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX: f64 = 24.0;
pub(in crate::gui::app) const GUTTER_DRAG_AUTO_SCROLL_TICK_MS: u64 = 16;
const GUTTER_DRAG_GUIDELINE_CONTENT_START_BASE_PX: f32 = 8.0 + BLOCK_GUTTER_WIDTH_PX + 8.0;
const GUTTER_DRAG_GUIDELINE_CONTENT_END_PX: f32 = DEFAULT_DOCUMENT_PAGE_WIDTH_PX - 8.0;

pub(in crate::gui::app) fn gutter_drag_guideline_y_px(
    _rects: &[super::geometry::ProjectedBlockRect],
    _target: Option<BlockDropTarget>,
    pointer_document_y: f32,
) -> f32 {
    pointer_document_y
}

pub(in crate::gui::app) fn gutter_drag_pointer_document_y(
    window_y: f32,
    document_viewport_origin_y: f64,
    scroll_top: f64,
) -> f32 {
    (f64::from(window_y) - document_viewport_origin_y + scroll_top) as f32
}

pub(in crate::gui::app) fn gutter_drag_guideline_start_x_px(
    rects: &[super::geometry::ProjectedBlockRect],
    target: Option<BlockDropTarget>,
) -> f32 {
    let indent_px = target
        .and_then(|target| {
            if let Some(block_id) = target.insert_before_block_id {
                rects
                    .iter()
                    .find(|rect| rect.block_id == block_id)
                    .map(|rect| rect.indent_px)
            } else {
                rects.last().map(|rect| rect.indent_px)
            }
        })
        .unwrap_or(0.0);
    GUTTER_DRAG_GUIDELINE_CONTENT_START_BASE_PX + indent_px
}

pub(in crate::gui::app) fn gutter_drag_guideline_end_x_px() -> f32 {
    GUTTER_DRAG_GUIDELINE_CONTENT_END_PX
}

pub(in crate::gui::app) fn gutter_drag_auto_scroll_delta(
    pointer_y: f64,
    viewport_height: f64,
) -> f64 {
    if viewport_height <= GUTTER_DRAG_AUTO_SCROLL_EDGE_PX * 2.0 {
        return 0.0;
    }
    if pointer_y < GUTTER_DRAG_AUTO_SCROLL_EDGE_PX {
        -((GUTTER_DRAG_AUTO_SCROLL_EDGE_PX - pointer_y) / GUTTER_DRAG_AUTO_SCROLL_EDGE_PX)
            .clamp(0.0, 1.0)
            * GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX
    } else if pointer_y > viewport_height - GUTTER_DRAG_AUTO_SCROLL_EDGE_PX {
        ((pointer_y - (viewport_height - GUTTER_DRAG_AUTO_SCROLL_EDGE_PX))
            / GUTTER_DRAG_AUTO_SCROLL_EDGE_PX)
            .clamp(0.0, 1.0)
            * GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::app::interaction::geometry::ProjectedBlockRect;
    use crate::gui::block::chrome::BLOCK_INDENT_STEP_PX;
    use cditor_core::ids::BlockId;

    fn rect(block_id: BlockId, top: f64, bottom: f64) -> ProjectedBlockRect {
        ProjectedBlockRect {
            block_id,
            visible_index: block_id as usize - 1,
            depth: 0,
            document_top: top,
            document_bottom: bottom,
            indent_px: 0.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: false,
        }
    }

    #[test]
    fn gutter_drag_guideline_start_aligns_with_target_content_level() {
        let mut root = rect(1, 0.0, 40.0);
        root.indent_px = 0.0;
        let mut child = rect(2, 40.0, 80.0);
        child.indent_px = BLOCK_INDENT_STEP_PX;
        let rects = vec![root, child];

        assert_eq!(
            gutter_drag_guideline_start_x_px(
                &rects,
                Some(BlockDropTarget {
                    insert_before_block_id: Some(1),
                    target_visible_index: 0,
                }),
            ),
            GUTTER_DRAG_GUIDELINE_CONTENT_START_BASE_PX
        );
        assert_eq!(
            gutter_drag_guideline_start_x_px(
                &rects,
                Some(BlockDropTarget {
                    insert_before_block_id: Some(2),
                    target_visible_index: 1,
                }),
            ),
            GUTTER_DRAG_GUIDELINE_CONTENT_START_BASE_PX + BLOCK_INDENT_STEP_PX
        );
        assert_eq!(
            GUTTER_DRAG_GUIDELINE_CONTENT_START_BASE_PX,
            8.0 + BLOCK_GUTTER_WIDTH_PX + 8.0
        );
        assert_eq!(
            gutter_drag_guideline_end_x_px(),
            DEFAULT_DOCUMENT_PAGE_WIDTH_PX - 8.0
        );
        assert!(
            gutter_drag_guideline_end_x_px()
                > GUTTER_DRAG_GUIDELINE_CONTENT_START_BASE_PX + BLOCK_INDENT_STEP_PX
        );
    }

    #[test]
    fn gutter_drag_guideline_tracks_pointer_document_y() {
        let rects = vec![rect(1, 100.0, 132.0), rect(2, 132.0, 164.0)];

        assert_eq!(
            gutter_drag_guideline_y_px(
                &rects,
                Some(BlockDropTarget {
                    insert_before_block_id: Some(2),
                    target_visible_index: 1,
                }),
                12.0,
            ),
            12.0,
        );
        assert_eq!(
            gutter_drag_guideline_y_px(
                &rects,
                Some(BlockDropTarget {
                    insert_before_block_id: None,
                    target_visible_index: 2,
                }),
                150.0,
            ),
            150.0,
        );
        assert_eq!(
            gutter_drag_guideline_y_px(
                &rects,
                Some(BlockDropTarget {
                    insert_before_block_id: None,
                    target_visible_index: 2,
                }),
                220.0,
            ),
            220.0,
        );
    }

    #[test]
    fn gutter_drag_pointer_document_y_removes_window_origin() {
        assert_eq!(gutter_drag_pointer_document_y(235.0, 112.0, 0.0), 123.0);
        assert_eq!(gutter_drag_pointer_document_y(235.0, 112.0, 80.0), 203.0);
    }
}
