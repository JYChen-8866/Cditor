use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::document_runtime) struct TableRuntime {
    table: cditor_core::rich_text::TablePayload,
    revision: u64,
    dirty: bool,
}

impl TableRuntime {
    pub(in crate::document_runtime) fn from_payload(payload: BlockPayload) -> Self {
        let payload = ensure_table_payload_for_kind(&RichBlockKind::Table, payload);
        let BlockPayload::Table(table) = payload else {
            unreachable!("table invariant always returns table payload for table kind");
        };
        let mut table = table;
        table.normalize();
        Self {
            table,
            revision: 0,
            dirty: false,
        }
    }

    pub(in crate::document_runtime) fn payload(&self) -> BlockPayload {
        BlockPayload::Table(self.table.clone())
    }

    pub(in crate::document_runtime) fn table(&self) -> &cditor_core::rich_text::TablePayload {
        &self.table
    }

    pub(in crate::document_runtime) fn cell_plain_text(
        &self,
        row: usize,
        col: usize,
    ) -> Option<String> {
        self.table.cell_plain_text(row, col)
    }

    pub(in crate::document_runtime) fn set_cell_plain_text(
        &mut self,
        row: usize,
        col: usize,
        text: String,
    ) -> Option<u64> {
        self.table.set_cell_plain_text(row, col, text)?;
        self.revision = self.revision.saturating_add(1);
        self.dirty = true;
        Some(self.revision)
    }

    pub(in crate::document_runtime) fn replace_cell_spans(
        &mut self,
        row: usize,
        col: usize,
        range: Range<usize>,
        inserted: &[InlineSpan],
    ) -> Result<bool, String> {
        let (row, col) = self
            .table
            .cell_origin(row, col)
            .ok_or_else(|| format!("missing table cell {row}:{col}"))?;
        let cell = self
            .table
            .rows
            .get_mut(row)
            .and_then(|row| row.cells.get_mut(col))
            .ok_or_else(|| format!("missing table cell {row}:{col}"))?;
        let text = plain_text_from_spans(&cell.spans);
        let range = safe_char_range(&text, range);
        let next = replace_rich_text_spans_with_spans(&cell.spans, range, inserted);
        if next == cell.spans {
            return Ok(false);
        }
        cell.spans = next;
        self.revision = self.revision.saturating_add(1);
        self.dirty = true;
        Ok(true)
    }

    pub(in crate::document_runtime) fn merge_cells(
        &mut self,
        range: TableRange,
    ) -> Result<bool, String> {
        let changed = self.table.merge_cells(range)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn split_cell(
        &mut self,
        row: usize,
        col: usize,
    ) -> Result<bool, String> {
        let changed = self.table.split_cell(row, col)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn set_cell_align(
        &mut self,
        range: TableRange,
        align: TableCellAlign,
    ) -> Result<bool, String> {
        let changed = self.table.set_cell_align(range, align)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn set_cell_background_color(
        &mut self,
        range: TableRange,
        background_color: Option<String>,
    ) -> Result<bool, String> {
        let changed = self
            .table
            .set_cell_background_color(range, background_color)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn paste_table_at(
        &mut self,
        row: usize,
        col: usize,
        table: &cditor_core::rich_text::TablePayload,
    ) -> Result<bool, String> {
        let changed = self.table.paste_table_at(row, col, table)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn set_row_height(
        &mut self,
        row: usize,
        height: TableTrackSize,
    ) -> Result<bool, String> {
        let changed = self.table.set_row_height(row, height)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn set_column_width(
        &mut self,
        col: usize,
        width: TableTrackSize,
    ) -> Result<bool, String> {
        let changed = self.table.set_column_width(col, width)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn insert_row(&mut self, index: usize) -> Result<bool, String> {
        let changed = self.table.insert_row(index)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn delete_row(&mut self, index: usize) -> Result<bool, String> {
        let changed = self.table.delete_row(index)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn duplicate_row(
        &mut self,
        index: usize,
    ) -> Result<bool, String> {
        let changed = self.table.duplicate_row(index)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn move_row(
        &mut self,
        from: usize,
        to: usize,
    ) -> Result<bool, String> {
        let changed = self.table.move_row(from, to)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn insert_column(
        &mut self,
        index: usize,
    ) -> Result<bool, String> {
        let changed = self.table.insert_column(index)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn delete_column(
        &mut self,
        index: usize,
    ) -> Result<bool, String> {
        let changed = self.table.delete_column(index)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn duplicate_column(
        &mut self,
        index: usize,
    ) -> Result<bool, String> {
        let changed = self.table.duplicate_column(index)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }

    pub(in crate::document_runtime) fn move_column(
        &mut self,
        from: usize,
        to: usize,
    ) -> Result<bool, String> {
        let changed = self.table.move_column(from, to)?;
        if changed {
            self.revision = self.revision.saturating_add(1);
            self.dirty = true;
        }
        Ok(changed)
    }
}
