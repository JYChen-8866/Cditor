use super::*;

#[test]
fn table_cell_background_color_is_projected_from_payload_style() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(
        runtime
            .set_table_cell_background_color(
                10,
                TableRange::normalized(0, 0, 0, 1),
                Some("action_background".to_owned()),
            )
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[0].cells[1].style.background_color.as_deref(),
        Some("action_background")
    );

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    assert_eq!(
        table_view.visible_cells[0].background_color.as_deref(),
        Some("action_background")
    );
}

#[test]
fn table_structure_edits_reject_merged_tables_without_losing_runtime_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(runtime.insert_table_row(10, 1).is_err());
    assert!(runtime.delete_table_row(10, 1).is_err());
    assert!(runtime.move_table_row(10, 0, 1).is_err());
    assert!(runtime.insert_table_column(10, 1).is_err());
    assert!(runtime.delete_table_column(10, 1).is_err());
    assert!(runtime.move_table_column(10, 0, 1).is_err());

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 2);
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 2);
    assert_eq!(table.visible_cells().count(), 1);
}

#[test]
fn table_row_and_column_reorder_support_undo_and_redo() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_block(10);

    assert!(runtime.move_table_row(10, 0, 1).unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("C"));

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A"));

    assert!(runtime.redo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("C"));

    assert!(runtime.move_table_column(10, 0, 1).unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("D"));

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("C"));
}

#[test]
fn table_auto_row_height_grows_for_multiline_cell_text() {
    let mut payload = sample_table_payload();
    let BlockPayload::Table(table) = &mut payload.payload else {
        panic!("payload should be table");
    };
    table.set_cell_plain_text(0, 0, "埃塞\n埃塞\n埃塞\n埃塞");

    let runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);
    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let first_cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("first cell");
    let second_row_cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 1, col: 0 }))
        .expect("second row cell");

    assert!(
        first_cell.height_px > 72.0,
        "height={}",
        first_cell.height_px
    );
    assert_eq!(first_cell.y_px, 0.0);
    assert_eq!(second_row_cell.y_px, first_cell.height_px);
    assert_eq!(
        table_view.height_px,
        first_cell.height_px + second_row_cell.height_px
    );
    assert_eq!(
        projection.blocks[0].layout.effective_height() as f32,
        table_view.height_px
            + cditor_core::layout::COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX as f32
            + cditor_core::layout::TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX as f32
    );
}

#[test]
fn table_cell_enter_updates_block_height_and_pushes_following_blocks_down() {
    let table = sample_table_payload();
    let paragraph = BlockPayloadRecord::rich_text(11, RichBlockKind::Paragraph, "below");
    let mut runtime = DocumentRuntime::from_payloads(1, vec![table, paragraph], 720.0);
    let before_table_height = runtime.index.layout_meta[0].effective_height();
    let before_second_offset = runtime.height_index.offset_of_block(1).unwrap();

    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    for _ in 0..6 {
        runtime.handle_enter().unwrap();
    }

    let projection = runtime.projection_for_window();
    let table_block = projection
        .blocks
        .iter()
        .find(|block| block.block_id == 10)
        .expect("table block");
    let table_view = table_block.table_view.as_ref().expect("table projection");
    let after_table_height = table_block.layout.effective_height();
    let after_second_offset = runtime.height_index.offset_of_block(1).unwrap();

    assert!(table_view.height_px > before_table_height as f32);
    assert!(after_table_height > before_table_height);
    assert_eq!(after_second_offset, after_table_height);
    assert!(after_second_offset > before_second_offset);
}

#[test]
fn table_height_change_above_viewport_restores_viewport_anchor() {
    let table = sample_table_payload();
    let mut payloads = vec![table];
    payloads.extend((11..160).map(|block_id| {
        BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "below")
    }));
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, 720.0);
    runtime
        .scroll
        .scroll_to_global_offset(1_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_table_height = runtime.height_index.heights[0];

    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    for _ in 0..6 {
        runtime.handle_enter().unwrap();
    }

    let after_table_height = runtime.height_index.heights[0];
    let height_delta = after_table_height - before_table_height;
    assert!(height_delta > 0.0);
    assert_eq!(
        runtime.scroll.global_scroll_top,
        before_scroll_top + height_delta
    );
}

#[test]
fn table_height_change_during_scrollbar_drag_defers_displayed_total_update() {
    let table = sample_table_payload();
    let mut payloads = vec![table];
    payloads.extend((11..80).map(|block_id| {
        BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "below")
    }));
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, 720.0);
    let policy = ScrollbarPolicy {
        track_height: 720.0,
        ..ScrollbarPolicy::default()
    };
    let before_displayed_total = runtime.scroll.displayed_total_height;
    let before_model_total = runtime.scroll.model_total_height;

    let visual = runtime.begin_scrollbar_drag(policy);
    assert!(visual.enabled);
    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    for _ in 0..6 {
        runtime.handle_enter().unwrap();
    }

    assert!(runtime.scroll.model_total_height > before_model_total);
    assert_eq!(
        runtime.scroll.displayed_total_height,
        before_displayed_total
    );
    let end = runtime.finish_scrollbar_drag().unwrap().unwrap();
    assert!(end.pending_layout_corrections > 0);
    assert_eq!(
        runtime.scroll.displayed_total_height,
        runtime.scroll.model_total_height
    );
}

#[test]
fn table_manual_row_height_is_not_overridden_by_multiline_text() {
    let mut payload = sample_table_payload();
    let BlockPayload::Table(table) = &mut payload.payload else {
        panic!("payload should be table");
    };
    table.rows[0].height = cditor_core::rich_text::TableTrackSize::Px(40);
    table.set_cell_plain_text(0, 0, "埃塞\n埃塞\n埃塞\n埃塞");

    let runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);
    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let first_cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("first cell");

    assert_eq!(first_cell.height_px, 40.0);
}

#[test]
fn split_table_cell_restores_covered_cells_and_bumps_content_version() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(runtime.split_table_cell(10, 1, 1).unwrap());

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 3);
    assert_eq!(
        table.rows[0].cells[0].merge,
        cditor_core::rich_text::TableCellMerge::Unmerged
    );
    assert_eq!(
        table.rows[1].cells[1].merge,
        cditor_core::rich_text::TableCellMerge::Unmerged
    );
    assert_eq!(table.cell_plain_text(0, 0), Some("A\tB\nC\tD".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some(String::new()));
    assert_eq!(table.cell_plain_text(1, 0), Some(String::new()));
    assert_eq!(table.cell_plain_text(1, 1), Some(String::new()));
}

#[test]
fn merge_and_split_table_cells_support_undo_and_redo() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_block(10);

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
    assert_eq!(table.visible_cells().count(), 1);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A\tB\nC\tD"));

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.visible_cells().count(), 4);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("D"));

    assert!(runtime.redo_focused_block().unwrap());
    assert!(runtime.split_table_cell(10, 0, 0).unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.visible_cells().count(), 4);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A\tB\nC\tD"));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some(""));

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.visible_cells().count(), 1);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A\tB\nC\tD"));
}

#[test]
fn merged_table_cell_geometry_updates_after_row_and_column_resize() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(
        runtime
            .set_table_column_width(10, 0, TableTrackSize::Px(180))
            .unwrap()
    );
    assert!(
        runtime
            .set_table_row_height(10, 1, TableTrackSize::Px(64))
            .unwrap()
    );

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let merged = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("merged cell");
    assert_eq!(table_view.visible_cells.len(), 1);
    assert_eq!(merged.width_px, 300.0);
    assert_eq!(merged.height_px, 100.0);
}

#[test]
fn merged_table_metadata_remaps_after_runtime_row_and_column_reorder() {
    let mut row_runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    assert!(row_runtime.insert_table_row(10, 2).unwrap());
    row_runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(row_runtime.move_table_row(10, 2, 0).unwrap());

    let payload = row_runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[1].cells[0].merge,
        cditor_core::rich_text::TableCellMerge::Origin {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(
        table.rows[2].cells[1].merge,
        cditor_core::rich_text::TableCellMerge::Covered {
            origin_row: 1,
            origin_col: 0
        }
    );

    let mut column_runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    assert!(column_runtime.insert_table_column(10, 2).unwrap());
    column_runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(column_runtime.move_table_column(10, 2, 0).unwrap());

    let payload = column_runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[0].cells[1].merge,
        cditor_core::rich_text::TableCellMerge::Origin {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(
        table.rows[1].cells[2].merge,
        cditor_core::rich_text::TableCellMerge::Covered {
            origin_row: 0,
            origin_col: 1
        }
    );
}

#[test]
fn runtime_rejects_reorder_that_would_split_merged_table_cell() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(runtime.move_table_row(10, 0, 1).is_err());
    assert!(runtime.move_table_column(10, 0, 1).is_err());
}
