use super::*;

impl TablePayload {
    pub fn insert_row(&mut self, index: usize) -> Result<bool, String> {
        self.ensure_unmerged_table_for_structure_edit()?;
        let col_count = self.column_count();
        let row_count = self.row_count();
        if index > row_count {
            return Err(format!(
                "table row insert out of bounds rows={} index={index}",
                row_count
            ));
        }
        let mut row = TableRowPayload::default();
        row.cells.resize_with(col_count, TableCellPayload::default);
        self.rows.insert(index, row);
        if index < self.header_rows {
            self.header_rows = self.header_rows.saturating_add(1);
        }
        self.normalize();
        Ok(true)
    }

    pub fn delete_row(&mut self, index: usize) -> Result<bool, String> {
        self.ensure_unmerged_table_for_structure_edit()?;
        let row_count = self.row_count();
        if index >= row_count {
            return Err(format!(
                "table row delete out of bounds rows={} index={index}",
                row_count
            ));
        }
        if row_count <= 1 {
            return Err("cannot delete the last table row".to_owned());
        }
        self.rows.remove(index);
        if index < self.header_rows {
            self.header_rows = self.header_rows.saturating_sub(1);
        }
        self.normalize();
        Ok(true)
    }

    pub fn duplicate_row(&mut self, index: usize) -> Result<bool, String> {
        self.ensure_unmerged_table_for_structure_edit()?;
        let row_count = self.row_count();
        if index >= row_count {
            return Err(format!(
                "table row duplicate out of bounds rows={} index={index}",
                row_count
            ));
        }
        let row = self
            .rows
            .get(index)
            .cloned()
            .ok_or_else(|| format!("missing table row {index}"))?;
        self.rows.insert(index.saturating_add(1), row);
        if index < self.header_rows {
            self.header_rows = self.header_rows.saturating_add(1);
        }
        self.normalize();
        Ok(true)
    }

    pub fn move_row(&mut self, from: usize, to: usize) -> Result<bool, String> {
        let row_count = self.row_count();
        if from >= row_count || to >= row_count {
            return Err(format!(
                "table row move out of bounds rows={} from={from} to={to}",
                row_count
            ));
        }
        if from == to {
            return Ok(false);
        }
        self.ensure_axis_move_preserves_merges(TableMoveAxis::Row, from, to)?;
        let row = self.rows.remove(from);
        self.rows.insert(to, row);
        self.remap_merge_origins_after_axis_move(TableMoveAxis::Row, from, to);
        self.normalize();
        Ok(true)
    }

    pub fn insert_column(&mut self, index: usize) -> Result<bool, String> {
        self.ensure_unmerged_table_for_structure_edit()?;
        self.normalize();
        let col_count = self.column_count();
        if index > col_count {
            return Err(format!(
                "table column insert out of bounds cols={} index={index}",
                col_count
            ));
        }
        self.columns.insert(index, TableColumnPayload::default());
        for row in &mut self.rows {
            row.cells.insert(index, TableCellPayload::default());
        }
        if index < self.header_cols {
            self.header_cols = self.header_cols.saturating_add(1);
        }
        self.normalize();
        Ok(true)
    }

    pub fn delete_column(&mut self, index: usize) -> Result<bool, String> {
        self.ensure_unmerged_table_for_structure_edit()?;
        self.normalize();
        let col_count = self.column_count();
        if index >= col_count {
            return Err(format!(
                "table column delete out of bounds cols={} index={index}",
                col_count
            ));
        }
        if col_count <= 1 {
            return Err("cannot delete the last table column".to_owned());
        }
        self.columns.remove(index);
        for row in &mut self.rows {
            row.cells.remove(index);
        }
        if index < self.header_cols {
            self.header_cols = self.header_cols.saturating_sub(1);
        }
        self.normalize();
        Ok(true)
    }

    pub fn duplicate_column(&mut self, index: usize) -> Result<bool, String> {
        self.ensure_unmerged_table_for_structure_edit()?;
        self.normalize();
        let col_count = self.column_count();
        if index >= col_count {
            return Err(format!(
                "table column duplicate out of bounds cols={} index={index}",
                col_count
            ));
        }
        let column = self
            .columns
            .get(index)
            .cloned()
            .ok_or_else(|| format!("missing table column {index}"))?;
        self.columns.insert(index.saturating_add(1), column);
        for row in &mut self.rows {
            let cell = row
                .cells
                .get(index)
                .cloned()
                .ok_or_else(|| format!("missing table cell column {index}"))?;
            row.cells.insert(index.saturating_add(1), cell);
        }
        if index < self.header_cols {
            self.header_cols = self.header_cols.saturating_add(1);
        }
        self.normalize();
        Ok(true)
    }

    pub fn move_column(&mut self, from: usize, to: usize) -> Result<bool, String> {
        self.normalize();
        let col_count = self.column_count();
        if from >= col_count || to >= col_count {
            return Err(format!(
                "table column move out of bounds cols={} from={from} to={to}",
                col_count
            ));
        }
        if from == to {
            return Ok(false);
        }
        self.ensure_axis_move_preserves_merges(TableMoveAxis::Column, from, to)?;
        let column = self.columns.remove(from);
        self.columns.insert(to, column);
        for row in &mut self.rows {
            let cell = row.cells.remove(from);
            row.cells.insert(to, cell);
        }
        self.remap_merge_origins_after_axis_move(TableMoveAxis::Column, from, to);
        self.normalize();
        Ok(true)
    }

    fn ensure_axis_move_preserves_merges(
        &self,
        axis: TableMoveAxis,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        for (row_index, row) in self.rows.iter().enumerate() {
            for (col_index, cell) in row.cells.iter().enumerate() {
                let TableCellMerge::Origin { row_span, col_span } = cell.merge else {
                    continue;
                };
                let origin_index = axis.index(row_index, col_index);
                let span = axis.span(row_span, col_span);
                let mapped_origin = remap_moved_axis_index(origin_index, from, to);
                let mapped_indexes = (origin_index..origin_index.saturating_add(span))
                    .map(|index| remap_moved_axis_index(index, from, to))
                    .collect::<Vec<_>>();
                let Some(min_index) = mapped_indexes.iter().copied().min() else {
                    continue;
                };
                let Some(max_index) = mapped_indexes.iter().copied().max() else {
                    continue;
                };
                let remains_contiguous =
                    max_index.saturating_sub(min_index).saturating_add(1) == span;
                let origin_remains_first = mapped_origin == min_index;
                if !remains_contiguous || !origin_remains_first {
                    return Err(format!(
                        "cannot move {axis:?} {from} to {to}; merged cell at {row_index}:{col_index} would be split"
                    ));
                }
            }
        }
        Ok(())
    }

    fn remap_merge_origins_after_axis_move(&mut self, axis: TableMoveAxis, from: usize, to: usize) {
        for row in &mut self.rows {
            for cell in &mut row.cells {
                if let TableCellMerge::Covered {
                    origin_row,
                    origin_col,
                } = &mut cell.merge
                {
                    match axis {
                        TableMoveAxis::Row => {
                            *origin_row = remap_moved_axis_index(*origin_row, from, to)
                        }
                        TableMoveAxis::Column => {
                            *origin_col = remap_moved_axis_index(*origin_col, from, to)
                        }
                    }
                }
            }
        }
    }

    fn ensure_unmerged_table_for_structure_edit(&self) -> Result<(), String> {
        for (row_index, row) in self.rows.iter().enumerate() {
            for (col_index, cell) in row.cells.iter().enumerate() {
                if !matches!(cell.merge, TableCellMerge::Unmerged) {
                    return Err(format!(
                        "cannot structurally edit table with merged cell {row_index}:{col_index}"
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableMoveAxis {
    Row,
    Column,
}

impl TableMoveAxis {
    fn index(self, row: usize, col: usize) -> usize {
        match self {
            Self::Row => row,
            Self::Column => col,
        }
    }

    fn span(self, row_span: usize, col_span: usize) -> usize {
        match self {
            Self::Row => row_span,
            Self::Column => col_span,
        }
    }
}

fn remap_moved_axis_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < to && index > from && index <= to {
        index - 1
    } else if to < from && index >= to && index < from {
        index + 1
    } else {
        index
    }
}
