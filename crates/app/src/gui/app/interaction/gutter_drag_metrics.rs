use cditor_core::block::BlockDropTarget;

use crate::gui::block::chrome::block_content_left_px;
use crate::gui::document::DEFAULT_DOCUMENT_CONTENT_WIDTH_PX;

const GUTTER_DRAG_AUTO_SCROLL_EDGE_PX: f64 = 40.0;
const GUTTER_DRAG_AUTO_SCROLL_MAX_STEP_PX: f64 = 24.0;
pub(in crate::gui::app) const GUTTER_DRAG_AUTO_SCROLL_TICK_MS: u64 = 16;
const GUTTER_DRAG_GUIDELINE_CONTENT_END_PX: f32 = DEFAULT_DOCUMENT_CONTENT_WIDTH_PX - 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::gui::app) struct GutterDragGuidelineGeometry {
    pub(in crate::gui::app) y_px: f32,
    pub(in crate::gui::app) start_x_px: f32,
    pub(in crate::gui::app) end_x_px: f32,
}

pub(in crate::gui::app) fn gutter_drag_guideline_geometry(
    rects: &[super::geometry::ProjectedBlockRect],
    target: BlockDropTarget,
) -> Option<GutterDragGuidelineGeometry> {
    let (anchor, y_px) = if let Some(block_id) = target.insert_before_block_id {
        let anchor = rects.iter().find(|rect| rect.block_id == block_id)?;
        (anchor, anchor.document_top as f32)
    } else {
        let anchor = rects
            .iter()
            .filter(|rect| rect.visible_index < target.target_visible_index)
            .max_by_key(|rect| rect.visible_index)?;
        (anchor, anchor.document_bottom as f32)
    };
    let start_x_px = block_content_left_px(anchor.indent_px);
    let end_x_px = GUTTER_DRAG_GUIDELINE_CONTENT_END_PX;
    (end_x_px > start_x_px).then_some(GutterDragGuidelineGeometry {
        y_px,
        start_x_px,
        end_x_px,
    })
}

pub(in crate::gui::app) fn gutter_drag_pointer_document_y(
    window_y: f32,
    document_viewport_origin_y: f64,
    scroll_top: f64,
) -> f32 {
    (f64::from(window_y) - document_viewport_origin_y + scroll_top) as f32
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
            text_width_px: DEFAULT_DOCUMENT_CONTENT_WIDTH_PX.into(),
            supports_children: false,
        }
    }

    #[test]
    fn gutter_drag_guideline_aligns_with_target_content_level_and_width() {
        let mut root = rect(1, 0.0, 40.0);
        root.indent_px = 0.0;
        let mut child = rect(2, 40.0, 80.0);
        child.indent_px = BLOCK_INDENT_STEP_PX;
        let rects = vec![root, child];

        assert_eq!(
            gutter_drag_guideline_geometry(
                &rects,
                BlockDropTarget {
                    insert_before_block_id: Some(1),
                    target_visible_index: 0,
                },
            ),
            Some(GutterDragGuidelineGeometry {
                y_px: 0.0,
                start_x_px: block_content_left_px(0.0),
                end_x_px: DEFAULT_DOCUMENT_CONTENT_WIDTH_PX - 8.0,
            })
        );
        assert_eq!(
            gutter_drag_guideline_geometry(
                &rects,
                BlockDropTarget {
                    insert_before_block_id: Some(2),
                    target_visible_index: 1,
                },
            ),
            Some(GutterDragGuidelineGeometry {
                y_px: 40.0,
                start_x_px: block_content_left_px(BLOCK_INDENT_STEP_PX),
                end_x_px: DEFAULT_DOCUMENT_CONTENT_WIDTH_PX - 8.0,
            })
        );
        assert!(GUTTER_DRAG_GUIDELINE_CONTENT_END_PX > block_content_left_px(BLOCK_INDENT_STEP_PX));
    }

    #[test]
    fn gutter_drag_guideline_snaps_only_to_block_top_or_bottom() {
        let rects = vec![rect(1, 100.0, 132.0), rect(2, 132.0, 164.0)];

        assert_eq!(
            gutter_drag_guideline_geometry(
                &rects,
                BlockDropTarget {
                    insert_before_block_id: Some(2),
                    target_visible_index: 1,
                },
            ),
            Some(GutterDragGuidelineGeometry {
                y_px: 132.0,
                start_x_px: block_content_left_px(0.0),
                end_x_px: DEFAULT_DOCUMENT_CONTENT_WIDTH_PX - 8.0,
            }),
        );
        assert_eq!(
            gutter_drag_guideline_geometry(
                &rects,
                BlockDropTarget {
                    insert_before_block_id: None,
                    target_visible_index: 2,
                },
            ),
            Some(GutterDragGuidelineGeometry {
                y_px: 164.0,
                start_x_px: block_content_left_px(0.0),
                end_x_px: DEFAULT_DOCUMENT_CONTENT_WIDTH_PX - 8.0,
            }),
        );
    }

    #[test]
    fn gutter_drag_guideline_after_target_uses_preceding_visible_block() {
        let mut source = rect(3, 164.0, 196.0);
        source.visible_index = 2;
        source.indent_px = BLOCK_INDENT_STEP_PX;
        let rects = vec![rect(1, 100.0, 132.0), rect(2, 132.0, 164.0), source];

        assert_eq!(
            gutter_drag_guideline_geometry(
                &rects,
                BlockDropTarget {
                    insert_before_block_id: None,
                    target_visible_index: 2,
                },
            ),
            Some(GutterDragGuidelineGeometry {
                y_px: 164.0,
                start_x_px: block_content_left_px(0.0),
                end_x_px: DEFAULT_DOCUMENT_CONTENT_WIDTH_PX - 8.0,
            }),
        );
    }

    #[test]
    fn gutter_drag_guideline_is_hidden_without_a_resolvable_block_boundary() {
        let rects = vec![rect(1, 100.0, 132.0)];

        assert_eq!(
            gutter_drag_guideline_geometry(
                &rects,
                BlockDropTarget {
                    insert_before_block_id: Some(99),
                    target_visible_index: 0,
                },
            ),
            None,
        );
        assert_eq!(
            gutter_drag_guideline_geometry(
                &rects,
                BlockDropTarget {
                    insert_before_block_id: None,
                    target_visible_index: 0,
                },
            ),
            None,
        );
    }

    #[test]
    fn gutter_drag_pointer_document_y_removes_window_origin() {
        assert_eq!(gutter_drag_pointer_document_y(235.0, 112.0, 0.0), 123.0);
        assert_eq!(gutter_drag_pointer_document_y(235.0, 112.0, 80.0), 203.0);
    }
}
