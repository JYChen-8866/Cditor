use super::*;
use cditor_core::rich_text::{BlockPayload, ImagePayload, RichBlockKind};
use gpui::{px, size};

#[test]
fn complex_block_gutter_menu_disables_unsupported_text_and_ai_actions() {
    let runtime = DocumentRuntime::from_payloads(
        1,
        vec![cditor_core::rich_text::BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Image,
            payload: BlockPayload::Image(ImagePayload::default()),
        }],
        720.0,
    );
    let rect = ProjectedBlockRect {
        block_id: 1,
        visible_index: 0,
        depth: 0,
        document_top: 0.0,
        document_bottom: 160.0,
        indent_px: 0.0,
        text_origin_x_in_block_px: 0.0,
        text_origin_y_in_block_px: 0.0,
        text_width_px: 500.0,
        supports_children: false,
    };

    let state = formatting_toolbar_state(
        Some(&runtime),
        &HashMap::new(),
        false,
        false,
        crate::gui::menu_metrics::EditorViewport::from_size(size(px(900.0), px(700.0))),
        Some(1),
        false,
        false,
        None,
        &[rect],
        0.0,
    )
    .unwrap();

    assert!(state.show_inline_format);
    assert!(state.show_color);
    assert!(!state.inline_format_enabled);
    assert!(!state.color_enabled);
    assert!(!state.ai_enabled);
    assert!(state.delete_enabled);
}
