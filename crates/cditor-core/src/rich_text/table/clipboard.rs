use crate::rich_text::{
    TableCellMerge, TableCellPayload, TableColumnPayload, TablePayload, TableRange, TableRowPayload,
};

pub fn paste_table_at(
    table: &mut TablePayload,
    start_row: usize,
    start_col: usize,
    source: &TablePayload,
) -> Result<bool, String> {
    let row_count = source.row_count();
    let col_count = source.column_count();
    if row_count == 0 || col_count == 0 {
        return Ok(false);
    }
    let end_row = start_row
        .checked_add(row_count - 1)
        .ok_or_else(|| "table paste row range overflow".to_owned())?;
    let end_col = start_col
        .checked_add(col_count - 1)
        .ok_or_else(|| "table paste column range overflow".to_owned())?;
    let target = TableRange::normalized(start_row, start_col, end_row, end_col);

    let mut next = table.clone();
    ensure_size(&mut next, end_row + 1, end_col + 1);
    clear_merges_intersecting(&mut next, target);
    apply_table_clipboard(&mut next, start_row, start_col, source)?;
    next.normalize();

    if next == *table {
        return Ok(false);
    }
    *table = next;
    Ok(true)
}

fn ensure_size(table: &mut TablePayload, rows: usize, cols: usize) {
    table.normalize();
    while table.rows.len() < rows {
        let mut row = TableRowPayload::default();
        row.cells
            .resize_with(table.column_count().max(cols), TableCellPayload::default);
        table.rows.push(row);
    }
    if table.columns.len() < cols {
        table.columns.resize_with(cols, TableColumnPayload::default);
    }
    for row in &mut table.rows {
        if row.cells.len() < cols {
            row.cells.resize_with(cols, TableCellPayload::default);
        }
    }
    table.normalize();
}

fn apply_table_clipboard(
    table: &mut TablePayload,
    start_row: usize,
    start_col: usize,
    source: &TablePayload,
) -> Result<(), String> {
    let mut source = source.clone();
    source.normalize();
    for (local_col, column) in source.columns.iter().enumerate() {
        if let Some(target) = table.columns.get_mut(start_col + local_col) {
            *target = column.clone();
        }
    }
    for (local_row, source_row) in source.rows.iter().enumerate() {
        let target_row_index = start_row + local_row;
        let Some(target_row) = table.rows.get_mut(target_row_index) else {
            return Err(format!("missing target paste row {target_row_index}"));
        };
        target_row.height = source_row.height;
        for (local_col, source_cell) in source_row.cells.iter().enumerate() {
            let target_col_index = start_col + local_col;
            let Some(target_cell) = target_row.cells.get_mut(target_col_index) else {
                return Err(format!(
                    "missing target paste cell {target_row_index}:{target_col_index}"
                ));
            };
            let mut cell = source_cell.clone();
            if let TableCellMerge::Covered {
                origin_row,
                origin_col,
            } = cell.merge
            {
                cell.merge = TableCellMerge::Covered {
                    origin_row: start_row + origin_row,
                    origin_col: start_col + origin_col,
                };
            }
            *target_cell = cell;
        }
    }
    Ok(())
}

fn clear_merges_intersecting(table: &mut TablePayload, range: TableRange) {
    let mut origins = Vec::new();
    for row in 0..table.row_count() {
        for col in 0..table.column_count() {
            let Some((origin_row, origin_col)) = table.cell_origin(row, col) else {
                continue;
            };
            if origins.contains(&(origin_row, origin_col)) {
                continue;
            }
            let Some(span) = cell_span_range(table, origin_row, origin_col) else {
                continue;
            };
            if table_ranges_intersect(span, range) {
                origins.push((origin_row, origin_col));
            }
        }
    }

    for (origin_row, origin_col) in origins {
        let Some(span) = cell_span_range(table, origin_row, origin_col) else {
            continue;
        };
        for row in span.start_row..=span.end_row {
            for col in span.start_col..=span.end_col {
                if let Some(cell) = table
                    .rows
                    .get_mut(row)
                    .and_then(|row| row.cells.get_mut(col))
                {
                    cell.merge = TableCellMerge::Unmerged;
                }
            }
        }
    }
}

fn cell_span_range(table: &TablePayload, row: usize, col: usize) -> Option<TableRange> {
    let cell = table.rows.get(row)?.cells.get(col)?;
    match cell.merge {
        TableCellMerge::Origin { row_span, col_span } => Some(TableRange::normalized(
            row,
            col,
            row + row_span.saturating_sub(1),
            col + col_span.saturating_sub(1),
        )),
        TableCellMerge::Unmerged => Some(TableRange::normalized(row, col, row, col)),
        TableCellMerge::Covered { .. } => None,
    }
}

fn table_ranges_intersect(a: TableRange, b: TableRange) -> bool {
    a.start_row <= b.end_row
        && a.end_row >= b.start_row
        && a.start_col <= b.end_col
        && a.end_col >= b.start_col
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_text::{TableCellAlign, TableHeaderStyle, TableTrackSize};

    fn table(rows: &[&[&str]]) -> TablePayload {
        let mut table = TablePayload {
            rows: rows
                .iter()
                .map(|row| TableRowPayload {
                    cells: row
                        .iter()
                        .map(|cell| TableCellPayload::plain(*cell))
                        .collect(),
                    height: TableTrackSize::Auto,
                })
                .collect(),
            columns: Vec::new(),
            header_rows: 0,
            header_cols: 0,
            header_style: TableHeaderStyle::default(),
        };
        table.normalize();
        table
    }

    #[test]
    fn paste_table_at_replaces_range_and_expands_target() {
        let mut target = table(&[&["a"]]);
        let mut source = table(&[&["b", "c"], &["d", "e"]]);
        source.rows[0].height = TableTrackSize::Px(48);
        source.columns[1].width = TableTrackSize::Px(160);
        source.rows[1].cells[0].align = TableCellAlign::Right;

        assert!(paste_table_at(&mut target, 1, 1, &source).unwrap());

        assert_eq!(target.row_count(), 3);
        assert_eq!(target.column_count(), 3);
        assert_eq!(target.cell_plain_text(1, 1).as_deref(), Some("b"));
        assert_eq!(target.cell_plain_text(2, 2).as_deref(), Some("e"));
        assert_eq!(target.rows[1].height, TableTrackSize::Px(48));
        assert_eq!(target.columns[2].width, TableTrackSize::Px(160));
        assert_eq!(target.rows[2].cells[1].align, TableCellAlign::Right);
    }

    #[test]
    fn paste_table_at_preserves_source_merge_and_clears_intersecting_target_merge() {
        let mut target = table(&[&["a", "b"], &["c", "d"]]);
        target
            .merge_cells(TableRange::normalized(0, 0, 1, 1))
            .unwrap();

        let mut source = table(&[&["x", "y"], &["z", "w"]]);
        source
            .merge_cells(TableRange::normalized(0, 0, 1, 1))
            .unwrap();

        assert!(paste_table_at(&mut target, 0, 0, &source).unwrap());

        assert_eq!(
            target.rows[0].cells[0].merge,
            TableCellMerge::Origin {
                row_span: 2,
                col_span: 2,
            }
        );
        assert_eq!(
            target.rows[1].cells[1].merge,
            TableCellMerge::Covered {
                origin_row: 0,
                origin_col: 0,
            }
        );
        assert_eq!(target.cell_plain_text(1, 1).as_deref(), Some("x\ty\nz\tw"));
    }
}
