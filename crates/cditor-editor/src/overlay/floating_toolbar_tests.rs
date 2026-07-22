use super::*;

fn toolbar_state() -> FloatingToolbarState {
    FloatingToolbarState {
        x: 0.0,
        y: 0.0,
        block_id: Some(1),
        has_text_selection: true,
        show_inline_format: true,
        show_color: true,
        show_delete: false,
        inline_format_enabled: true,
        color_enabled: true,
        ai_enabled: true,
        delete_enabled: false,
        bold: true,
        italic: false,
        underline: true,
        strike: false,
        code: false,
        block_transform: None,
        block_transform_availability: BlockTransformAvailability::default(),
        transform_menu_opens_left: false,
        transform_menu_top_offset: 0.0,
        block_transform_menu_open: false,
        text_color: ActiveColor::Default,
        background_color: ActiveColor::Default,
        color_menu_opens_left: false,
        color_menu_top_offset: 0.0,
        color_menu_height: 520.0,
        color_menu_open: false,
        last_color_action: None,
    }
}

#[test]
fn toolbar_prefers_above_selection_and_clamps_to_viewport() {
    assert_eq!(
        floating_toolbar_position(100.0, 420.0, 180.0, 444.0, 800.0, 600.0),
        (43.0, 88.0),
    );
    assert_eq!(
        floating_toolbar_position(0.0, 12.0, 20.0, 32.0, 200.0, 100.0),
        (10.0, 10.0),
    );
}

#[test]
fn left_aligned_toolbar_uses_anchor_left_and_clamps_to_viewport() {
    assert_eq!(
        left_aligned_floating_toolbar_position(140.0, 420.0, 444.0, 800.0, 600.0),
        (140.0, 88.0),
    );
    assert_eq!(
        left_aligned_floating_toolbar_position(0.0, 12.0, 32.0, 200.0, 100.0),
        (10.0, 10.0),
    );
    assert_eq!(
        left_aligned_floating_toolbar_position(760.0, 12.0, 32.0, 800.0, 600.0),
        (596.0, 40.0),
    );
}

#[test]
fn gutter_toolbar_opens_left_and_aligns_with_the_gutter_top() {
    let (x, y) = gutter_floating_toolbar_position(320.0, 140.0, 1_200.0, 800.0);

    assert_eq!(x, 118.0);
    assert_eq!(x + TOOLBAR_WIDTH_PX + TOOLBAR_ANCHOR_GAP_PX, 320.0);
    assert_eq!(y, 140.0);
}

#[test]
fn gutter_toolbar_stays_inside_the_viewport_when_left_or_bottom_space_is_tight() {
    assert_eq!(
        gutter_floating_toolbar_position(180.0, 700.0, 1_000.0, 800.0),
        (10.0, 466.0),
    );
    assert_eq!(
        gutter_floating_toolbar_position(320.0, 2.0, 1_200.0, 800.0),
        (118.0, 10.0),
    );
}

#[test]
fn toolbar_state_reports_active_and_enabled_actions_separately() {
    let state = toolbar_state();
    assert!(state.action_active(InlineFormatAction::Bold));
    assert!(!state.action_active(InlineFormatAction::Italic));
    assert!(state.action_active(InlineFormatAction::Underline));
    assert!(state.action_enabled(InlineFormatAction::Bold));

    let disabled = FloatingToolbarState {
        inline_format_enabled: false,
        ..state
    };
    assert!(!disabled.action_enabled(InlineFormatAction::Bold));
}

#[test]
fn only_gutter_toolbar_uses_click_outside_dismissal() {
    let selection_toolbar = toolbar_state();
    let gutter_toolbar = FloatingToolbarState {
        has_text_selection: false,
        show_delete: true,
        delete_enabled: true,
        ..selection_toolbar
    };

    assert!(!floating_toolbar_dismisses_on_mouse_down_out(
        selection_toolbar
    ));
    assert!(floating_toolbar_dismisses_on_mouse_down_out(gutter_toolbar));
}
