use super::*;

impl DocumentRuntime {
    pub fn set_table_row_height(
        &mut self,
        block_id: BlockId,
        row: usize,
        height: TableTrackSize,
    ) -> Result<bool, String> {
        let old_height = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .table()
            .rows
            .get(row)
            .ok_or_else(|| format!("row {row} out of bounds"))?
            .height;
        if old_height == height {
            return Ok(false);
        }
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::DragDrop,
            TableEditOperation::ResizeRow {
                block_id,
                row,
                old_height,
                new_height: height,
            },
            TableEditOperation::ResizeRow {
                block_id,
                row,
                old_height: height,
                new_height: old_height,
            },
        )?;
        Ok(true)
    }

    pub fn set_table_column_width(
        &mut self,
        block_id: BlockId,
        col: usize,
        width: TableTrackSize,
    ) -> Result<bool, String> {
        let mut table = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .table()
            .clone();
        table.normalize();
        let old_width = table
            .columns
            .get(col)
            .ok_or_else(|| format!("column {col} out of bounds"))?
            .width;
        if old_width == width {
            return Ok(false);
        }
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::DragDrop,
            TableEditOperation::ResizeColumn {
                block_id,
                column: col,
                old_width,
                new_width: width,
            },
            TableEditOperation::ResizeColumn {
                block_id,
                column: col,
                old_width: width,
                new_width: old_width,
            },
        )?;
        Ok(true)
    }
}
