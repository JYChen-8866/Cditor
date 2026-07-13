use super::*;

impl DocumentRuntime {
    pub fn merge_table_cells(
        &mut self,
        block_id: BlockId,
        range: TableRange,
    ) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.merge_cells(range)?
        };
        if changed {
            self.commit_table_runtime_payload(block_id)?;
            if let Some(focused) = self.focused_table_cell
                && focused.block_id == block_id
            {
                self.focused_table_cell = Some(FocusedTableCell::collapsed(
                    block_id,
                    range.start_row,
                    range.start_col,
                    0,
                ));
            }
        }
        Ok(changed)
    }

    pub fn split_table_cell(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.split_cell(row, col)?
        };
        if changed {
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn set_table_cell_align(
        &mut self,
        block_id: BlockId,
        range: TableRange,
        align: TableCellAlign,
    ) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.set_cell_align(range, align)?
        };
        if changed {
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn set_table_cell_background_color(
        &mut self,
        block_id: BlockId,
        range: TableRange,
        background_color: Option<String>,
    ) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.set_cell_background_color(range, background_color)?
        };
        if changed {
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn set_table_header_rows(
        &mut self,
        block_id: BlockId,
        count: usize,
    ) -> Result<bool, String> {
        let changed = self
            .table_runtime_mut(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .set_header_rows(count);
        if changed {
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn set_table_header_columns(
        &mut self,
        block_id: BlockId,
        count: usize,
    ) -> Result<bool, String> {
        let changed = self
            .table_runtime_mut(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .set_header_columns(count);
        if changed {
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn insert_table_row(&mut self, block_id: BlockId, index: usize) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.insert_row(index)?
        };
        if changed {
            self.remap_focused_table_cell_after_row_insert(block_id, index)?;
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn delete_table_row(&mut self, block_id: BlockId, index: usize) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.delete_row(index)?
        };
        if changed {
            self.remap_focused_table_cell_after_row_delete(block_id, index)?;
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn duplicate_table_row(&mut self, block_id: BlockId, index: usize) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.duplicate_row(index)?
        };
        if changed {
            self.remap_focused_table_cell_after_row_insert(block_id, index.saturating_add(1))?;
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn insert_table_column(&mut self, block_id: BlockId, index: usize) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.insert_column(index)?
        };
        if changed {
            self.remap_focused_table_cell_after_column_insert(block_id, index)?;
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn delete_table_column(&mut self, block_id: BlockId, index: usize) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.delete_column(index)?
        };
        if changed {
            self.remap_focused_table_cell_after_column_delete(block_id, index)?;
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn duplicate_table_column(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.duplicate_column(index)?
        };
        if changed {
            self.remap_focused_table_cell_after_column_insert(block_id, index.saturating_add(1))?;
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    fn remap_focused_table_cell_after_row_insert(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<(), String> {
        let Some(focused) = self
            .focused_table_cell
            .filter(|focused| focused.block_id == block_id)
        else {
            return Ok(());
        };
        let row = if focused.row >= index {
            focused.row.saturating_add(1)
        } else {
            focused.row
        };
        self.set_focused_table_cell_after_structure_edit(block_id, row, focused.col, focused.offset)
    }

    fn remap_focused_table_cell_after_row_delete(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<(), String> {
        let Some(focused) = self
            .focused_table_cell
            .filter(|focused| focused.block_id == block_id)
        else {
            return Ok(());
        };
        let Some(table) = self.table_runtime(block_id).map(|runtime| runtime.table()) else {
            self.focused_table_cell = None;
            return Ok(());
        };
        let row_count = table.row_count();
        let col_count = table.column_count();
        if row_count == 0 || col_count == 0 {
            self.focused_table_cell = None;
            return Ok(());
        }
        let row = if focused.row > index {
            focused.row.saturating_sub(1)
        } else {
            focused.row.min(row_count - 1)
        };
        let col = focused.col.min(col_count - 1);
        self.set_focused_table_cell_after_structure_edit(block_id, row, col, focused.offset)
    }

    fn remap_focused_table_cell_after_column_insert(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<(), String> {
        let Some(focused) = self
            .focused_table_cell
            .filter(|focused| focused.block_id == block_id)
        else {
            return Ok(());
        };
        let col = if focused.col >= index {
            focused.col.saturating_add(1)
        } else {
            focused.col
        };
        self.set_focused_table_cell_after_structure_edit(block_id, focused.row, col, focused.offset)
    }

    fn remap_focused_table_cell_after_column_delete(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<(), String> {
        let Some(focused) = self
            .focused_table_cell
            .filter(|focused| focused.block_id == block_id)
        else {
            return Ok(());
        };
        let Some(table) = self.table_runtime(block_id).map(|runtime| runtime.table()) else {
            self.focused_table_cell = None;
            return Ok(());
        };
        let row_count = table.row_count();
        let col_count = table.column_count();
        if row_count == 0 || col_count == 0 {
            self.focused_table_cell = None;
            return Ok(());
        }
        let row = focused.row.min(row_count - 1);
        let col = if focused.col > index {
            focused.col.saturating_sub(1)
        } else {
            focused.col.min(col_count - 1)
        };
        self.set_focused_table_cell_after_structure_edit(block_id, row, col, focused.offset)
    }

    pub(in crate::document_runtime) fn set_focused_table_cell_after_structure_edit(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        offset: usize,
    ) -> Result<(), String> {
        let Some(text) = self.table_cell_plain_text(block_id, row, col) else {
            self.focused_table_cell = None;
            return Ok(());
        };
        let offset = normalized_grapheme_offset(&text, offset);
        self.focused_table_cell = Some(FocusedTableCell::collapsed(block_id, row, col, offset));
        Ok(())
    }
}
