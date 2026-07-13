use super::{InlineSpan, plain_text_from_spans};
use serde::{Deserialize, Serialize};

mod clipboard;
mod structure;
mod style;

pub use style::{TableCellStyle, TableHeaderStyle};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TablePayload {
    pub rows: Vec<TableRowPayload>,
    pub columns: Vec<TableColumnPayload>,
    pub header_rows: usize,
    pub header_cols: usize,
    pub header_style: TableHeaderStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableRowPayload {
    pub cells: Vec<TableCellPayload>,
    pub height: TableTrackSize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableColumnPayload {
    pub width: TableTrackSize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableCellPayload {
    pub spans: Vec<InlineSpan>,
    pub align: TableCellAlign,
    pub merge: TableCellMerge,
    pub style: TableCellStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TableTrackSize {
    #[default]
    Auto,
    Px(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TableCellAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TableCellMerge {
    #[default]
    Unmerged,
    Origin {
        row_span: usize,
        col_span: usize,
    },
    Covered {
        origin_row: usize,
        origin_col: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRange {
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
}

impl TableRange {
    pub fn normalized(row_a: usize, col_a: usize, row_b: usize, col_b: usize) -> Self {
        Self {
            start_row: row_a.min(row_b),
            start_col: col_a.min(col_b),
            end_row: row_a.max(row_b),
            end_col: col_a.max(col_b),
        }
    }

    pub fn row_count(self) -> usize {
        self.end_row
            .saturating_sub(self.start_row)
            .saturating_add(1)
    }

    pub fn col_count(self) -> usize {
        self.end_col
            .saturating_sub(self.start_col)
            .saturating_add(1)
    }
}

impl TableCellPayload {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![InlineSpan::plain(text)],
            align: TableCellAlign::Left,
            merge: TableCellMerge::Unmerged,
            style: TableCellStyle::default(),
        }
    }
}

impl TablePayload {
    pub fn set_header_rows(&mut self, count: usize) -> bool {
        let count = count.min(self.row_count());
        if self.header_rows == count {
            return false;
        }
        self.header_rows = count;
        true
    }

    pub fn set_header_columns(&mut self, count: usize) -> bool {
        let count = count.min(self.column_count());
        if self.header_cols == count {
            return false;
        }
        self.header_cols = count;
        true
    }

    pub fn normalize(&mut self) {
        let columns = self.column_count();
        if self.columns.len() < columns {
            self.columns
                .resize_with(columns, TableColumnPayload::default);
        } else if self.columns.len() > columns {
            self.columns.truncate(columns);
        }
        for row in &mut self.rows {
            if row.cells.len() < columns {
                row.cells.resize_with(columns, TableCellPayload::default);
            } else if row.cells.len() > columns {
                row.cells.truncate(columns);
            }
        }
        self.normalize_merge_spans();
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn column_count(&self) -> usize {
        self.rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(0)
            .max(self.columns.len())
    }

    pub fn cell_plain_text(&self, row: usize, col: usize) -> Option<String> {
        let (row, col) = self.cell_origin(row, col)?;
        self.rows
            .get(row)
            .and_then(|row| row.cells.get(col))
            .map(|cell| plain_text_from_spans(&cell.spans))
    }

    pub fn plain_text(&self) -> String {
        self.rows
            .iter()
            .enumerate()
            .map(|(row_index, row)| {
                row.cells
                    .iter()
                    .enumerate()
                    .map(|(col_index, cell)| {
                        if self.cell_origin(row_index, col_index) == Some((row_index, col_index)) {
                            plain_text_from_spans(&cell.spans)
                        } else {
                            String::new()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn set_cell_plain_text(
        &mut self,
        row: usize,
        col: usize,
        text: impl Into<String>,
    ) -> Option<()> {
        let (row, col) = self.cell_origin(row, col)?;
        let cell = self.rows.get_mut(row)?.cells.get_mut(col)?;
        cell.spans = vec![InlineSpan::plain(text)];
        Some(())
    }

    pub fn set_cell_align(
        &mut self,
        range: TableRange,
        align: TableCellAlign,
    ) -> Result<bool, String> {
        self.ensure_range(range)?;
        let mut changed = false;
        for row in range.start_row..=range.end_row {
            for col in range.start_col..=range.end_col {
                let Some(cell) = self
                    .rows
                    .get_mut(row)
                    .and_then(|row| row.cells.get_mut(col))
                else {
                    continue;
                };
                if cell.align != align {
                    cell.align = align;
                    changed = true;
                }
            }
        }
        Ok(changed)
    }

    pub fn set_cell_background_color(
        &mut self,
        range: TableRange,
        background_color: Option<String>,
    ) -> Result<bool, String> {
        self.ensure_range(range)?;
        let mut changed = false;
        for row in range.start_row..=range.end_row {
            for col in range.start_col..=range.end_col {
                let Some(cell) = self
                    .rows
                    .get_mut(row)
                    .and_then(|row| row.cells.get_mut(col))
                else {
                    continue;
                };
                if cell.style.background_color != background_color {
                    cell.style.background_color = background_color.clone();
                    changed = true;
                }
            }
        }
        Ok(changed)
    }

    pub fn set_row_height(&mut self, row: usize, height: TableTrackSize) -> Result<bool, String> {
        let row_count = self.row_count();
        if row >= row_count {
            return Err(format!(
                "table row out of bounds rows={} row={row}",
                row_count
            ));
        }
        let Some(row_payload) = self.rows.get_mut(row) else {
            return Err(format!("missing table row {row}"));
        };
        if row_payload.height == height {
            return Ok(false);
        }
        row_payload.height = height;
        Ok(true)
    }

    pub fn set_column_width(&mut self, col: usize, width: TableTrackSize) -> Result<bool, String> {
        self.normalize();
        let col_count = self.column_count();
        if col >= col_count {
            return Err(format!(
                "table column out of bounds cols={} col={col}",
                col_count
            ));
        }
        let Some(column_payload) = self.columns.get_mut(col) else {
            return Err(format!("missing table column {col}"));
        };
        if column_payload.width == width {
            return Ok(false);
        }
        column_payload.width = width;
        Ok(true)
    }

    pub fn merge_cells(&mut self, range: TableRange) -> Result<bool, String> {
        self.ensure_range(range)?;
        if range.row_count() == 1 && range.col_count() == 1 {
            return Ok(false);
        }
        self.ensure_unmerged_range(range)?;
        let merged_text = self.merged_range_plain_text(range)?;
        let origin = self
            .rows
            .get_mut(range.start_row)
            .and_then(|row| row.cells.get_mut(range.start_col))
            .ok_or_else(|| "missing merge origin cell".to_owned())?;
        origin.spans = vec![InlineSpan::plain(merged_text)];
        origin.merge = TableCellMerge::Origin {
            row_span: range.row_count(),
            col_span: range.col_count(),
        };
        for row in range.start_row..=range.end_row {
            for col in range.start_col..=range.end_col {
                if row == range.start_row && col == range.start_col {
                    continue;
                }
                let cell = self
                    .rows
                    .get_mut(row)
                    .and_then(|row| row.cells.get_mut(col))
                    .ok_or_else(|| format!("missing covered table cell {row}:{col}"))?;
                cell.spans = vec![InlineSpan::plain("")];
                cell.merge = TableCellMerge::Covered {
                    origin_row: range.start_row,
                    origin_col: range.start_col,
                };
            }
        }
        Ok(true)
    }

    fn merged_range_plain_text(&self, range: TableRange) -> Result<String, String> {
        let mut rows = Vec::new();
        for row in range.start_row..=range.end_row {
            let mut cols = Vec::new();
            for col in range.start_col..=range.end_col {
                let text = self
                    .rows
                    .get(row)
                    .and_then(|row| row.cells.get(col))
                    .map(|cell| plain_text_from_spans(&cell.spans))
                    .ok_or_else(|| format!("missing table cell {row}:{col}"))?;
                cols.push(text);
            }
            rows.push(cols.join("\t"));
        }
        Ok(rows.join("\n"))
    }

    pub fn split_cell(&mut self, row: usize, col: usize) -> Result<bool, String> {
        let (origin_row, origin_col) = self
            .cell_origin(row, col)
            .ok_or_else(|| format!("missing table cell {row}:{col}"))?;
        let merge = self.rows[origin_row].cells[origin_col].merge;
        let TableCellMerge::Origin { row_span, col_span } = merge else {
            return Ok(false);
        };
        self.rows[origin_row].cells[origin_col].merge = TableCellMerge::Unmerged;
        for split_row in origin_row..origin_row + row_span {
            for split_col in origin_col..origin_col + col_span {
                if split_row == origin_row && split_col == origin_col {
                    continue;
                }
                if let Some(cell) = self
                    .rows
                    .get_mut(split_row)
                    .and_then(|row| row.cells.get_mut(split_col))
                {
                    cell.merge = TableCellMerge::Unmerged;
                }
            }
        }
        Ok(true)
    }

    pub fn visible_cells(&self) -> impl Iterator<Item = (usize, usize, &TableCellPayload)> {
        self.rows.iter().enumerate().flat_map(|(row_index, row)| {
            row.cells
                .iter()
                .enumerate()
                .filter(|(_, cell)| !matches!(cell.merge, TableCellMerge::Covered { .. }))
                .map(move |(col_index, cell)| (row_index, col_index, cell))
        })
    }

    pub fn cell_origin(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        let cell = self.rows.get(row)?.cells.get(col)?;
        match cell.merge {
            TableCellMerge::Unmerged | TableCellMerge::Origin { .. } => Some((row, col)),
            TableCellMerge::Covered {
                origin_row,
                origin_col,
            } => self
                .rows
                .get(origin_row)
                .and_then(|row| row.cells.get(origin_col))
                .map(|_| (origin_row, origin_col)),
        }
    }

    fn ensure_range(&self, range: TableRange) -> Result<(), String> {
        if range.start_row > range.end_row || range.start_col > range.end_col {
            return Err("invalid empty table range".to_owned());
        }
        if range.end_row >= self.row_count() || range.end_col >= self.column_count() {
            return Err(format!(
                "table range out of bounds rows={} cols={} range={range:?}",
                self.row_count(),
                self.column_count()
            ));
        }
        Ok(())
    }

    fn ensure_unmerged_range(&self, range: TableRange) -> Result<(), String> {
        for row in range.start_row..=range.end_row {
            for col in range.start_col..=range.end_col {
                let cell = self
                    .rows
                    .get(row)
                    .and_then(|row| row.cells.get(col))
                    .ok_or_else(|| format!("missing table cell {row}:{col}"))?;
                if !matches!(cell.merge, TableCellMerge::Unmerged) {
                    return Err(format!(
                        "cannot merge over existing merged cell {row}:{col}"
                    ));
                }
            }
        }
        Ok(())
    }

    fn normalize_merge_spans(&mut self) {
        let row_count = self.row_count();
        let col_count = self.column_count();
        for row in 0..row_count {
            for col in 0..col_count {
                let merge = self.rows[row].cells[col].merge;
                match merge {
                    TableCellMerge::Origin { row_span, col_span } => {
                        if row_span == 0
                            || col_span == 0
                            || row + row_span > row_count
                            || col + col_span > col_count
                        {
                            self.rows[row].cells[col].merge = TableCellMerge::Unmerged;
                        }
                    }
                    TableCellMerge::Covered {
                        origin_row,
                        origin_col,
                    } => {
                        let origin_valid = self
                            .rows
                            .get(origin_row)
                            .and_then(|row| row.cells.get(origin_col))
                            .is_some_and(|origin| {
                                matches!(
                                    origin.merge,
                                    TableCellMerge::Origin { row_span, col_span }
                                        if row >= origin_row
                                            && col >= origin_col
                                            && row < origin_row + row_span
                                            && col < origin_col + col_span
                                )
                            });
                        if !origin_valid {
                            self.rows[row].cells[col].merge = TableCellMerge::Unmerged;
                        }
                    }
                    TableCellMerge::Unmerged => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
