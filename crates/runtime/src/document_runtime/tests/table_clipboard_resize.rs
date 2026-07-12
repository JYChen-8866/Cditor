use super::*;

#[test]
fn table_clipboard_preserves_merge_align_and_track_sizes_for_internal_snapshot() {
    let mut payload = sample_table_payload();
    let BlockPayload::Table(table) = &mut payload.payload else {
        panic!("expected table payload");
    };
    table.header_style.row_background_color = Some("header-row".to_owned());
    table.header_style.column_background_color = Some("header-column".to_owned());
    table.rows[0].cells[0].style.background_color = Some("cell-bg".to_owned());
    let mut runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);
    runtime
        .set_table_column_width(10, 0, TableTrackSize::Px(180))
        .unwrap();
    runtime
        .set_table_row_height(10, 0, TableTrackSize::Px(64))
        .unwrap();
    runtime
        .set_table_cell_align(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
            cditor_core::rich_text::TableCellAlign::Center,
        )
        .unwrap();
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    let snapshot = runtime
        .table_clipboard_for_whole_table(10)
        .expect("whole merged table clipboard");
    assert_eq!(snapshot.table.columns[0].width, TableTrackSize::Px(180));
    assert_eq!(snapshot.table.rows[0].height, TableTrackSize::Px(64));
    assert_eq!(
        snapshot.table.rows[0].cells[0].merge,
        TableCellMerge::Origin {
            row_span: 2,
            col_span: 2,
        }
    );
    assert_eq!(
        snapshot.table.rows[1].cells[1].merge,
        TableCellMerge::Covered {
            origin_row: 0,
            origin_col: 0,
        }
    );
    assert_eq!(
        snapshot.table.rows[0].cells[0].align,
        TableCellAlign::Center
    );
    assert_eq!(
        snapshot.table.header_style.row_background_color.as_deref(),
        Some("header-row")
    );
    assert_eq!(
        snapshot
            .table
            .header_style
            .column_background_color
            .as_deref(),
        Some("header-column")
    );
    assert_eq!(
        snapshot.table.rows[0].cells[0]
            .style
            .background_color
            .as_deref(),
        Some("cell-bg")
    );
    assert!(snapshot.plain_text.contains("A\tB\nC\tD"));
    assert!(snapshot.markdown.contains("A\tB<br>C\tD"));

    let partial = runtime
        .table_clipboard_for_column(10, 1)
        .expect("partial merged column clipboard");
    assert_eq!(
        partial.table.rows[0].cells[0].merge,
        TableCellMerge::Unmerged,
        "partial selections must not keep dangling merge metadata"
    );
    assert_eq!(partial.table.rows[0].cells[0].align, TableCellAlign::Center);
}

#[test]
fn paste_table_clipboard_at_focused_cell_expands_table_and_supports_undo_redo() {
    let source_runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    let snapshot = source_runtime
        .table_clipboard_for_whole_table(10)
        .expect("source table clipboard");
    let mut target = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(20, RichBlockKind::Table, "")],
        720.0,
    );
    target.focus_table_cell_at_offset(20, 1, 1, 0).unwrap();

    assert!(
        target
            .paste_table_clipboard_at_focused_cell(&snapshot)
            .unwrap()
    );

    let payload = target.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 2);
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(1, 2).as_deref(), Some("B"));
    assert_eq!(table.cell_plain_text(2, 1).as_deref(), Some("C"));
    assert_eq!(target.focused_table_cell_offset(), Some((20, 1, 1, 0)));

    assert!(target.undo_focused_block().unwrap());
    let payload = target.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some(""));

    assert!(target.redo_focused_block().unwrap());
    let payload = target.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(2, 2).as_deref(), Some("D"));
}

#[test]
fn paste_delimited_table_text_at_focused_cell_supports_tsv_csv_and_expansion() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(20, RichBlockKind::Table, "")],
        720.0,
    );
    runtime.focus_table_cell_at_offset(20, 0, 1, 0).unwrap();

    assert!(
        runtime
            .paste_delimited_table_text_at_focused_cell("A\tB\nC\tD")
            .unwrap()
    );

    let payload = runtime.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(1, 2).as_deref(), Some("D"));

    runtime.focus_table_cell_at_offset(20, 0, 0, 0).unwrap();
    assert!(
        runtime
            .paste_delimited_table_text_at_focused_cell("\"x,y\",z")
            .unwrap()
    );
    let payload = runtime.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("x,y"));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("z"));
    assert!(
        !runtime
            .paste_delimited_table_text_at_focused_cell("plain")
            .unwrap()
    );
}

#[test]
fn table_track_resize_updates_payload_projection_and_content_version() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(
        runtime
            .set_table_column_width(10, 1, TableTrackSize::Px(180))
            .unwrap()
    );
    assert!(
        runtime
            .set_table_row_height(10, 1, TableTrackSize::Px(56))
            .unwrap()
    );
    assert!(
        !runtime
            .set_table_row_height(10, 1, TableTrackSize::Px(56))
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 3);
    assert_eq!(table.columns[1].width, TableTrackSize::Px(180));
    assert_eq!(table.rows[1].height, TableTrackSize::Px(56));

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    assert_eq!(table_view.width_px, 300.0);
    assert_eq!(table_view.height_px, 92.0);
    let resized_cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 1, col: 1 }))
        .expect("resized cell");
    assert_eq!(resized_cell.x_px, 120.0);
    assert_eq!(resized_cell.y_px, 36.0);
    assert_eq!(resized_cell.width_px, 180.0);
    assert_eq!(resized_cell.height_px, 56.0);
}

#[test]
fn table_resize_supports_undo_and_redo() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_block(10);

    assert!(
        runtime
            .set_table_column_width(10, 0, TableTrackSize::Px(180))
            .unwrap()
    );
    assert!(
        runtime
            .set_table_row_height(10, 0, TableTrackSize::Px(64))
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Px(180));
    assert_eq!(table.rows[0].height, TableTrackSize::Px(64));
    assert_eq!(
        runtime.projection_for_window().blocks[0]
            .table_view
            .as_ref()
            .expect("table projection")
            .height_px,
        100.0
    );

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Px(180));
    assert_eq!(table.rows[0].height, TableTrackSize::Auto);
    assert_eq!(
        runtime.projection_for_window().blocks[0]
            .table_view
            .as_ref()
            .expect("table projection")
            .height_px,
        72.0
    );

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Auto);
    assert_eq!(table.rows[0].height, TableTrackSize::Auto);
    assert_eq!(
        runtime.projection_for_window().blocks[0]
            .table_view
            .as_ref()
            .expect("table projection")
            .height_px,
        72.0
    );

    assert!(runtime.redo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Px(180));
    assert_eq!(table.rows[0].height, TableTrackSize::Auto);
}

#[test]
fn table_column_width_change_recomputes_auto_row_height_from_wrapping() {
    let mut payload = sample_table_payload();
    let BlockPayload::Table(table) = &mut payload.payload else {
        panic!("payload should be table");
    };
    table.set_cell_plain_text(0, 0, "abcdefghijklmnopqrstuvwxyz".repeat(8));
    let mut runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);

    let before_projection = runtime.projection_for_window();
    let before_table = before_projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let before_height = before_table
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("first cell")
        .height_px;

    assert!(
        runtime
            .set_table_column_width(10, 0, TableTrackSize::Px(360))
            .unwrap()
    );

    let after_projection = runtime.projection_for_window();
    let after_table = after_projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let after_height = after_table
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("first cell")
        .height_px;

    assert!(
        after_height < before_height,
        "before_height={before_height} after_height={after_height}"
    );
    assert_eq!(
        after_projection.blocks[0].layout.effective_height() as f32,
        after_table.height_px
            + cditor_core::layout::COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX as f32
            + cditor_core::layout::TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX as f32
    );
}

#[test]
fn table_row_and_column_structure_edits_update_payload_projection_and_focus() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 1, 1, 1).unwrap();
    assert!(runtime.insert_table_row(10, 1).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 2, 1, 1)));
    assert!(runtime.insert_table_column(10, 1).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 2, 2, 1)));

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 3);
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(2, 2).as_deref(), Some("D"));

    assert!(runtime.move_table_row(10, 2, 0).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 2, 1)));
    assert!(runtime.move_table_column(10, 2, 0).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert!(runtime.delete_table_row(10, 1).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert!(runtime.delete_table_column(10, 1).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 7);
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 2);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("D"));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some(""));

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    assert_eq!(table_view.row_count, 2);
    assert_eq!(table_view.col_count, 2);
    assert_eq!(table_view.visible_cells.len(), 4);
}

#[test]
fn table_duplicate_row_and_column_update_payload_and_projection() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(runtime.duplicate_table_row(10, 0).unwrap());
    assert!(runtime.duplicate_table_column(10, 0).unwrap());

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 3);
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("A"));

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    assert_eq!(table_view.row_count, 3);
    assert_eq!(table_view.col_count, 3);
    assert_eq!(table_view.visible_cells.len(), 9);
}
