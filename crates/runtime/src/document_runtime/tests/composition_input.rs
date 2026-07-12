use super::*;

#[test]
fn document_text_selection_updates_input_session_selection_truth() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    runtime.set_document_text_selection(1, 5, 1, 2).unwrap();

    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, 2..5);
    assert!(editing.selection_reversed);
    assert_eq!(runtime.input_session_selected_range(), Some(2..5));
}

#[test]
fn caret_movement_updates_input_session_selection_truth() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 3).unwrap();

    runtime.move_caret_right(true).unwrap();

    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.selected_range, 3..4);
    assert!(!editing.selection_reversed);

    runtime.move_caret_left(false).unwrap();

    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.selected_range, 3..3);
    assert!(!editing.selection_reversed);
}

#[test]
fn stale_input_session_content_version_is_rejected() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime
        .begin_or_update_composition_with_selection(1, 2..2, "你", Some("你".len().."你".len()))
        .unwrap();
    runtime
        .payload_window
        .payloads
        .get_mut(&1)
        .unwrap()
        .content_version += 1;

    assert_eq!(runtime.input_session_target(), None);
    assert_eq!(runtime.input_session_selected_range(), None);
    assert_eq!(runtime.input_session_marked_range(), None);
    assert_eq!(runtime.active_composition(), None);
    assert_eq!(runtime.focused_text_for_platform_input(), None);
}

#[test]
fn insert_char_uses_caret_offset() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.set_caret_offset(1, 2).unwrap();

    runtime.insert_char('X').unwrap();

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abXcd");
    assert_eq!(projection.blocks[0].caret_offset, Some(3));
}

#[test]
fn composition_preview_does_not_commit_until_commit() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "ab",
        )],
        720.0,
    );
    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "a中b");
    assert_eq!(projection.blocks[0].marked_range, Some(1.."a中".len()));
    assert_eq!(projection.blocks[0].caret_offset, Some("a中".len()));
    assert_eq!(runtime.composition_preview_text().as_deref(), Some("a中b"));
    assert_eq!(runtime.focused_text_for_platform_input().unwrap().1, "a中b");
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(1.."a中".len())
    );
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, "a中".len().."a中".len());
    assert_eq!(editing.marked_range, Some(1.."a中".len()));
    assert_eq!(
        runtime
            .editing
            .as_ref()
            .unwrap()
            .composition
            .as_ref()
            .unwrap()
            .preview_text,
        "中"
    );

    assert!(runtime.commit_composition().unwrap());
    assert_eq!(runtime.undo_events.len(), 1);
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "a中b");
    assert!(runtime.editing.as_ref().unwrap().composition.is_none());
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("a中".len().."a中".len())
    );
    assert_eq!(runtime.input_session_marked_range(), None);
}

#[test]
fn composition_commit_undo_redo_restores_block_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "ab",
        )],
        720.0,
    );

    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert!(runtime.commit_composition().unwrap());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "a中b");

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "a中b");
}

#[test]
fn multistage_ime_commit_creates_one_undo_step() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "ab",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    runtime.begin_or_update_composition(1, 1..1, "n").unwrap();
    runtime.begin_or_update_composition(1, 1..1, "ni").unwrap();
    runtime.begin_or_update_composition(1, 1..1, "你").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert!(runtime.commit_composition().unwrap());
    assert_eq!(runtime.undo_events.len(), 1);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "a你b");

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");
    assert!(!runtime.undo_focused_block().unwrap());
}

#[test]
fn replace_text_in_focused_range_commits_text_and_clears_composition() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime.begin_or_update_composition(1, 1..3, "中").unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "字").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "a字d");
    assert!(runtime.active_composition().is_none());
}

#[test]
fn table_cell_composition_preview_and_commit_stay_inside_cell() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("ab")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 1).unwrap();

    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert_eq!(runtime.focused_text_for_platform_input().unwrap().1, "a中b");
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(1.."a中".len())
    );
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    let BlockPayload::Table(table) = &payload.payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a中b"));
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");
    assert_eq!(
        projection.blocks[0].focused_table_cell_offset,
        Some("a中".len())
    );
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(
        editing.input_target,
        InputTarget::TableCell {
            block_id: 1,
            row: 0,
            col: 0
        }
    );
    assert_eq!(editing.selected_range, "a中".len().."a中".len());
    assert_eq!(editing.marked_range, Some(1.."a中".len()));

    assert!(runtime.commit_composition().unwrap());
    assert_eq!(runtime.undo_events.len(), 1);
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a中b"));
    assert!(runtime.active_composition().is_none());
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("a中".len().."a中".len())
    );
    assert_eq!(runtime.input_session_marked_range(), None);
}

#[test]
fn table_cell_composition_commit_undo_redo_restores_cell_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("ab")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 1).unwrap();

    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert!(runtime.commit_composition().unwrap());
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a中b"));

    assert!(runtime.undo_focused_block().unwrap());
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("ab"));

    assert!(runtime.redo_focused_block().unwrap());
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a中b"));
}

#[test]
fn table_cell_emoji_composition_preview_and_commit_stay_inside_cell() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("ab")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 1).unwrap();

    runtime.begin_or_update_composition(1, 1..1, "😀").unwrap();

    assert_eq!(runtime.focused_text_for_platform_input().unwrap().1, "a😀b");
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(1.."a😀".len())
    );

    assert!(runtime.commit_composition().unwrap());
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a😀b"));
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("a😀".len().."a😀".len())
    );
}

#[test]
fn table_cell_cjk_and_emoji_composition_commit_preserves_utf8_boundaries() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("ab")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 1).unwrap();

    let preview = "中かな한글😀";
    runtime
        .begin_or_update_composition(1, 1..1, preview)
        .unwrap();

    assert_eq!(
        runtime.focused_text_for_platform_input().unwrap().1,
        format!("a{preview}b")
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(1..format!("a{preview}").len())
    );
    assert!(runtime.commit_composition().unwrap());

    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(
        table.cell_plain_text(0, 0).as_deref(),
        Some(format!("a{preview}b").as_str())
    );
    assert_eq!(
        runtime.focused_table_cell_offset(),
        Some((1, 0, 0, format!("a{preview}").len()))
    );
}

#[test]
fn platform_input_text_comes_from_input_session_target() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
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
            }),
        }],
        720.0,
    );

    runtime.focus_table_cell_at_offset(1, 0, 1, 2).unwrap();

    assert_eq!(
        runtime.focused_text_for_platform_input(),
        Some((1, "right".to_owned()))
    );
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 1,
            row: 0,
            col: 1
        })
    );
}

#[test]
fn table_cell_replace_text_prioritizes_active_composition() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("abcd")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 2).unwrap();
    runtime.begin_or_update_composition(1, 1..3, "中").unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "文").unwrap());

    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a文d"));
    assert_eq!(
        runtime.focused_table_cell_offset(),
        Some((1, 0, 0, "a文".len()))
    );
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(
        editing.input_target,
        InputTarget::TableCell {
            block_id: 1,
            row: 0,
            col: 0
        }
    );
    assert_eq!(editing.selected_range, "a文".len().."a文".len());
    assert!(runtime.active_composition().is_none());
}

#[test]
fn block_and_table_text_edits_share_invalid_cjk_offset_normalization() {
    let mut block_runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "萨德",
        )],
        720.0,
    );
    block_runtime.focus_block_at_offset(1, 2).unwrap();
    assert!(
        block_runtime
            .replace_text_in_focused_range(Some(2..2), "中")
            .unwrap()
    );
    assert_eq!(block_runtime.focused_text(), Some("中萨德"));
    assert_eq!(block_runtime.caret_offset_for_block(1), Some("中".len()));

    let mut table_runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("萨德")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    table_runtime
        .focus_table_cell_at_offset(1, 0, 0, 2)
        .unwrap();
    assert!(
        table_runtime
            .replace_text_in_focused_range(Some(2..2), "中")
            .unwrap()
    );
    let BlockPayload::Table(table) = &table_runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("中萨德"));
    assert_eq!(
        table_runtime.focused_table_cell_offset(),
        Some((1, 0, 0, "中".len()))
    );
}

#[test]
fn table_composition_normalizes_invalid_cjk_caret_before_preview_and_commit() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("萨德")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 2).unwrap();
    runtime.begin_or_update_composition(1, 2..2, "中").unwrap();

    assert_eq!(
        runtime.focused_text_for_platform_input().unwrap().1,
        "中萨德"
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(0.."中".len())
    );
    assert!(runtime.commit_composition().unwrap());
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("中萨德"));
}
