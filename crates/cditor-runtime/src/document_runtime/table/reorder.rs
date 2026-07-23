use super::*;

impl DocumentRuntime {
    pub(crate) fn move_table_row(
        &mut self,
        block_id: BlockId,
        from: usize,
        to: usize,
    ) -> Result<bool, String> {
        let mut preview = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .clone();
        if !preview.move_row(from, to)? {
            return Ok(false);
        }
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::DragDrop,
            TableEditOperation::MoveRows {
                block_id,
                from,
                to,
                count: 1,
            },
            TableEditOperation::MoveRows {
                block_id,
                from: to,
                to: from,
                count: 1,
            },
        )?;
        Ok(true)
    }

    pub(crate) fn move_table_column(
        &mut self,
        block_id: BlockId,
        from: usize,
        to: usize,
    ) -> Result<bool, String> {
        let mut preview = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .clone();
        if !preview.move_column(from, to)? {
            return Ok(false);
        }
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::DragDrop,
            TableEditOperation::MoveColumns {
                block_id,
                from,
                to,
                count: 1,
            },
            TableEditOperation::MoveColumns {
                block_id,
                from: to,
                to: from,
                count: 1,
            },
        )?;
        Ok(true)
    }

    pub(in crate::document_runtime) fn remap_focused_table_cell_after_table_operations(
        &mut self,
        operations: &[EditOperation],
    ) {
        for operation in operations {
            match operation {
                EditOperation::Table(TableEditOperation::MoveRows {
                    block_id,
                    from,
                    to,
                    count: 1,
                }) => {
                    let _ = self.remap_focused_table_cell_after_row_move(*block_id, *from, *to);
                }
                EditOperation::Table(TableEditOperation::MoveColumns {
                    block_id,
                    from,
                    to,
                    count: 1,
                }) => {
                    let _ = self.remap_focused_table_cell_after_column_move(*block_id, *from, *to);
                }
                EditOperation::Table(TableEditOperation::InsertRows {
                    block_id,
                    index,
                    rows,
                }) => {
                    for _ in rows {
                        let _ = self.remap_focused_table_cell_after_row_insert(*block_id, *index);
                    }
                }
                EditOperation::Table(TableEditOperation::DeleteRows {
                    block_id,
                    index,
                    rows,
                }) => {
                    for _ in rows {
                        let _ = self.remap_focused_table_cell_after_row_delete(*block_id, *index);
                    }
                }
                EditOperation::Table(TableEditOperation::InsertColumns {
                    block_id,
                    index,
                    columns,
                    ..
                }) => {
                    for _ in columns {
                        let _ =
                            self.remap_focused_table_cell_after_column_insert(*block_id, *index);
                    }
                }
                EditOperation::Table(TableEditOperation::DeleteColumns {
                    block_id,
                    index,
                    columns,
                    ..
                }) => {
                    for _ in columns {
                        let _ =
                            self.remap_focused_table_cell_after_column_delete(*block_id, *index);
                    }
                }
                EditOperation::Table(TableEditOperation::MergeCells {
                    block_id, range, ..
                }) => {
                    if let Some(focused) = self
                        .selection
                        .focused_table_cell
                        .filter(|focused| focused.block_id == *block_id)
                        && focused.row >= range.start_row
                        && focused.row <= range.end_row
                        && focused.col >= range.start_col
                        && focused.col <= range.end_col
                    {
                        let _ = self.set_focused_table_cell_after_structure_edit(
                            *block_id,
                            range.start_row,
                            range.start_col,
                            0,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn remap_focused_table_cell_after_row_move(
        &mut self,
        block_id: BlockId,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        let Some(focused) = self
            .selection
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
            .selection
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
