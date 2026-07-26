use cditor_core::rich_text::{TableCellPayload, TableColumnPayload, TablePayload, TableRowPayload};

use super::*;

#[test]
fn table_axis_menu_matches_notion_row_and_column_action_order() {
    let row_items = table_axis_menu_items(TableAxisSelection::new(7, TableAxis::Row, 1));
    let column_items = table_axis_menu_items(TableAxisSelection::new(7, TableAxis::Column, 1));

    assert_eq!(row_items.len(), 7);
    assert_eq!(row_items[0].label, "标题行");
    assert_eq!(row_items[1].action, TableMenuAction::BackgroundColor);
    assert_eq!(row_items[6].action, TableMenuAction::DeleteRow);
    assert_eq!(column_items.len(), 7);
    assert_eq!(column_items[0].label, "标题列");
    assert_eq!(column_items[2].action, TableMenuAction::InsertColumnLeft);
    assert_eq!(column_items[6].action, TableMenuAction::DeleteColumn);
}

#[test]
fn table_menu_filter_matches_chinese_labels_and_english_keywords() {
    let items = table_axis_menu_items(TableAxisSelection::new(7, TableAxis::Column, 1));

    assert_eq!(filter_table_menu_items(&items, "右侧").len(), 1);
    assert_eq!(filter_table_menu_items(&items, "insert").len(), 2);
    assert!(
        filter_table_menu_items(&items, "fill")
            .iter()
            .any(|item| item.action == TableMenuAction::BackgroundColor)
    );
    assert!(filter_table_menu_items(&items, "zzz").is_empty());
}

#[test]
fn table_menu_query_edits_unicode_on_character_boundaries() {
    let mut state = TableMenuUiState::default();
    state.replace_range(state.input_replacement_range(), "颜色a");
    assert_eq!(state.caret_offset, "颜色a".len());

    state.move_left();
    state.delete_backward();
    assert_eq!(state.query, "颜a");
    assert_eq!(state.caret_offset, "颜".len());

    state.delete_forward();
    assert_eq!(state.query, "颜");
    state.move_to_end();
    state.delete_backward();
    assert_eq!(state.query, "");
}

#[test]
fn table_menu_query_tracks_ime_marked_range() {
    let mut state = TableMenuUiState::default();
    state.replace_and_mark_range(0..0, "颜", Some("颜".len().."颜".len()));

    assert_eq!(state.query, "颜");
    assert_eq!(state.marked_range, Some(0.."颜".len()));
    assert_eq!(state.input_replacement_range(), 0.."颜".len());

    state.unmark();
    assert_eq!(state.marked_range, None);
}

#[test]
fn row_insert_count_is_clamped_and_resets_with_menu_state() {
    let mut state = TableMenuUiState::default();
    assert_eq!(state.row_insert_count(TableRowInsertDirection::Above), 1);
    assert!(!state.decrement_row_insert_count(TableRowInsertDirection::Above));

    assert!(state.set_row_insert_count(TableRowInsertDirection::Above, 12));
    assert_eq!(state.row_insert_count(TableRowInsertDirection::Above), 12);
    assert_eq!(state.row_insert_count(TableRowInsertDirection::Below), 1);
    assert!(state.set_row_insert_count(TableRowInsertDirection::Below, usize::MAX));
    assert_eq!(
        state.row_insert_count(TableRowInsertDirection::Below),
        TABLE_MENU_MAX_ROW_INSERT_COUNT
    );
    assert!(!state.increment_row_insert_count(TableRowInsertDirection::Below));
    assert_eq!(
        TableMenuUiState::default().row_insert_count(TableRowInsertDirection::Above),
        1
    );
}

#[test]
fn table_menu_height_includes_search_and_is_scroll_limited() {
    let fixed =
        TABLE_MENU_PADDING_PX * 2.0 + TABLE_MENU_SEARCH_HEIGHT_PX + TABLE_MENU_SEARCH_GAP_PX;
    assert_eq!(table_menu_panel_height(0), fixed + TABLE_MENU_ROW_HEIGHT_PX);
    assert_eq!(
        table_menu_panel_height(3),
        fixed + TABLE_MENU_ROW_HEIGHT_PX * 3.0
    );
    assert_eq!(
        table_menu_panel_height(30),
        fixed + TABLE_MENU_ROW_HEIGHT_PX * TABLE_MENU_MAX_VISIBLE_ROWS as f32
    );
}

#[test]
fn table_menu_position_keeps_gutter_left_alignment_and_flips_vertically() {
    let aligned = table_menu_position(-28.0, 10.0, 16.0, 3, 960.0, 640.0);
    assert_eq!(aligned.x, -28.0);
    assert!(!aligned.placed_above);

    let right_clamped = table_menu_position(900.0, 100.0, 16.0, 3, 960.0, 640.0);
    assert_eq!(right_clamped.x, 688.0);

    let flipped = table_menu_position(120.0, 610.0, 16.0, 10, 960.0, 640.0);
    assert!(flipped.placed_above);
    assert!(flipped.y < 610.0);
}

#[test]
fn header_toggle_reflects_table_axis_metadata() {
    let mut table_view = basic_table_view();
    table_view.table.header_rows = 1;
    table_view.table.header_cols = 0;

    assert!(table_axis_header_enabled(
        TableAxisSelection::new(7, TableAxis::Row, 1),
        &table_view,
    ));
    assert!(!table_axis_header_enabled(
        TableAxisSelection::new(7, TableAxis::Column, 1),
        &table_view,
    ));
}

#[test]
fn merged_table_disables_only_unsafe_structure_actions() {
    let mut table_view = basic_table_view();
    table_view
        .table
        .merge_cells(cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1))
        .unwrap();
    let selection = TableAxisSelection::new(7, TableAxis::Row, 0);

    assert!(!table_menu_action_enabled(
        TableMenuAction::InsertRowBelow,
        selection,
        &table_view
    ));
    assert!(!table_menu_action_enabled(
        TableMenuAction::DeleteColumn,
        selection,
        &table_view
    ));
    assert!(table_menu_action_enabled(
        TableMenuAction::ClearContents,
        selection,
        &table_view
    ));
    assert!(table_menu_action_enabled(
        TableMenuAction::ToggleHeader,
        selection,
        &table_view
    ));
}

fn basic_table_view() -> TableViewState {
    let table = TablePayload {
        rows: vec![
            TableRowPayload {
                cells: vec![TableCellPayload::plain("A"), TableCellPayload::plain("B")],
                height: Default::default(),
            },
            TableRowPayload {
                cells: vec![TableCellPayload::plain("C"), TableCellPayload::plain("D")],
                height: Default::default(),
            },
        ],
        columns: vec![TableColumnPayload::default(), TableColumnPayload::default()],
        ..TablePayload::default()
    };
    TableViewState {
        table: table.into(),
        row_count: 2,
        col_count: 2,
        width_px: 240.0,
        height_px: 72.0,
        column_widths_px: vec![120.0, 120.0],
        row_heights_px: vec![36.0, 36.0],
        horizontal_scroll_offset_px: 0.0,
        visible_cells: Vec::new(),
        focused_cell: None,
        focused_cell_offset: None,
        focused_cell_affinity: None,
        focused_cell_selection_range: None,
    }
}
