use crate::gui::app::input::mouse::scroll_delta_y;
use crate::gui::app::interaction::geometry::{
    ParentDropTarget, ProjectedBlockRect, parent_drop_target_from_rects,
};
use cditor_core::block::BlockDropTarget;
use gpui::{ScrollDelta, ScrollWheelEvent};

#[test]
fn parent_drop_target_computes_direct_child_sibling_index() {
    let rects = vec![
        ProjectedBlockRect {
            block_id: 10,
            visible_index: 0,
            depth: 0,
            document_top: 0.0,
            document_bottom: 40.0,
            indent_px: 0.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: true,
        },
        ProjectedBlockRect {
            block_id: 11,
            visible_index: 1,
            depth: 1,
            document_top: 40.0,
            document_bottom: 80.0,
            indent_px: 24.0,
            text_origin_x_in_block_px: 24.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 836.0,
            supports_children: false,
        },
        ProjectedBlockRect {
            block_id: 12,
            visible_index: 2,
            depth: 1,
            document_top: 80.0,
            document_bottom: 120.0,
            indent_px: 24.0,
            text_origin_x_in_block_px: 24.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 836.0,
            supports_children: false,
        },
        ProjectedBlockRect {
            block_id: 20,
            visible_index: 3,
            depth: 0,
            document_top: 120.0,
            document_bottom: 160.0,
            indent_px: 0.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: false,
        },
    ];

    assert_eq!(
        parent_drop_target_from_rects(
            &rects,
            20,
            BlockDropTarget {
                insert_before_block_id: Some(12),
                target_visible_index: 2,
            },
        ),
        Some(ParentDropTarget {
            parent_id: 10,
            sibling_index: 1,
        })
    );
}

#[test]
fn gui_scroll_delta_pixels_and_lines_are_normalized() {
    let pixel_event = ScrollWheelEvent {
        position: gpui::point(gpui::px(0.0), gpui::px(0.0)),
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(42.0))),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    };
    let line_event = ScrollWheelEvent {
        position: gpui::point(gpui::px(0.0), gpui::px(0.0)),
        delta: ScrollDelta::Lines(gpui::point(0.0, 3.0)),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    };

    assert_eq!(scroll_delta_y(&pixel_event), -42.0);
    assert_eq!(scroll_delta_y(&line_event), -48.0);
}
