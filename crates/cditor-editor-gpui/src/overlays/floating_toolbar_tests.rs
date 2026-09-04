use super::*;

#[test]
fn gutter_format_controls_use_the_provided_svg_assets() {
    assert!(std::str::from_utf8(ICON_COLOR).unwrap().starts_with("<svg"));
    assert!(std::str::from_utf8(ICON_TEXT).unwrap().starts_with("<svg"));
    assert!(std::str::from_utf8(ICON_COPY).unwrap().starts_with("<svg"));
    assert!(
        std::str::from_utf8(ICON_DELETE)
            .unwrap()
            .starts_with("<svg")
    );
    assert!(
        std::str::from_utf8(ICON_SUBMENU_ARROW)
            .unwrap()
            .starts_with("<svg")
    );
    for action in [
        InlineFormatAction::Bold,
        InlineFormatAction::Italic,
        InlineFormatAction::Underline,
        InlineFormatAction::Strike,
        InlineFormatAction::Code,
    ] {
        let (_, source) = format_icon_source(action).expect("provided formatting SVG is mapped");
        assert!(std::str::from_utf8(source).unwrap().starts_with("<svg"));
    }
    assert_eq!(FORMAT_ICON_SIZE_PX, 20.0);
    assert_eq!(GUTTER_FORMAT_ROW_PADDING_PX, 8.0);
    assert_eq!(FORMAT_BUTTON_SIZE_PX, 30.0);
    assert_eq!(GUTTER_FORMAT_BUTTON_SIZE_PX, 36.0);
    assert_eq!(TOOLBAR_GROUP_LABEL_HEIGHT_PX, 26.0);
    assert_eq!(POPUP_MENU_ITEM_FONT_SIZE_PX, 14.0);
    assert_eq!(GUTTER_MENU_LABEL_FONT_SIZE_PX, 12.0);
}

#[test]
fn gutter_menu_groups_format_color_and_delete_under_actions() {
    assert_eq!(GUTTER_GROUP_LABELS, ["文字样式", "操作"]);
}

fn toolbar_state() -> FloatingToolbarState {
    FloatingToolbarState {
        x: 0.0,
        y: 0.0,
        block_id: Some(1),
        text_selection: None,
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
        link: false,
        block_transform: None,
        callout_variant: None,
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
        copy_menu_open: false,
        last_color_action: None,
    }
}

#[test]
fn floating_toolbar_width_tracks_visible_control_groups() {
    let state = toolbar_state();
    assert_eq!(toolbar_content_width(state), 306.0);

    let color_only = FloatingToolbarState {
        show_inline_format: false,
        ..state
    };
    assert_eq!(toolbar_content_width(color_only), 102.0);

    let format_only = FloatingToolbarState {
        show_color: false,
        ..state
    };
    assert_eq!(toolbar_content_width(format_only), 216.0);
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

    assert_eq!(x, 10.0);
    assert!(x + GUTTER_MENU_WIDTH_PX >= 320.0);
    assert_eq!(y, 140.0);
}

#[test]
fn gutter_toolbar_stays_inside_the_viewport_when_left_or_bottom_space_is_tight() {
    assert_eq!(
        gutter_floating_toolbar_position(180.0, 700.0, 1_000.0, 800.0),
        (10.0, 364.0),
    );
    assert_eq!(
        gutter_floating_toolbar_position(320.0, 2.0, 1_200.0, 800.0),
        (10.0, 10.0),
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

#[test]
fn ai_actions_use_a_real_scroll_range_for_all_commands() {
    let content_height = AI_ACTION_COUNT as f32 * AI_ACTION_ROW_HEIGHT_PX;

    assert_eq!(AI_ACTION_COUNT, 6);
    assert!(content_height > AI_ACTIONS_VIEWPORT_HEIGHT_PX);
    assert_eq!(content_height, 216.0);
    assert_eq!(AI_ACTION_ROW_HEIGHT_PX, 36.0);
}

#[test]
fn ai_actions_use_provided_svg_icons_and_shared_row_metrics() {
    let icons = [
        ICON_AI_IMPROVE,
        ICON_AI_PROOFREAD,
        ICON_AI_SHORTEN,
        ICON_AI_EXPAND,
        ICON_AI_EXPLAIN,
        ICON_AI_TRANSLATE,
    ];
    for icon in icons {
        assert!(std::str::from_utf8(icon).unwrap().starts_with("<svg"));
    }
    assert_eq!(
        AI_ACTION_ICON_KEYS
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        AI_ACTION_COUNT
    );
    assert_eq!(
        icons
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        AI_ACTION_COUNT
    );
    assert_eq!(AI_ACTION_ROW_HEIGHT_PX, 36.0);
    assert_eq!(FORMAT_ICON_SIZE_PX, 20.0);
}
