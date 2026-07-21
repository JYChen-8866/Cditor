use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::document_runtime) enum TableSelectionAxis {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::document_runtime) struct TableSelection {
    pub block_id: BlockId,
    pub range: TableRange,
}

impl TableSelection {
    pub(in crate::document_runtime) fn range(block_id: BlockId, range: TableRange) -> Self {
        Self { block_id, range }
    }

    pub(in crate::document_runtime) fn cell(block_id: BlockId, row: usize, col: usize) -> Self {
        Self::range(block_id, TableRange::normalized(row, col, row, col))
    }

    pub(in crate::document_runtime) fn whole_table(
        block_id: BlockId,
        row_count: usize,
        col_count: usize,
    ) -> Option<Self> {
        if row_count == 0 || col_count == 0 {
            return None;
        }
        Some(Self::range(
            block_id,
            TableRange::normalized(0, 0, row_count - 1, col_count - 1),
        ))
    }

    pub(in crate::document_runtime) fn axis(
        block_id: BlockId,
        axis: TableSelectionAxis,
        index: usize,
        row_count: usize,
        col_count: usize,
    ) -> Option<Self> {
        if row_count == 0 || col_count == 0 {
            return None;
        }
        match axis {
            TableSelectionAxis::Row => (index < row_count).then(|| {
                Self::range(
                    block_id,
                    TableRange::normalized(index, 0, index, col_count - 1),
                )
            }),
            TableSelectionAxis::Column => (index < col_count).then(|| {
                Self::range(
                    block_id,
                    TableRange::normalized(0, index, row_count - 1, index),
                )
            }),
        }
    }

    pub(in crate::document_runtime) fn contains_cell(
        self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> bool {
        self.block_id == block_id
            && row >= self.range.start_row
            && row <= self.range.end_row
            && col >= self.range.start_col
            && col <= self.range.end_col
    }

    pub(in crate::document_runtime) fn selects_whole_row(
        self,
        row: usize,
        col_count: usize,
    ) -> bool {
        col_count > 0
            && self.range.start_row == row
            && self.range.end_row == row
            && self.range.start_col == 0
            && self.range.end_col >= col_count - 1
    }

    pub(in crate::document_runtime) fn selects_whole_column(
        self,
        col: usize,
        row_count: usize,
    ) -> bool {
        row_count > 0
            && self.range.start_col == col
            && self.range.end_col == col
            && self.range.start_row == 0
            && self.range.end_row >= row_count - 1
    }
}

impl DocumentRuntime {
    pub fn table_row_selection_range(&self, block_id: BlockId, row: usize) -> Option<TableRange> {
        let table = self.table_runtime(block_id)?.table();
        let selection = TableSelection::axis(
            block_id,
            TableSelectionAxis::Row,
            row,
            table.row_count(),
            table.column_count(),
        )?;
        debug_assert!(selection.selects_whole_row(row, table.column_count()));
        Some(selection.range)
    }

    pub fn table_column_selection_range(
        &self,
        block_id: BlockId,
        col: usize,
    ) -> Option<TableRange> {
        let table = self.table_runtime(block_id)?.table();
        let selection = TableSelection::axis(
            block_id,
            TableSelectionAxis::Column,
            col,
            table.row_count(),
            table.column_count(),
        )?;
        debug_assert!(selection.selects_whole_column(col, table.row_count()));
        Some(selection.range)
    }

    pub fn table_cell_selection_range(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Option<TableRange> {
        let table = self.table_runtime(block_id)?.table();
        if row >= table.row_count() || col >= table.column_count() {
            return None;
        }
        let selection = TableSelection::cell(block_id, row, col);
        debug_assert!(selection.contains_cell(block_id, row, col));
        Some(selection.range)
    }

    pub fn table_range_selection_range(
        &self,
        block_id: BlockId,
        range: TableRange,
    ) -> Option<TableRange> {
        let table = self.table_runtime(block_id)?.table();
        if table.row_count() == 0
            || table.column_count() == 0
            || range.start_row >= table.row_count()
            || range.end_row >= table.row_count()
            || range.start_col >= table.column_count()
            || range.end_col >= table.column_count()
        {
            return None;
        }
        let selection = TableSelection::range(block_id, range);
        debug_assert!(selection.contains_cell(block_id, range.start_row, range.start_col));
        debug_assert!(selection.contains_cell(block_id, range.end_row, range.end_col));
        Some(selection.range)
    }

    pub fn whole_table_selection_range(&self, block_id: BlockId) -> Option<TableRange> {
        let table = self.table_runtime(block_id)?.table();
        let selection =
            TableSelection::whole_table(block_id, table.row_count(), table.column_count())?;
        Some(selection.range)
    }
}
