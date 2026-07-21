//! external transaction 的 payload 级操作（transaction_apply 的辅助）。
//!
//! 文本编辑支持 RichText 与 Code payload；全部偏移都要求 char boundary，
//! 违反即拒绝（保持 grapheme/scalar 安全约定）。表格操作携带 before 状态时
//! 逐项核对（OT 安全：过期事务拒绝而非覆盖）。

use cditor_core::edit::TableEditOperation;
use cditor_core::rich_text::splice_spans_at_range;

use super::*;

pub(super) fn insert_text_into_payload(
    record: &mut BlockPayloadRecord,
    offset: usize,
    text: &str,
) -> Result<(), String> {
    match &mut record.payload {
        BlockPayload::RichText { spans } => {
            let plain = cditor_core::rich_text::plain_text_from_spans(spans);
            ensure_char_boundary(&plain, offset)?;
            *spans = splice_spans_at_range(
                spans,
                offset..offset,
                vec![InlineSpan::plain(text.to_owned())],
            );
            Ok(())
        }
        BlockPayload::Code { text: code, .. } => {
            ensure_char_boundary(code, offset)?;
            code.insert_str(offset, text);
            Ok(())
        }
        other => Err(format!(
            "block {} payload {:?} does not accept text edits",
            record.block_id,
            std::mem::discriminant(other)
        )),
    }
}

pub(super) fn delete_text_from_payload(
    record: &mut BlockPayloadRecord,
    range: Range<usize>,
) -> Result<(), String> {
    if range.start > range.end {
        return Err(format!("inverted range {range:?}"));
    }
    match &mut record.payload {
        BlockPayload::RichText { spans } => {
            let plain = cditor_core::rich_text::plain_text_from_spans(spans);
            ensure_char_boundary(&plain, range.start)?;
            ensure_char_boundary(&plain, range.end)?;
            *spans = splice_spans_at_range(spans, range, Vec::new());
            Ok(())
        }
        BlockPayload::Code { text: code, .. } => {
            ensure_char_boundary(code, range.start)?;
            ensure_char_boundary(code, range.end)?;
            code.replace_range(range, "");
            Ok(())
        }
        other => Err(format!(
            "block {} payload {:?} does not accept text edits",
            record.block_id,
            std::mem::discriminant(other)
        )),
    }
}

/// 把 payload 文本在 offset 处一分为二，返回 (leading, trailing)。
pub(super) fn split_payload_text(
    record: &BlockPayloadRecord,
    offset: usize,
) -> Result<(BlockPayload, BlockPayload), String> {
    match &record.payload {
        BlockPayload::RichText { spans } => {
            let plain = cditor_core::rich_text::plain_text_from_spans(spans);
            ensure_char_boundary(&plain, offset)?;
            let leading = splice_spans_at_range(spans, offset..plain.len(), Vec::new());
            let trailing = splice_spans_at_range(spans, 0..offset, Vec::new());
            Ok((
                BlockPayload::RichText { spans: leading },
                BlockPayload::RichText { spans: trailing },
            ))
        }
        BlockPayload::Code { text, language } => {
            ensure_char_boundary(text, offset)?;
            Ok((
                BlockPayload::Code {
                    language: language.clone(),
                    text: text[..offset].to_owned(),
                },
                BlockPayload::Code {
                    language: language.clone(),
                    text: text[offset..].to_owned(),
                },
            ))
        }
        other => Err(format!(
            "block {} payload {:?} cannot be split",
            record.block_id,
            std::mem::discriminant(other)
        )),
    }
}

fn ensure_char_boundary(text: &str, offset: usize) -> Result<(), String> {
    if offset > text.len() {
        return Err(format!("offset {offset} beyond text length {}", text.len()));
    }
    if !text.is_char_boundary(offset) {
        return Err(format!("offset {offset} is not a char boundary"));
    }
    Ok(())
}

pub(super) fn apply_table_op(
    record: &mut BlockPayloadRecord,
    op: &TableEditOperation,
) -> Result<(), String> {
    let BlockPayload::Table(table) = &mut record.payload else {
        return Err(format!("block {} is not a table", record.block_id));
    };
    match op {
        TableEditOperation::SetCellText {
            row,
            col,
            old_spans,
            new_spans,
            ..
        } => {
            let cell = table
                .rows
                .get_mut(*row)
                .and_then(|row_payload| row_payload.cells.get_mut(*col))
                .ok_or_else(|| format!("cell ({row},{col}) out of bounds"))?;
            if cell.spans != *old_spans {
                return Err(format!(
                    "cell ({row},{col}) content changed since the transaction was built"
                ));
            }
            cell.spans = new_spans.clone();
        }
        TableEditOperation::InsertRows { index, rows, .. } => {
            if *index > table.rows.len() {
                return Err(format!("row index {index} out of bounds"));
            }
            table.rows.splice(*index..*index, rows.iter().cloned());
        }
        TableEditOperation::DeleteRows { index, rows, .. } => {
            let end = index
                .checked_add(rows.len())
                .filter(|end| *end <= table.rows.len())
                .ok_or_else(|| format!("row range {index}+{} out of bounds", rows.len()))?;
            if table.rows[*index..end] != *rows {
                return Err("deleted rows changed since the transaction was built".to_owned());
            }
            table.rows.drain(*index..end);
        }
        TableEditOperation::InsertColumns {
            index,
            columns,
            cells_by_row,
            ..
        } => {
            if *index > table.column_count() {
                return Err(format!("column index {index} out of bounds"));
            }
            if cells_by_row.len() != table.rows.len() {
                return Err(format!(
                    "cells_by_row has {} rows, table has {}",
                    cells_by_row.len(),
                    table.rows.len()
                ));
            }
            let insert_columns = columns.iter().cloned();
            let column_at = (*index).min(table.columns.len());
            table.columns.splice(column_at..column_at, insert_columns);
            for (row_payload, cells) in table.rows.iter_mut().zip(cells_by_row) {
                let at = (*index).min(row_payload.cells.len());
                row_payload.cells.splice(at..at, cells.iter().cloned());
            }
        }
        TableEditOperation::DeleteColumns {
            index,
            columns,
            cells_by_row,
            ..
        } => {
            let width = cells_by_row.first().map(Vec::len).unwrap_or(1).max(1);
            if index + width > table.column_count() {
                return Err(format!("column range {index}+{width} out of bounds"));
            }
            table.normalize();
            if columns.len() != width || table.columns[*index..index + width] != *columns {
                return Err("deleted columns changed since the transaction was built".to_owned());
            }
            if cells_by_row.len() != table.rows.len()
                || table
                    .rows
                    .iter()
                    .zip(cells_by_row)
                    .any(|(row, expected)| row.cells[*index..index + width] != expected[..])
            {
                return Err(
                    "deleted column cells changed since the transaction was built".to_owned(),
                );
            }
            if index + width <= table.columns.len() {
                table.columns.drain(*index..index + width);
            }
            for row_payload in &mut table.rows {
                let end = (index + width).min(row_payload.cells.len());
                let start = (*index).min(end);
                row_payload.cells.drain(start..end);
            }
        }
        TableEditOperation::ResizeRow {
            row,
            old_height,
            new_height,
            ..
        } => {
            let row_payload = table
                .rows
                .get_mut(*row)
                .ok_or_else(|| format!("row {row} out of bounds"))?;
            if row_payload.height != *old_height {
                return Err(format!(
                    "row {row} height changed since the transaction was built"
                ));
            }
            row_payload.height = *new_height;
        }
        TableEditOperation::ResizeColumn {
            column,
            old_width,
            new_width,
            ..
        } => {
            if *column >= table.column_count() {
                return Err(format!("column {column} out of bounds"));
            }
            table.normalize();
            let column_payload = table
                .columns
                .get_mut(*column)
                .ok_or_else(|| format!("column {column} missing after normalize"))?;
            if column_payload.width != *old_width {
                return Err(format!(
                    "column {column} width changed since the transaction was built"
                ));
            }
            column_payload.width = *new_width;
        }
        TableEditOperation::MoveRows {
            from, to, count, ..
        } => {
            if *count != 1 {
                return Err("multi-row move requires a range permutation operation".to_owned());
            }
            table.move_row(*from, *to)?;
        }
        TableEditOperation::MoveColumns {
            from, to, count, ..
        } => {
            if *count != 1 {
                return Err("multi-column move requires a range permutation operation".to_owned());
            }
            table.move_column(*from, *to)?;
        }
        TableEditOperation::MergeCells { before, after, .. }
        | TableEditOperation::SplitCell { before, after, .. } => {
            if table != before {
                return Err("table changed since the merge/split transaction was built".to_owned());
            }
            *table = after.clone();
        }
        TableEditOperation::SetCellAlign {
            range,
            old_aligns,
            new_align,
            ..
        } => {
            if range.end_row >= table.rows.len() || range.end_col >= table.column_count() {
                return Err(format!("align range {range:?} out of bounds"));
            }
            let expected_rows = range.end_row - range.start_row + 1;
            let expected_cols = range.end_col - range.start_col + 1;
            if old_aligns.len() != expected_rows
                || old_aligns.iter().any(|row| row.len() != expected_cols)
            {
                return Err("old align matrix does not match target range".to_owned());
            }
            for (row_index, row_payload) in table.rows[range.start_row..=range.end_row]
                .iter_mut()
                .enumerate()
            {
                let end_col = range.end_col.min(row_payload.cells.len().saturating_sub(1));
                for (column_index, cell) in row_payload.cells[range.start_col..=end_col]
                    .iter_mut()
                    .enumerate()
                {
                    if cell.align != old_aligns[row_index][column_index] {
                        return Err(
                            "cell alignment changed since the transaction was built".to_owned()
                        );
                    }
                    cell.align = *new_align;
                }
            }
        }
    }
    table.normalize();
    Ok(())
}
