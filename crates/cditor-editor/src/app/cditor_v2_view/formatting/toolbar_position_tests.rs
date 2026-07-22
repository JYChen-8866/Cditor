use super::*;
use gpui::{px, size};

#[test]
fn gutter_toolbar_opens_left_of_the_actual_gutter_and_aligns_its_top() {
    let runtime = DocumentRuntime::demo();
    let rect = ProjectedBlockRect {
        block_id: 1,
        visible_index: 0,
        depth: 1,
        document_top: 220.0,
        document_bottom: 268.0,
        indent_px: 24.0,
        text_origin_x_in_block_px: 0.0,
        text_origin_y_in_block_px: 0.0,
        text_width_px: 500.0,
        supports_children: false,
    };
    let viewport = EditorViewport::from_size(size(px(1_440.0), px(800.0)));
    let page_left = (viewport.width - DEFAULT_DOCUMENT_PAGE_WIDTH_PX) / 2.0;
    let gutter_left = page_left + block_gutter_left_px(rect.indent_px);
    let gutter_top =
        (rect.document_top - 80.0) as f32 + DEFAULT_DOCUMENT_TOP_INSET_PX + block_gutter_top_px();

    let state = formatting_toolbar_state(
        Some(&runtime),
        &HashMap::new(),
        false,
        false,
        viewport,
        Some(1),
        false,
        false,
        None,
        &[rect],
        80.0,
    )
    .unwrap();

    assert_eq!(
        (state.x, state.y),
        gutter_floating_toolbar_position(gutter_left, gutter_top, viewport.width, viewport.height,)
    );
    assert_eq!(state.x, gutter_left - 194.0 - 8.0);
    assert_eq!(state.y, gutter_top);
}
