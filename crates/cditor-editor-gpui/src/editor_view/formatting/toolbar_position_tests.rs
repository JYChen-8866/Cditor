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
        gutter_left_px: 28.0,
        text_origin_x_in_block_px: 0.0,
        text_origin_y_in_block_px: 0.0,
        text_width_px: 500.0,
        supports_children: false,
        ..ProjectedBlockRect::default()
    };
    let viewport = EditorViewport::from_size(size(px(1_440.0), px(800.0)));
    let document_layout = DocumentLayoutMetrics::for_viewport(viewport.width);
    let page_left = (viewport.width - document_layout.page_width_px) / 2.0;
    let gutter_left = page_left + rect.gutter_left_px;
    let gutter_top =
        (rect.document_top - 80.0) as f32 + document_layout.top_inset_px + block_gutter_top_px();

    let mut context = formatting_toolbar_context(Some(&runtime), Some(1)).unwrap();
    context.global_scroll_top = 80.0;
    let state = formatting_toolbar_state(
        Some(&context),
        None,
        false,
        false,
        viewport,
        Some(1),
        false,
        false,
        None,
        &[rect],
    )
    .unwrap();

    assert_eq!(
        (state.x, state.y),
        gutter_floating_toolbar_position(gutter_left, gutter_top, viewport.width, viewport.height,)
    );
    assert_eq!(state.x, 10.0);
    assert_eq!(state.y, gutter_top);
}
