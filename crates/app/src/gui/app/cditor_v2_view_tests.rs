use super::*;
use crate::gui::GuiTheme;
use crate::gui::app::GuiPlatformInputTarget;
use crate::gui::app::input::ime::{
    code_language_input_target_allows, platform_input_fallback_range, platform_input_target_allows,
    platform_selected_text_range,
};
use crate::gui::app::input::keyboard::ensure_runtime_focus_for_insert_char;
use crate::gui::app::input::mouse::scroll_delta_y;
use crate::gui::app::interaction::geometry::{
    ParentDropTarget, drop_target_for_document_y_from_rects, fallback_text_metrics_for_block,
    parent_drop_target_from_rects,
};
use crate::gui::app::interaction::gutter_drag_metrics::gutter_drag_auto_scroll_delta;
use crate::gui::block::code::{V1_CODE_CONTENT_PADDING_TOP_PX, V1_CODE_CONTENT_PADDING_X_PX};
use cditor_core::block::BlockDropTarget;
use gpui::{ScrollDelta, ScrollWheelEvent};

#[test]
fn save_status_for_mode_respects_readonly() {
    assert_eq!(save_status_for_mode(false), EditorSaveStatus::Clean);
    assert_eq!(save_status_for_mode(true), EditorSaveStatus::Readonly);
}

#[test]
fn cditor_view_state_can_swap_from_loading_to_ready_or_failed() {
    let mut state = CditorViewState::Loading {
        message: "loading".to_owned(),
    };

    assert!(state.is_loading());
    state.apply_loaded_runtime(DocumentRuntime::demo());
    assert!(state.is_ready());
    state.apply_load_failed("network error");
    assert!(state.is_load_failed());
}

#[test]
fn insert_char_focus_helper_preserves_existing_middle_caret() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
            1,
            cditor_core::rich_text::RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 3).unwrap();

    ensure_runtime_focus_for_insert_char(&mut runtime);

    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some(3));
}

#[test]
fn insert_char_focus_helper_falls_back_only_when_unfocused() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
            1,
            cditor_core::rich_text::RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );

    ensure_runtime_focus_for_insert_char(&mut runtime);

    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some("abcdef".len()));
}

#[test]
fn missed_hit_test_focus_offset_never_jumps_new_block_to_end() {
    assert_eq!(
        block_focus_offset_after_missed_hit_test(Some(1), 1, Some(3)),
        3
    );
    assert_eq!(
        block_focus_offset_after_missed_hit_test(Some(1), 1, None),
        0
    );
    assert_eq!(
        block_focus_offset_after_missed_hit_test(Some(1), 2, Some(9)),
        0
    );
    assert_eq!(
        block_focus_offset_after_missed_hit_test(None, 2, Some(9)),
        0
    );
}

#[test]
fn platform_input_fallback_prefers_active_composition_base_range_over_caret() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
            1,
            cditor_core::rich_text::RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 3).unwrap();
    runtime
        .begin_or_update_composition_with_selection(1, 3..3, "你", Some("你".len().."你".len()))
        .unwrap();
    assert_eq!(runtime.caret_offset_for_block(1), Some("abc你".len()));

    let fallback = platform_input_fallback_range(&runtime, 1);

    assert_eq!(fallback, 3..3);
}

#[test]
fn platform_input_fallback_uses_table_cell_offset() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![cditor_core::rich_text::BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: cditor_core::rich_text::RichBlockKind::Table,
            payload: cditor_core::rich_text::BlockPayload::Table(
                cditor_core::rich_text::TablePayload {
                    rows: vec![cditor_core::rich_text::TableRowPayload {
                        cells: vec![cditor_core::rich_text::TableCellPayload::plain("abcd")],
                        height: Default::default(),
                    }],
                    columns: Vec::new(),
                    header_rows: 0,
                    header_cols: 0,
                    header_style: Default::default(),
                },
            ),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 2).unwrap();

    let fallback = platform_input_fallback_range(&runtime, 1);
    let selection = platform_selected_text_range(&runtime).unwrap();

    assert_eq!(fallback, 2..2);
    assert_eq!(selection.range, 2..2);
}

#[test]
fn platform_input_fallback_prefers_session_selection_over_legacy_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
            1,
            cditor_core::rich_text::RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime.set_document_text_selection(1, 4, 1, 5).unwrap();
    runtime.focus_block_at_offset(1, 2).unwrap();

    let fallback = platform_input_fallback_range(&runtime, 1);
    let selection = platform_selected_text_range(&runtime).unwrap();

    assert_eq!(fallback, 2..2);
    assert_eq!(selection.range, 2..2);
}

#[test]
fn platform_input_target_guard_rejects_stale_registered_cell() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![cditor_core::rich_text::BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: cditor_core::rich_text::RichBlockKind::Table,
            payload: cditor_core::rich_text::BlockPayload::Table(
                cditor_core::rich_text::TablePayload {
                    rows: vec![cditor_core::rich_text::TableRowPayload {
                        cells: vec![
                            cditor_core::rich_text::TableCellPayload::plain("left"),
                            cditor_core::rich_text::TableCellPayload::plain("right"),
                        ],
                        height: Default::default(),
                    }],
                    columns: Vec::new(),
                    header_rows: 0,
                    header_cols: 0,
                    header_style: Default::default(),
                },
            ),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 1, 2).unwrap();

    assert!(platform_input_target_allows(
        Some(GuiPlatformInputTarget::TableCell {
            block_id: 1,
            row: 0,
            col: 1
        }),
        &runtime
    ));
    assert!(!platform_input_target_allows(
        Some(GuiPlatformInputTarget::TableCell {
            block_id: 1,
            row: 0,
            col: 0
        }),
        &runtime
    ));
}

#[test]
fn platform_input_registration_rejects_second_or_mismatched_target() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            cditor_core::rich_text::BlockPayloadRecord::rich_text(
                1,
                cditor_core::rich_text::RichBlockKind::Paragraph,
                "body",
            ),
            cditor_core::rich_text::BlockPayloadRecord {
                block_id: 2,
                content_version: 1,
                kind: cditor_core::rich_text::RichBlockKind::Table,
                payload: cditor_core::rich_text::BlockPayload::Table(
                    cditor_core::rich_text::TablePayload {
                        rows: vec![cditor_core::rich_text::TableRowPayload {
                            cells: vec![cditor_core::rich_text::TableCellPayload::plain("cell")],
                            height: Default::default(),
                        }],
                        columns: Vec::new(),
                        header_rows: 0,
                        header_cols: 0,
                        header_style: Default::default(),
                    },
                ),
            },
        ],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    assert!(platform_input_registration_allows(
        None,
        GuiPlatformInputTarget::BlockText { block_id: 1 },
        &runtime
    ));
    assert!(!platform_input_registration_allows(
        Some(GuiPlatformInputTarget::code_language(1)),
        GuiPlatformInputTarget::BlockText { block_id: 1 },
        &runtime
    ));
    assert!(!platform_input_registration_allows(
        None,
        GuiPlatformInputTarget::TableCell {
            block_id: 2,
            row: 0,
            col: 0,
        },
        &runtime
    ));
}

#[test]
fn code_language_input_target_guard_rejects_stale_document_targets() {
    assert!(code_language_input_target_allows(
        Some(GuiPlatformInputTarget::code_language(7)),
        7
    ));
    assert!(code_language_input_target_allows(None, 7));
    assert!(!code_language_input_target_allows(
        Some(GuiPlatformInputTarget::code_language(8)),
        7
    ));
    assert!(!code_language_input_target_allows(
        Some(GuiPlatformInputTarget::BlockText { block_id: 7 }),
        7
    ));
    assert!(!code_language_input_target_allows(
        Some(GuiPlatformInputTarget::TableCell {
            block_id: 7,
            row: 0,
            col: 0,
        }),
        7
    ));
}

#[test]
fn gui_platform_input_target_covers_runtime_and_toolbar_targets() {
    assert_eq!(
        GuiPlatformInputTarget::from_runtime_target(cditor_runtime::InputTarget::BlockText {
            block_id: 1,
        }),
        GuiPlatformInputTarget::BlockText { block_id: 1 }
    );
    assert_eq!(
        GuiPlatformInputTarget::from_runtime_target(cditor_runtime::InputTarget::TableCell {
            block_id: 2,
            row: 3,
            col: 4,
        }),
        GuiPlatformInputTarget::TableCell {
            block_id: 2,
            row: 3,
            col: 4,
        }
    );

    let code_language = GuiPlatformInputTarget::code_language(5);
    assert_eq!(code_language.block_id(), 5);
    assert!(code_language.is_code_language_for(5));
    assert!(!code_language.is_code_language_for(6));
    assert!(!GuiPlatformInputTarget::BlockText { block_id: 5 }.is_code_language_for(5));
}

#[test]
fn platform_selected_text_range_prefers_ime_selected_subrange() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
            1,
            cditor_core::rich_text::RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime
        .begin_or_update_composition_with_selection(1, 2..2, "你好", Some("你".len().."你好".len()))
        .unwrap();

    let selection = platform_selected_text_range(&runtime).unwrap();

    assert_eq!(selection.range, 3..4);
    assert!(!selection.reversed);
}

#[test]
fn platform_selected_text_range_uses_marked_end_when_ime_has_no_subrange() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
            1,
            cditor_core::rich_text::RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime
        .begin_or_update_composition_with_selection(1, 2..2, "你好", None)
        .unwrap();

    let selection = platform_selected_text_range(&runtime).unwrap();

    assert_eq!(selection.range, 4..4);
    assert!(!selection.reversed);
}

fn fallback_snapshot(
    kind: cditor_core::rich_text::RichBlockKind,
    chrome: cditor_core::block::BlockChromeSnapshot,
) -> cditor_runtime::ViewBlockSnapshot {
    cditor_runtime::ViewBlockSnapshot {
        block_id: 1,
        visible_index: 0,
        depth: chrome.list_info.depth as u16,
        chrome,
        kind,
        attrs: cditor_core::rich_text::BlockAttrs::default(),
        payload: cditor_core::rich_text::BlockPayloadView::Placeholder {
            estimated_height: 32.0,
        },
        layout: cditor_core::layout::BlockLayoutMeta::new(1, 32.0),
        selected: false,
        selection_range: None,
        selection_overlay: false,
        focused: false,
        caret_offset: None,
        marked_range: None,
        table_view: None,
        focused_table_cell: None,
        focused_table_cell_offset: None,
        pinned: false,
        placeholder: false,
    }
}

#[test]
fn fallback_text_metrics_include_list_prefix_and_indent() {
    let list_block = fallback_snapshot(
        cditor_core::rich_text::RichBlockKind::BulletedList,
        cditor_core::block::BlockChromeSnapshot {
            list_info: cditor_core::block::BlockListInfo::with_depth(2),
            prefix: cditor_core::block::BlockPrefixSnapshot::Bullet { depth: 2 },
            has_children: false,
            collapsed: false,
        },
    );

    let metrics = fallback_text_metrics_for_block(&list_block, GuiTheme::light());

    assert!(
        metrics.origin_x_in_block_px
            >= 8.0
                + 48.0
                + 24.0
                + 8.0
                + f64::from(crate::gui::block::chrome::BLOCK_PREFIX_WIDTH_PX)
    );
    assert!(metrics.width_px > 0.0);
}

#[test]
fn fallback_text_metrics_include_v1_code_content_padding() {
    let code_block = fallback_snapshot(
        cditor_core::rich_text::RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        cditor_core::block::BlockChromeSnapshot::plain(),
    );
    let paragraph = fallback_snapshot(
        cditor_core::rich_text::RichBlockKind::Paragraph,
        cditor_core::block::BlockChromeSnapshot::plain(),
    );

    let code = fallback_text_metrics_for_block(&code_block, GuiTheme::light());
    let paragraph = fallback_text_metrics_for_block(&paragraph, GuiTheme::light());

    assert_eq!(
        code.origin_y_in_block_px,
        4.0 + 1.0 + f64::from(V1_CODE_CONTENT_PADDING_TOP_PX)
    );
    assert!(
        code.origin_x_in_block_px
            >= paragraph.origin_x_in_block_px + f64::from(V1_CODE_CONTENT_PADDING_X_PX)
    );
}

#[test]
fn gutter_drag_auto_scroll_delta_only_triggers_near_edges() {
    assert_eq!(gutter_drag_auto_scroll_delta(100.0, 400.0), 0.0);
    assert_eq!(gutter_drag_auto_scroll_delta(20.0, 400.0), -12.0);
    assert_eq!(gutter_drag_auto_scroll_delta(380.0, 400.0), 12.0);
    assert_eq!(gutter_drag_auto_scroll_delta(0.0, 400.0), -24.0);
    assert_eq!(gutter_drag_auto_scroll_delta(400.0, 400.0), 24.0);
    assert_eq!(gutter_drag_auto_scroll_delta(10.0, 60.0), 0.0);
}

#[test]
fn gutter_drag_drop_target_uses_midpoints_and_skips_source_subtree() {
    let rects = vec![
        ProjectedBlockRect {
            block_id: 1,
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
            block_id: 2,
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
            block_id: 3,
            visible_index: 2,
            depth: 0,
            document_top: 80.0,
            document_bottom: 120.0,
            indent_px: 0.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: false,
        },
    ];

    assert_eq!(
        drop_target_for_document_y_from_rects(&rects, 1, 10.0),
        Some(BlockDropTarget {
            insert_before_block_id: Some(3),
            target_visible_index: 2,
        })
    );
    assert_eq!(
        drop_target_for_document_y_from_rects(&rects, 1, 140.0),
        Some(BlockDropTarget {
            insert_before_block_id: None,
            target_visible_index: 3,
        })
    );
}

#[test]
fn parent_drop_target_uses_previous_supported_block_outside_source_subtree() {
    let rects = vec![
        ProjectedBlockRect {
            block_id: 1,
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
            block_id: 2,
            visible_index: 1,
            depth: 1,
            document_top: 40.0,
            document_bottom: 80.0,
            indent_px: 24.0,
            text_origin_x_in_block_px: 24.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 836.0,
            supports_children: true,
        },
        ProjectedBlockRect {
            block_id: 3,
            visible_index: 2,
            depth: 0,
            document_top: 80.0,
            document_bottom: 120.0,
            indent_px: 0.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: false,
        },
        ProjectedBlockRect {
            block_id: 4,
            visible_index: 3,
            depth: 0,
            document_top: 120.0,
            document_bottom: 160.0,
            indent_px: 0.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: true,
        },
    ];

    assert_eq!(
        parent_drop_target_from_rects(
            &rects,
            1,
            BlockDropTarget {
                insert_before_block_id: Some(4),
                target_visible_index: 3,
            },
        ),
        None
    );
    assert_eq!(
        parent_drop_target_from_rects(
            &rects,
            3,
            BlockDropTarget {
                insert_before_block_id: Some(4),
                target_visible_index: 3,
            },
        ),
        Some(ParentDropTarget {
            parent_id: 2,
            sibling_index: usize::MAX,
        })
    );
}

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
