use super::*;

impl DocumentRuntime {
    pub(crate) fn merge_table_cells(
        &mut self,
        block_id: BlockId,
        range: TableRange,
    ) -> Result<bool, String> {
        let runtime = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
        let before = runtime.table().clone();
        let mut preview = runtime.clone();
        if !preview.merge_cells(range)? {
            return Ok(false);
        }
        let after = preview.table().clone();
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::ExplicitCommand,
            TableEditOperation::MergeCells {
                block_id,
                range,
                before: before.clone(),
                after: after.clone(),
            },
            TableEditOperation::SplitCell {
                block_id,
                row: range.start_row,
                col: range.start_col,
                before: after,
                after: before,
            },
        )?;
        Ok(true)
    }

    pub(crate) fn split_table_cell(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Result<bool, String> {
        let runtime = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
        let before = runtime.table().clone();
        let mut preview = runtime.clone();
        if !preview.split_cell(row, col)? {
            return Ok(false);
        }
        let after = preview.table().clone();
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::ExplicitCommand,
            TableEditOperation::SplitCell {
                block_id,
                row,
                col,
                before: before.clone(),
                after: after.clone(),
            },
            TableEditOperation::MergeCells {
                block_id,
                range: merged_range_at(&before, row, col)?,
                before: after,
                after: before,
            },
        )?;
        Ok(true)
    }

    pub(crate) fn set_table_cell_align(
        &mut self,
        block_id: BlockId,
        range: TableRange,
        align: TableCellAlign,
    ) -> Result<bool, String> {
        let table = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .table();
        if range.end_row >= table.row_count() || range.end_col >= table.column_count() {
            return Err(format!("align range {range:?} out of bounds"));
        }
        let mut forward = Vec::new();
        let mut inverse = Vec::new();
        for row in range.start_row..=range.end_row {
            for col in range.start_col..=range.end_col {
                let old_align = table.rows[row].cells[col].align;
                if old_align == align {
                    continue;
                }
                let cell_range = TableRange::normalized(row, col, row, col);
                forward.push(TableEditOperation::SetCellAlign {
                    block_id,
                    range: cell_range,
                    old_aligns: vec![vec![old_align]],
                    new_align: align,
                });
                inverse.push(TableEditOperation::SetCellAlign {
                    block_id,
                    range: cell_range,
                    old_aligns: vec![vec![align]],
                    new_align: old_align,
                });
            }
        }
        if forward.is_empty() {
            return Ok(false);
        }
        inverse.reverse();
        self.apply_local_table_operations(block_id, EditTransactionKind::Format, forward, inverse)?;
        Ok(true)
    }

    pub(crate) fn set_table_cell_background_color(
        &mut self,
        block_id: BlockId,
        range: TableRange,
        background_color: Option<String>,
    ) -> Result<bool, String> {
        let (kind, mut preview) = self.table_payload_preview(block_id)?;
        if !preview.set_cell_background_color(range, background_color)? {
            return Ok(false);
        }
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::Format,
            kind,
            preview.payload(),
        )
    }

    pub(crate) fn set_table_header_rows(
        &mut self,
        block_id: BlockId,
        count: usize,
    ) -> Result<bool, String> {
        let (kind, mut preview) = self.table_payload_preview(block_id)?;
        if !preview.set_header_rows(count) {
            return Ok(false);
        }
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::Format,
            kind,
            preview.payload(),
        )
    }

    pub(crate) fn set_table_header_columns(
        &mut self,
        block_id: BlockId,
        count: usize,
    ) -> Result<bool, String> {
        let (kind, mut preview) = self.table_payload_preview(block_id)?;
        if !preview.set_header_columns(count) {
            return Ok(false);
        }
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::Format,
            kind,
            preview.payload(),
        )
    }

    pub(crate) fn insert_table_row(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<bool, String> {
        let mut preview = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .clone();
        if !preview.insert_row(index)? {
            return Ok(false);
        }
        let row = preview.table().rows[index].clone();
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::ExplicitCommand,
            TableEditOperation::InsertRows {
                block_id,
                index,
                rows: vec![row.clone()],
            },
            TableEditOperation::DeleteRows {
                block_id,
                index,
                rows: vec![row],
            },
        )?;
        Ok(true)
    }

    pub(crate) fn delete_table_row(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<bool, String> {
        let runtime = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
        let row = runtime
            .table()
            .rows
            .get(index)
            .cloned()
            .ok_or_else(|| format!("row {index} out of bounds"))?;
        let mut preview = runtime.clone();
        if !preview.delete_row(index)? {
            return Ok(false);
        }
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::ExplicitCommand,
            TableEditOperation::DeleteRows {
                block_id,
                index,
                rows: vec![row.clone()],
            },
            TableEditOperation::InsertRows {
                block_id,
                index,
                rows: vec![row],
            },
        )?;
        Ok(true)
    }

    pub(crate) fn duplicate_table_row(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<bool, String> {
        let mut preview = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .clone();
        if !preview.duplicate_row(index)? {
            return Ok(false);
        }
        let insert_at = index.saturating_add(1);
        let row = preview.table().rows[insert_at].clone();
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::ExplicitCommand,
            TableEditOperation::InsertRows {
                block_id,
                index: insert_at,
                rows: vec![row.clone()],
            },
            TableEditOperation::DeleteRows {
                block_id,
                index: insert_at,
                rows: vec![row],
            },
        )?;
        Ok(true)
    }

    pub(crate) fn insert_table_column(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<bool, String> {
        let mut preview = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .clone();
        if !preview.insert_column(index)? {
            return Ok(false);
        }
        let (columns, cells_by_row) = table_column_snapshot(preview.table(), index)?;
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::ExplicitCommand,
            TableEditOperation::InsertColumns {
                block_id,
                index,
                columns: columns.clone(),
                cells_by_row: cells_by_row.clone(),
            },
            TableEditOperation::DeleteColumns {
                block_id,
                index,
                columns,
                cells_by_row,
            },
        )?;
        Ok(true)
    }

    pub(crate) fn delete_table_column(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<bool, String> {
        let runtime = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
        let (columns, cells_by_row) = table_column_snapshot(runtime.table(), index)?;
        let mut preview = runtime.clone();
        if !preview.delete_column(index)? {
            return Ok(false);
        }
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::ExplicitCommand,
            TableEditOperation::DeleteColumns {
                block_id,
                index,
                columns: columns.clone(),
                cells_by_row: cells_by_row.clone(),
            },
            TableEditOperation::InsertColumns {
                block_id,
                index,
                columns,
                cells_by_row,
            },
        )?;
        Ok(true)
    }

    pub(crate) fn duplicate_table_column(
        &mut self,
        block_id: BlockId,
        index: usize,
    ) -> Result<bool, String> {
        let mut preview = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .clone();
        if !preview.duplicate_column(index)? {
            return Ok(false);
        }
        let insert_at = index.saturating_add(1);
        let (columns, cells_by_row) = table_column_snapshot(preview.table(), insert_at)?;
        self.apply_local_table_operation(
            block_id,
            EditTransactionKind::ExplicitCommand,
            TableEditOperation::InsertColumns {
                block_id,
                index: insert_at,
                columns: columns.clone(),
                cells_by_row: cells_by_row.clone(),
            },
            TableEditOperation::DeleteColumns {
                block_id,
                index: insert_at,
                columns,
                cells_by_row,
            },
        )?;
        Ok(true)
    }

    pub(in crate::document_runtime) fn remap_focused_table_cell_after_row_insert(
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

    pub(in crate::document_runtime) fn remap_focused_table_cell_after_row_delete(
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

    pub(in crate::document_runtime) fn remap_focused_table_cell_after_column_insert(
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

    pub(in crate::document_runtime) fn remap_focused_table_cell_after_column_delete(
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

    fn table_payload_preview(
        &self,
        block_id: BlockId,
    ) -> Result<(RichBlockKind, TableRuntime), String> {
        let kind = self
            .payload_window
            .get(block_id)
            .map(|record| record.kind.clone())
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let preview = self
            .table_runtime(block_id)
            .cloned()
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
        Ok((kind, preview))
    }
}

fn merged_range_at(
    table: &cditor_core::rich_text::TablePayload,
    row: usize,
    col: usize,
) -> Result<TableRange, String> {
    let (origin_row, origin_col) = table
        .cell_origin(row, col)
        .ok_or_else(|| format!("missing table cell ({row}, {col})"))?;
    let merge = table.rows[origin_row].cells[origin_col].merge;
    let TableCellMerge::Origin { row_span, col_span } = merge else {
        return Err(format!("table cell ({row}, {col}) is not merged"));
    };
    Ok(TableRange::normalized(
        origin_row,
        origin_col,
        origin_row + row_span.saturating_sub(1),
        origin_col + col_span.saturating_sub(1),
    ))
}

fn table_column_snapshot(
    table: &cditor_core::rich_text::TablePayload,
    index: usize,
) -> Result<
    (
        Vec<cditor_core::rich_text::TableColumnPayload>,
        Vec<Vec<cditor_core::rich_text::TableCellPayload>>,
    ),
    String,
> {
    let mut table = table.clone();
    table.normalize();
    let column = table
        .columns
        .get(index)
        .cloned()
        .ok_or_else(|| format!("column {index} out of bounds"))?;
    let cells_by_row = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .get(index)
                .cloned()
                .map(|cell| vec![cell])
                .ok_or_else(|| format!("column {index} missing from a table row"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((vec![column], cells_by_row))
}
