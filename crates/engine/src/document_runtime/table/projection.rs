use super::*;

pub(in crate::document_runtime) fn table_view_state_from_payload(
    table: &cditor_core::rich_text::TablePayload,
    focused_cell: Option<TableCellPosition>,
    focused_cell_offset: Option<usize>,
) -> TableViewState {
    let geometry = table_layout_from_payload(table);
    let visible_cells = table
        .visible_cells()
        .map(|(row, col, cell)| {
            let (row_span, col_span) = match cell.merge {
                TableCellMerge::Origin { row_span, col_span } => (row_span, col_span),
                TableCellMerge::Unmerged | TableCellMerge::Covered { .. } => (1, 1),
            };
            TableVisibleCell {
                position: TableCellPosition { row, col },
                row_span,
                col_span,
                x_px: geometry.x_offsets.get(col).copied().unwrap_or(0.0),
                y_px: geometry.y_offsets.get(row).copied().unwrap_or(0.0),
                width_px: span_size(&geometry.column_widths, col, col_span),
                height_px: span_size(&geometry.row_heights, row, row_span),
                header: is_table_header_cell(table, row, col),
                align: cell.align,
                background_color: cell.style.background_color.clone(),
                spans: cell.spans.clone(),
            }
        })
        .collect::<Vec<_>>();
    TableViewState {
        table: table.clone(),
        row_count: geometry.row_count,
        col_count: geometry.col_count,
        width_px: geometry.width_px,
        height_px: geometry.height_px,
        visible_cells,
        focused_cell,
        focused_cell_offset,
    }
}

fn is_table_header_cell(
    table: &cditor_core::rich_text::TablePayload,
    row: usize,
    col: usize,
) -> bool {
    row < table.header_rows.max(usize::from(table.header_rows == 0)) || col < table.header_cols
}
