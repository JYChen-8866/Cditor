use super::*;

#[test]
fn set_table_cell_align_updates_selected_range() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(
        runtime
            .set_table_cell_align(
                10,
                cditor_core::rich_text::TableRange::normalized(0, 1, 1, 1),
                cditor_core::rich_text::TableCellAlign::Right,
            )
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[0].cells[1].align,
        cditor_core::rich_text::TableCellAlign::Right
    );
    assert_eq!(
        table.rows[1].cells[1].align,
        cditor_core::rich_text::TableCellAlign::Right
    );
    assert_eq!(
        table.rows[1].cells[0].align,
        cditor_core::rich_text::TableCellAlign::Left
    );
    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let right_aligned = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 1, col: 1 }))
        .expect("right aligned cell");
    assert_eq!(
        right_aligned.align,
        cditor_core::rich_text::TableCellAlign::Right
    );
    let left_aligned = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 1, col: 0 }))
        .expect("left aligned cell");
    assert_eq!(
        left_aligned.align,
        cditor_core::rich_text::TableCellAlign::Left
    );
}

#[test]
fn table_cell_align_supports_undo_and_redo() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_block(10);
    runtime.focus_table_cell_at_offset(10, 1, 0, 1).unwrap();
    if let Some(cell) = runtime.focused_table_cell.as_mut() {
        *cell = cell.with_selected_range(0..1, true);
    }

    assert!(
        runtime
            .set_table_cell_align(
                10,
                runtime.table_row_selection_range(10, 1).expect("row range"),
                cditor_core::rich_text::TableCellAlign::Center,
            )
            .unwrap()
    );
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[1].cells[0].align,
        cditor_core::rich_text::TableCellAlign::Center
    );
    assert_eq!(
        table.rows[1].cells[1].align,
        cditor_core::rich_text::TableCellAlign::Center
    );

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 1, 0, 0..1, true, None))
    );
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[1].cells[0].align,
        cditor_core::rich_text::TableCellAlign::Left
    );

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 1, 0, 0..1, true, None))
    );
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[1].cells[0].align,
        cditor_core::rich_text::TableCellAlign::Center
    );
}

#[test]
fn table_kind_with_non_table_payload_is_normalized_on_load() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 10,
            content_version: 7,
            kind: RichBlockKind::Table,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan::plain("legacy")],
            },
        }],
        720.0,
    );

    assert!(!runtime.text_models.contains_key(&10));
    runtime.focus_block(10);
    runtime.focus_block(10);
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("expected loaded payload");
    };
    let BlockPayload::Table(table) = &payload.payload else {
        panic!("expected normalized table payload");
    };
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0].cells.len(), 3);
    assert_eq!(table.cell_plain_text(0, 0), Some("legacy".to_owned()));
}

#[test]
fn table_kind_with_empty_table_payload_gets_default_cells() {
    let runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 10,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload::default()),
        }],
        720.0,
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0].cells.len(), 3);
}

#[test]
fn table_runtime_survives_stale_empty_payload_snapshot() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.insert_char('!').unwrap();
    let payload = runtime.payload_window.payloads.get_mut(&10).unwrap();
    payload.payload = BlockPayload::Table(cditor_core::rich_text::TablePayload::default());
    payload.content_version = payload.content_version.saturating_add(1);

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload from runtime");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));

    let snapshot = runtime.loaded_payload_records_snapshot();
    let BlockPayload::Table(table) = &snapshot
        .iter()
        .find(|payload| payload.block_id == 10)
        .expect("table payload snapshot")
        .payload
    else {
        panic!("expected saved table payload from runtime");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("expected loaded payload");
    };
    let BlockPayload::Table(table) = &payload.payload else {
        panic!("expected projected table payload from runtime");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table view state");
    assert_eq!(table_view.table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(
        table_view.table.cell_plain_text(0, 1),
        Some("B!".to_owned())
    );
}

#[test]
fn converting_to_table_removes_plain_text_model() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "/table")]);

    runtime.focus_block_at_offset(1, "/table".len()).unwrap();
    assert!(runtime.text_models.contains_key(&1));
    assert!(
        runtime
            .replace_text_in_focused_range(Some(0.."/table".len()), "")
            .unwrap()
    );
    assert!(
        runtime
            .convert_focused_block_kind(RichBlockKind::Table)
            .unwrap()
    );

    assert!(!runtime.text_models.contains_key(&1));
    assert_eq!(runtime.focused_text(), None);
    assert!(matches!(
        runtime.block_payload_record(1).unwrap().payload,
        BlockPayload::Table(_)
    ));
}

#[test]
fn editing_table_cell_does_not_create_block_text_model() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.insert_char('!').unwrap();

    assert!(!runtime.text_models.contains_key(&10));
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));
}

#[test]
fn enter_on_table_block_without_cell_focus_preserves_table_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_block(10);
    runtime.handle_enter().unwrap();

    assert_eq!(runtime.index.block_ids, vec![10]);
    assert!(!runtime.text_models.contains_key(&10));
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B".to_owned()));
}

#[test]
fn splitting_table_payload_never_exports_cell_text_to_new_paragraph() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_block(10);
    runtime
        .split_focused_block_at_caret(EnterSplitMode::ForceParagraph)
        .unwrap();

    assert_eq!(runtime.index.block_ids, vec![10, 11]);
    let current = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = current.payload else {
        panic!("expected original table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B".to_owned()));
    assert_eq!(runtime.block_payload_record(11).unwrap().plain_text(), "");
}

#[test]
fn enter_in_focused_table_cell_inserts_newline_inside_cell() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.handle_enter().unwrap();

    assert_eq!(runtime.index.block_ids, vec![10]);
    assert!(!runtime.text_models.contains_key(&10));
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1), Some("B\n".to_owned()));
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 2)));
}

#[test]
fn backspace_and_delete_in_table_cell_do_not_delete_table_block() {
    let paragraph = BlockPayloadRecord::rich_text(11, RichBlockKind::Paragraph, "below");
    let mut runtime =
        DocumentRuntime::from_payloads(1, vec![sample_table_payload(), paragraph], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 0, 0).unwrap();
    assert!(!runtime.delete_backward().unwrap());
    assert_eq!(runtime.index.block_ids, vec![10, 11]);
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 0)));

    assert!(runtime.delete_forward().unwrap());
    assert_eq!(runtime.index.block_ids, vec![10, 11]);
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some(""));
    assert_eq!(
        runtime.block_payload_record(11).unwrap().plain_text(),
        "below"
    );
}

#[test]
fn table_composition_preview_without_cell_focus_keeps_projection_payload_table() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_block(10);
    runtime
        .editing
        .as_mut()
        .unwrap()
        .update_composition(CompositionState {
            block_id: 10,
            range_start: 0,
            range_end: 0,
            preview_text: "中".to_owned(),
            selected_range_start: None,
            selected_range_end: None,
        })
        .unwrap();

    let payload = runtime.block_payload_record(10).unwrap();
    assert!(matches!(payload.payload, BlockPayload::Table(_)));
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("expected loaded payload");
    };
    assert!(matches!(payload.payload, BlockPayload::Table(_)));
}
