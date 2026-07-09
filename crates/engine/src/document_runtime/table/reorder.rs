use super::*;

impl DocumentRuntime {
    pub fn move_table_row(
        &mut self,
        block_id: BlockId,
        from: usize,
        to: usize,
    ) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.move_row(from, to)?
        };
        if changed {
            self.remap_focused_table_cell_after_row_move(block_id, from, to)?;
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn move_table_column(
        &mut self,
        block_id: BlockId,
        from: usize,
        to: usize,
    ) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.move_column(from, to)?
        };
        if changed {
            self.remap_focused_table_cell_after_column_move(block_id, from, to)?;
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    fn remap_focused_table_cell_after_row_move(
        &mut self,
        block_id: BlockId,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        let Some(focused) = self
            .focused_table_cell
            .filter(|focused| focused.block_id == block_id)
        else {
            return Ok(());
        };
        let row = remap_moved_axis_index(focused.row, from, to);
        self.set_focused_table_cell_after_structure_edit(block_id, row, focused.col, focused.offset)
    }

    fn remap_focused_table_cell_after_column_move(
        &mut self,
        block_id: BlockId,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        let Some(focused) = self
            .focused_table_cell
            .filter(|focused| focused.block_id == block_id)
        else {
            return Ok(());
        };
        let col = remap_moved_axis_index(focused.col, from, to);
        self.set_focused_table_cell_after_structure_edit(block_id, focused.row, col, focused.offset)
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
