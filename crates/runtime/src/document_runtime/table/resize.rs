use super::*;

impl DocumentRuntime {
    pub fn set_table_row_height(
        &mut self,
        block_id: BlockId,
        row: usize,
        height: TableTrackSize,
    ) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.set_row_height(row, height)?
        };
        if changed {
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn set_table_column_width(
        &mut self,
        block_id: BlockId,
        col: usize,
        width: TableTrackSize,
    ) -> Result<bool, String> {
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            runtime.set_column_width(col, width)?
        };
        if changed {
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }
}
