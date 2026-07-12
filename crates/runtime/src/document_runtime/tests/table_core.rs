use super::*;

#[test]
fn table_cell_focus_is_projected_without_ui_entity_state() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    let projection = runtime.projection_for_window();

    assert_eq!(runtime.focused_block_id(), Some(10));
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 1)));
    assert_eq!(
        projection.blocks[0].focused_table_cell,
        Some(TableCellPosition { row: 0, col: 1 })
    );
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table view state");
    assert_eq!(
        table_view.focused_cell,
        Some(TableCellPosition { row: 0, col: 1 })
    );
    assert_eq!(table_view.focused_cell_offset, Some(1));
}

#[test]
fn blur_table_cell_exits_cell_editing_without_writing_old_cell() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 1, 1).unwrap();
    assert!(runtime.blur_table_cell());
    runtime.insert_char('x').unwrap();

    assert_eq!(runtime.focused_block_id(), Some(10));
    assert_eq!(runtime.focused_table_cell_offset(), None);
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("B"));
}

#[test]
fn focused_table_cell_persists_selection_marked_range_and_direction() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 1, 1).unwrap();
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 1..1, false, None))
    );

    runtime
        .begin_or_update_composition_with_selection(10, 0..1, "中", Some(0.."中".len()))
        .unwrap();
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 0.."中".len(), false, Some(0.."中".len())))
    );
    assert_eq!(runtime.input_session_selected_range(), Some(0.."中".len()));
    assert_eq!(runtime.input_session_marked_range(), Some(0.."中".len()));

    runtime.cancel_composition();
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 0.."中".len(), false, None))
    );

    runtime
        .replace_text_in_focused_range(Some(0..1), "Z")
        .unwrap();
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 1..1, false, None))
    );
}

#[test]
fn insert_char_updates_focused_table_cell_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.insert_char('!').unwrap();

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));
    assert_eq!(payload.content_version, 2);
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 2)));
}

#[test]
fn delete_backward_and_forward_update_focused_table_cell_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.insert_char('中').unwrap();
    assert!(runtime.delete_backward().unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 1)));
    runtime.insert_char('x').unwrap();
    runtime.focused_table_cell = Some(FocusedTableCell::collapsed(10, 0, 1, 1));
    assert!(runtime.delete_forward().unwrap());

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1), Some("B".to_owned()));
}

#[test]
fn table_cell_arrow_navigation_updates_focus_and_input_session() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 1, 1).unwrap();
    assert!(runtime.move_focused_table_cell_left().unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 0)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 1,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(0..0));

    assert!(runtime.move_focused_table_cell_left().unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 0,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(1..1));

    assert!(runtime.move_focused_table_cell_down().unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 1, 0, 1)));
}

#[test]
fn table_cell_tab_navigation_updates_focus_and_input_session() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();

    assert!(runtime.move_focused_table_cell_tab(false).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 0)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 1,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(0..0));

    assert!(runtime.move_focused_table_cell_tab(true).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 0,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(1..1));
}

#[test]
fn plain_text_input_on_table_block_without_cell_focus_preserves_table_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(!runtime.text_models.contains_key(&10));
    runtime.focus_block(10);
    assert_eq!(runtime.focused_text(), None);
    runtime.insert_char('x').unwrap();
    assert!(!runtime.replace_text_in_focused_range(None, "y").unwrap());
    runtime
        .begin_or_update_composition_with_selection(10, 0..0, "中", None)
        .unwrap();

    assert_eq!(runtime.focused_text_for_platform_input(), None);
    assert!(runtime.active_composition().is_none());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B".to_owned()));
}

#[test]
fn merge_table_cells_updates_payload_runtime_and_content_version() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(
        runtime
            .merge_table_cells(
                10,
                cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
            )
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 2);
    assert_eq!(
        table.rows[0].cells[0].merge,
        cditor_core::rich_text::TableCellMerge::Origin {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(table.cell_origin(1, 1), Some((0, 0)));
    assert_eq!(table.cell_plain_text(1, 1), Some("A\tB\nC\tD".to_owned()));
}

#[test]
fn focusing_covered_table_cell_targets_merge_origin_cell() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    runtime.focus_table_cell_at_offset(10, 1, 1, 1).unwrap();

    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 0,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(1..1));
}

#[test]
fn projection_outputs_visible_cells_for_merged_table_geometry() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let merged = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("merged origin cell");

    assert_eq!(table_view.visible_cells.len(), 1);
    assert_eq!(merged.row_span, 2);
    assert_eq!(merged.col_span, 2);
    assert_eq!(merged.x_px, 0.0);
    assert_eq!(merged.y_px, 0.0);
    assert_eq!(merged.width_px, 240.0);
    assert_eq!(merged.height_px, 72.0);
}

#[test]
fn merged_table_projection_hit_test_area_targets_origin_for_covered_cell_space() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let covered_cell_center_x = 180.0;
    let covered_cell_center_y = 54.0;
    let hit = table_view
        .visible_cells
        .iter()
        .find(|cell| {
            covered_cell_center_x >= cell.x_px
                && covered_cell_center_x < cell.x_px + cell.width_px
                && covered_cell_center_y >= cell.y_px
                && covered_cell_center_y < cell.y_px + cell.height_px
        })
        .expect("covered cell area should be inside merged origin rect");

    assert_eq!(table_view.visible_cells.len(), 1);
    assert_eq!(hit.position, TableCellPosition { row: 0, col: 0 });
    runtime.focus_table_cell_at_offset(10, 1, 1, 1).unwrap();
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
}

#[test]
fn table_selection_axis_and_cell_predicates_are_block_scoped() {
    let runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    let row_selection = runtime
        .table_row_selection_range(10, 1)
        .expect("row selection");
    assert_eq!(
        row_selection,
        cditor_core::rich_text::TableRange::normalized(1, 0, 1, 1)
    );

    let column_selection = runtime
        .table_column_selection_range(10, 1)
        .expect("column selection");
    assert_eq!(
        column_selection,
        cditor_core::rich_text::TableRange::normalized(0, 1, 1, 1)
    );

    let cell_selection = runtime
        .table_cell_selection_range(10, 1, 1)
        .expect("cell selection");
    assert_eq!(
        cell_selection,
        cditor_core::rich_text::TableRange::normalized(1, 1, 1, 1)
    );
    assert!(runtime.table_row_selection_range(10, 2).is_none());
    assert!(runtime.table_column_selection_range(10, 2).is_none());
    assert!(runtime.table_cell_selection_range(10, 2, 0).is_none());
    assert!(runtime.table_row_selection_range(11, 1).is_none());
}

#[test]
fn table_range_and_whole_table_selection_are_bounds_checked() {
    let runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert_eq!(
        runtime
            .table_range_selection_range(
                10,
                cditor_core::rich_text::TableRange::normalized(1, 0, 0, 1)
            )
            .expect("range selection"),
        cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
    );
    assert_eq!(
        runtime
            .whole_table_selection_range(10)
            .expect("whole table selection"),
        cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
    );
    assert!(
        runtime
            .table_range_selection_range(
                10,
                cditor_core::rich_text::TableRange::normalized(0, 0, 2, 1)
            )
            .is_none()
    );
    assert!(runtime.whole_table_selection_range(11).is_none());
}

#[test]
fn table_clipboard_exports_cell_range_row_column_and_whole_table() {
    let runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    let cell = runtime
        .table_clipboard_for_cell(10, 0, 1)
        .expect("cell clipboard");
    assert_eq!(
        cell.range,
        cditor_core::rich_text::TableRange::normalized(0, 1, 0, 1)
    );
    assert_eq!(cell.table.row_count(), 1);
    assert_eq!(cell.table.column_count(), 1);
    assert_eq!(cell.plain_text, "B");
    assert_eq!(cell.markdown, "| B |\n| --- |");

    let range = runtime
        .table_clipboard_for_range(
            10,
            cditor_core::rich_text::TableRange::normalized(1, 1, 0, 0),
        )
        .expect("range clipboard");
    assert_eq!(
        range.range,
        cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
    );
    assert_eq!(range.plain_text, "A\tB\nC\tD");

    let row = runtime
        .table_clipboard_for_row(10, 1)
        .expect("row clipboard");
    assert_eq!(
        row.range,
        cditor_core::rich_text::TableRange::normalized(1, 0, 1, 1)
    );
    assert_eq!(row.table.row_count(), 1);
    assert_eq!(row.table.column_count(), 2);
    assert_eq!(row.plain_text, "C\tD");

    let column = runtime
        .table_clipboard_for_column(10, 1)
        .expect("column clipboard");
    assert_eq!(
        column.range,
        cditor_core::rich_text::TableRange::normalized(0, 1, 1, 1)
    );
    assert_eq!(column.table.row_count(), 2);
    assert_eq!(column.table.column_count(), 1);
    assert_eq!(column.plain_text, "B\nD");

    let whole = runtime
        .table_clipboard_for_whole_table(10)
        .expect("whole table clipboard");
    assert_eq!(
        whole.range,
        cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
    );
    assert_eq!(whole.plain_text, "A\tB\nC\tD");
    assert_eq!(whole.markdown, "| A | B |\n| --- | --- |\n| C | D |");
    assert!(runtime.table_clipboard_for_cell(10, 3, 0).is_none());
    assert!(runtime.table_clipboard_for_whole_table(99).is_none());
}
