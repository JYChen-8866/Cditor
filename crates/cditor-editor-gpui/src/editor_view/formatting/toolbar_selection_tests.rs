use super::*;
use cditor_core::rich_text::RichBlockKind;
use gpui::{Bounds, point, px};

#[test]
fn cross_block_text_selection_keeps_unsupported_actions_visible_but_disabled() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            cditor_core::rich_text::BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "first",
            ),
            cditor_core::rich_text::BlockPayloadRecord::rich_text(
                2,
                RichBlockKind::Paragraph,
                "second",
            ),
        ],
        720.0,
    );
    crate::test_support::set_document_text_selection(&mut runtime, 1, 1, 2, 2);
    let viewport = EditorViewport {
        window_left: 240.0,
        window_top: 80.0,
        width: 900.0,
        height: 700.0,
    };
    let selection_bounds = Bounds::from_corners(
        point(
            px(viewport.window_left + 120.0),
            px(viewport.window_top + 100.0),
        ),
        point(
            px(viewport.window_left + 620.0),
            px(viewport.window_top + 148.0),
        ),
    );

    let context = formatting_toolbar_context(Some(&runtime), None).unwrap();
    let state = formatting_toolbar_state(
        Some(&context),
        Some(selection_bounds),
        false,
        false,
        viewport,
        None,
        false,
        false,
        None,
        &[],
    )
    .expect("cross-block selection should show a toolbar");

    assert!(state.has_text_selection);
    assert!(state.show_inline_format);
    assert!(state.show_color);
    assert!(!state.inline_format_enabled);
    assert!(!state.color_enabled);
    assert!(state.ai_enabled);
    assert!(!state.show_delete);
    assert_eq!(state.block_id, Some(2));
    assert_eq!(state.y, 156.0);
    assert!((0.0..viewport.width).contains(&state.x));
}
