use super::*;

fn table_2_by_3() -> TablePayload {
    let mut table = TablePayload {
        rows: vec![
            TableRowPayload {
                cells: vec![
                    TableCellPayload::plain("a"),
                    TableCellPayload::plain("b"),
                    TableCellPayload::plain("c"),
                ],
                height: TableTrackSize::Auto,
            },
            TableRowPayload {
                cells: vec![
                    TableCellPayload::plain("d"),
                    TableCellPayload::plain("e"),
                    TableCellPayload::plain("f"),
                ],
                height: TableTrackSize::Auto,
            },
        ],
        columns: Vec::new(),
        header_rows: 0,
        header_cols: 0,
        header_style: Default::default(),
    };
    table.normalize();
    table
}

#[test]
fn normalize_backfills_column_tracks_for_old_payloads() {
    let table = table_2_by_3();

    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.columns.len(), 3);
}

#[test]
fn normalize_preserves_cell_and_header_style_payloads() {
    let mut table = table_2_by_3();
    table.header_style.row_background_color = Some("header-row".to_owned());
    table.header_style.column_background_color = Some("header-column".to_owned());
    table.rows[0].cells[1].style.background_color = Some("cell-bg".to_owned());

    table.normalize();

    assert_eq!(
        table.header_style.row_background_color.as_deref(),
        Some("header-row")
    );
    assert_eq!(
        table.header_style.column_background_color.as_deref(),
        Some("header-column")
    );
    assert_eq!(
        table.rows[0].cells[1].style.background_color.as_deref(),
        Some("cell-bg")
    );
}

#[test]
fn set_row_height_and_column_width_update_track_sizes() {
    let mut table = table_2_by_3();

    assert!(table.set_row_height(1, TableTrackSize::Px(56)).unwrap());
    assert!(table.set_column_width(2, TableTrackSize::Px(180)).unwrap());
    assert_eq!(table.rows[1].height, TableTrackSize::Px(56));
    assert_eq!(table.columns[2].width, TableTrackSize::Px(180));
    assert!(!table.set_row_height(1, TableTrackSize::Px(56)).unwrap());
    assert!(!table.set_column_width(2, TableTrackSize::Px(180)).unwrap());
    assert!(table.set_row_height(2, TableTrackSize::Px(56)).is_err());
    assert!(table.set_column_width(3, TableTrackSize::Px(180)).is_err());
}

#[test]
fn insert_delete_and_move_rows_preserve_cells_and_row_sizes() {
    let mut table = table_2_by_3();
    table.set_row_height(0, TableTrackSize::Px(44)).unwrap();

    assert!(table.insert_row(1).unwrap());
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(2, 0).as_deref(), Some("d"));

    assert!(table.move_row(0, 2).unwrap());
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some("d"));
    assert_eq!(table.cell_plain_text(2, 0).as_deref(), Some("a"));
    assert_eq!(table.rows[2].height, TableTrackSize::Px(44));

    assert!(table.delete_row(0).unwrap());
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("d"));
    assert!(table.delete_row(2).is_err());
}

#[test]
fn duplicate_row_preserves_cell_content_and_row_size() {
    let mut table = table_2_by_3();
    table.set_row_height(0, TableTrackSize::Px(44)).unwrap();

    assert!(table.duplicate_row(0).unwrap());

    assert_eq!(table.row_count(), 3);
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some("a"));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("b"));
    assert_eq!(table.rows[1].height, TableTrackSize::Px(44));
    assert!(table.duplicate_row(3).is_err());
}

#[test]
fn insert_delete_and_move_columns_preserve_cells_and_column_sizes() {
    let mut table = table_2_by_3();
    table.set_column_width(0, TableTrackSize::Px(150)).unwrap();

    assert!(table.insert_column(1).unwrap());
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 4);
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(0, 2).as_deref(), Some("b"));

    assert!(table.move_column(0, 3).unwrap());
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("b"));
    assert_eq!(table.cell_plain_text(0, 2).as_deref(), Some("c"));
    assert_eq!(table.cell_plain_text(0, 3).as_deref(), Some("a"));
    assert_eq!(table.columns[3].width, TableTrackSize::Px(150));

    assert!(table.delete_column(0).unwrap());
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("b"));
    assert!(table.delete_column(3).is_err());
}

#[test]
fn duplicate_column_preserves_cell_content_and_column_size() {
    let mut table = table_2_by_3();
    table.set_column_width(0, TableTrackSize::Px(150)).unwrap();

    assert!(table.duplicate_column(0).unwrap());

    assert_eq!(table.column_count(), 4);
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("a"));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("d"));
    assert_eq!(table.columns[1].width, TableTrackSize::Px(150));
    assert!(table.duplicate_column(4).is_err());
}

#[test]
fn set_cell_background_color_updates_range_style() {
    let mut table = table_2_by_3();

    assert!(
        table
            .set_cell_background_color(
                TableRange::normalized(0, 1, 1, 2),
                Some("action_background".to_owned()),
            )
            .unwrap()
    );
    assert_eq!(
        table.rows[0].cells[1].style.background_color.as_deref(),
        Some("action_background")
    );
    assert_eq!(
        table.rows[1].cells[2].style.background_color.as_deref(),
        Some("action_background")
    );
    assert!(
        !table
            .set_cell_background_color(
                TableRange::normalized(0, 1, 1, 2),
                Some("action_background".to_owned()),
            )
            .unwrap()
    );
}

#[test]
fn structural_row_and_column_edits_reject_merged_tables() {
    let mut table = table_2_by_3();
    table
        .merge_cells(TableRange::normalized(0, 0, 1, 1))
        .unwrap();

    assert!(table.insert_row(1).is_err());
    assert!(table.delete_row(1).is_err());
    assert!(table.move_row(0, 1).is_err());
    assert!(table.insert_column(1).is_err());
    assert!(table.delete_column(1).is_err());
    assert!(table.move_column(0, 1).is_err());
}

#[test]
fn move_row_remaps_merge_metadata_when_merged_range_stays_contiguous() {
    let mut table = table_2_by_3();
    table.insert_row(2).unwrap();
    table.rows[2].cells[0] = TableCellPayload::plain("g");
    table
        .merge_cells(TableRange::normalized(0, 0, 1, 1))
        .unwrap();

    assert!(table.move_row(2, 0).unwrap());

    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("g"));
    assert_eq!(
        table.rows[1].cells[0].merge,
        TableCellMerge::Origin {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(
        table.rows[2].cells[1].merge,
        TableCellMerge::Covered {
            origin_row: 1,
            origin_col: 0
        }
    );
    assert_eq!(table.cell_origin(2, 1), Some((1, 0)));
}

#[test]
fn move_column_remaps_merge_metadata_when_merged_range_stays_contiguous() {
    let mut table = table_2_by_3();
    table
        .merge_cells(TableRange::normalized(0, 0, 1, 1))
        .unwrap();

    assert!(table.move_column(2, 0).unwrap());

    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("c"));
    assert_eq!(
        table.rows[0].cells[1].merge,
        TableCellMerge::Origin {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(
        table.rows[1].cells[2].merge,
        TableCellMerge::Covered {
            origin_row: 0,
            origin_col: 1
        }
    );
    assert_eq!(table.cell_origin(1, 2), Some((0, 1)));
}

#[test]
fn move_row_or_column_rejects_moves_that_split_merged_origin_order() {
    let mut row_table = table_2_by_3();
    row_table
        .merge_cells(TableRange::normalized(0, 0, 1, 1))
        .unwrap();
    assert!(row_table.move_row(0, 1).is_err());

    let mut column_table = table_2_by_3();
    column_table
        .merge_cells(TableRange::normalized(0, 0, 1, 1))
        .unwrap();
    assert!(column_table.move_column(0, 1).is_err());
}

#[test]
fn plain_text_exports_rows_with_tabs_and_newlines() {
    let table = table_2_by_3();

    assert_eq!(table.plain_text(), "a\tb\tc\nd\te\tf");
}

#[test]
fn plain_text_exports_covered_merged_cells_as_empty_slots() {
    let mut table = table_2_by_3();
    table
        .merge_cells(TableRange::normalized(0, 0, 1, 1))
        .unwrap();

    assert_eq!(table.plain_text(), "a\tb\nd\te\t\tc\n\t\tf");
}

#[test]
fn merge_cells_marks_origin_and_covered_cells() {
    let mut table = table_2_by_3();

    assert!(
        table
            .merge_cells(TableRange::normalized(0, 0, 1, 1))
            .unwrap()
    );

    assert_eq!(
        table.rows[0].cells[0].merge,
        TableCellMerge::Origin {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(
        table.rows[1].cells[1].merge,
        TableCellMerge::Covered {
            origin_row: 0,
            origin_col: 0
        }
    );
    assert_eq!(table.cell_origin(1, 1), Some((0, 0)));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("a\tb\nd\te"));
    assert_eq!(plain_text_from_spans(&table.rows[1].cells[1].spans), "");
    assert_eq!(table.visible_cells().count(), 3);
}

#[test]
fn split_cell_restores_covered_cells_with_content_only_in_origin() {
    let mut table = table_2_by_3();
    table
        .merge_cells(TableRange::normalized(0, 0, 1, 1))
        .unwrap();

    assert!(table.split_cell(1, 1).unwrap());

    assert_eq!(table.rows[0].cells[0].merge, TableCellMerge::Unmerged);
    assert_eq!(table.rows[1].cells[1].merge, TableCellMerge::Unmerged);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a\tb\nd\te"));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some(""));
    assert_eq!(table.visible_cells().count(), 6);
}

#[test]
fn merge_rejects_existing_merged_cells() {
    let mut table = table_2_by_3();
    table
        .merge_cells(TableRange::normalized(0, 0, 1, 1))
        .unwrap();

    assert!(
        table
            .merge_cells(TableRange::normalized(0, 1, 1, 2))
            .is_err()
    );
}
