use cditor_core::ids::BlockId;
use cditor_core::rich_text::TableRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableAxis {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TableAxisSelection {
    pub block_id: BlockId,
    pub axis: TableAxis,
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TableCellRangeSelection {
    pub block_id: BlockId,
    pub anchor_row: usize,
    pub anchor_col: usize,
    pub focus_row: usize,
    pub focus_col: usize,
    pub range: TableRange,
}

impl TableCellRangeSelection {
    pub(crate) fn new(
        block_id: BlockId,
        anchor_row: usize,
        anchor_col: usize,
        focus_row: usize,
        focus_col: usize,
    ) -> Self {
        Self {
            block_id,
            anchor_row,
            anchor_col,
            focus_row,
            focus_col,
            range: TableRange::normalized(anchor_row, anchor_col, focus_row, focus_col),
        }
    }

    pub(super) fn selects_cell(self, block_id: BlockId, row: usize, col: usize) -> bool {
        self.block_id == block_id
            && row >= self.range.start_row
            && row <= self.range.end_row
            && col >= self.range.start_col
            && col <= self.range.end_col
    }

    pub(crate) fn is_multi_cell(self) -> bool {
        self.range.row_count() > 1 || self.range.col_count() > 1
    }
}

impl TableAxisSelection {
    pub(crate) fn new(block_id: BlockId, axis: TableAxis, index: usize) -> Self {
        Self {
            block_id,
            axis,
            index,
        }
    }

    pub(super) fn selects_cell(self, block_id: BlockId, row: usize, col: usize) -> bool {
        if self.block_id != block_id {
            return false;
        }
        match self.axis {
            TableAxis::Row => self.index == row,
            TableAxis::Column => self.index == col,
        }
    }

    pub(super) fn selects_row_handle(self, block_id: BlockId, row: usize) -> bool {
        self.block_id == block_id && self.axis == TableAxis::Row && self.index == row
    }

    pub(super) fn selects_column_handle(self, block_id: BlockId, col: usize) -> bool {
        self.block_id == block_id && self.axis == TableAxis::Column && self.index == col
    }
}

#[cfg(test)]
pub(super) fn cell_selected(
    axis_selection: Option<TableAxisSelection>,
    range_selection: Option<TableCellRangeSelection>,
    block_id: BlockId,
    row: usize,
    col: usize,
) -> bool {
    range_selection
        .map(|selection| selection.selects_cell(block_id, row, col))
        .unwrap_or(false)
        || axis_selection
            .map(|selection| selection.selects_cell(block_id, row, col))
            .unwrap_or(false)
}

#[cfg(test)]
pub(super) fn row_handle_selected(
    selection: Option<TableAxisSelection>,
    block_id: BlockId,
    row: usize,
) -> bool {
    selection
        .map(|selection| selection.selects_row_handle(block_id, row))
        .unwrap_or(false)
}

#[cfg(test)]
pub(super) fn column_handle_selected(
    selection: Option<TableAxisSelection>,
    block_id: BlockId,
    col: usize,
) -> bool {
    selection
        .map(|selection| selection.selects_column_handle(block_id, col))
        .unwrap_or(false)
}
