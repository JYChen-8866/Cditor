use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableClipboardSnapshot {
    pub range: TableRange,
    pub table: cditor_core::rich_text::TablePayload,
    pub plain_text: String,
    pub markdown: String,
}

impl DocumentRuntime {
    pub fn clear_table_range(
        &mut self,
        block_id: BlockId,
        range: TableRange,
    ) -> Result<bool, String> {
        let range = self
            .table_range_selection_range(block_id, range)
            .ok_or_else(|| "invalid table clipboard range".to_owned())?;
        let changed = {
            let runtime = self
                .table_runtime_mut(block_id)
                .ok_or_else(|| format!("missing table runtime for block {block_id}"))?;
            let mut changed = false;
            for row in range.start_row..=range.end_row {
                for col in range.start_col..=range.end_col {
                    if runtime
                        .cell_plain_text(row, col)
                        .is_some_and(|text| !text.is_empty())
                    {
                        runtime
                            .set_cell_plain_text(row, col, String::new())
                            .ok_or_else(|| format!("missing table cell {row}:{col}"))?;
                        changed = true;
                    }
                }
            }
            changed
        };
        if changed {
            self.commit_table_runtime_payload(block_id)?;
        }
        Ok(changed)
    }

    pub fn paste_delimited_table_text_at_focused_cell(
        &mut self,
        text: &str,
    ) -> Result<bool, String> {
        let Some(snapshot) = table_clipboard_snapshot_from_delimited_text(text) else {
            return Ok(false);
        };
        self.paste_table_clipboard_at_focused_cell(&snapshot)
    }

    pub fn paste_table_clipboard_at_focused_cell(
        &mut self,
        snapshot: &TableClipboardSnapshot,
    ) -> Result<bool, String> {
        let Some(focused) = self.focused_table_cell else {
            return Ok(false);
        };
        let changed = {
            let runtime = self
                .table_runtime_mut(focused.block_id)
                .ok_or_else(|| format!("missing table runtime for block {}", focused.block_id))?;
            runtime.paste_table_at(focused.row, focused.col, &snapshot.table)?
        };
        if changed {
            self.focused_table_cell = Some(FocusedTableCell::collapsed(
                focused.block_id,
                focused.row,
                focused.col,
                0,
            ));
            self.commit_table_runtime_payload(focused.block_id)?;
        }
        Ok(changed)
    }

    pub fn table_clipboard_for_cell(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Option<TableClipboardSnapshot> {
        let range = self.table_cell_selection_range(block_id, row, col)?;
        self.table_clipboard_for_range(block_id, range)
    }

    pub fn table_clipboard_for_row(
        &self,
        block_id: BlockId,
        row: usize,
    ) -> Option<TableClipboardSnapshot> {
        let range = self.table_row_selection_range(block_id, row)?;
        self.table_clipboard_for_range(block_id, range)
    }

    pub fn table_clipboard_for_column(
        &self,
        block_id: BlockId,
        col: usize,
    ) -> Option<TableClipboardSnapshot> {
        let range = self.table_column_selection_range(block_id, col)?;
        self.table_clipboard_for_range(block_id, range)
    }

    pub fn table_clipboard_for_whole_table(
        &self,
        block_id: BlockId,
    ) -> Option<TableClipboardSnapshot> {
        let range = self.whole_table_selection_range(block_id)?;
        self.table_clipboard_for_range(block_id, range)
    }

    pub fn table_clipboard_for_range(
        &self,
        block_id: BlockId,
        range: TableRange,
    ) -> Option<TableClipboardSnapshot> {
        let range = self.table_range_selection_range(block_id, range)?;
        let source = self.table_runtime(block_id)?.table();
        let table = table_payload_for_clipboard_range(source, range)?;
        let plain_text = table.plain_text();
        let markdown = table_clipboard_markdown(&table)?;
        Some(TableClipboardSnapshot {
            range,
            table,
            plain_text,
            markdown,
        })
    }
}

fn table_clipboard_snapshot_from_delimited_text(text: &str) -> Option<TableClipboardSnapshot> {
    let table = table_payload_from_delimited_text(text)?;
    let range = TableRange::normalized(0, 0, table.row_count() - 1, table.column_count() - 1);
    let plain_text = table.plain_text();
    let markdown = table_clipboard_markdown(&table)?;
    Some(TableClipboardSnapshot {
        range,
        table,
        plain_text,
        markdown,
    })
}

fn table_payload_from_delimited_text(text: &str) -> Option<cditor_core::rich_text::TablePayload> {
    let text = text.trim_end_matches(['\r', '\n']);
    if text.is_empty() || !looks_like_delimited_table_text(text) {
        return None;
    }
    let rows = if text.contains('\t') {
        parse_tsv_rows(text)
    } else {
        parse_csv_rows(text)
    };
    let rows = rows
        .into_iter()
        .filter(|row| !row.is_empty())
        .map(|row| cditor_core::rich_text::TableRowPayload {
            cells: row
                .into_iter()
                .map(cditor_core::rich_text::TableCellPayload::plain)
                .collect(),
            height: TableTrackSize::Auto,
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return None;
    }
    let mut table = cditor_core::rich_text::TablePayload {
        rows,
        columns: Vec::new(),
        header_rows: 0,
        header_cols: 0,
        header_style: Default::default(),
    };
    table.normalize();
    Some(table)
}

fn looks_like_delimited_table_text(text: &str) -> bool {
    text.contains('\t') || text.contains(',') || text.contains('\n')
}

fn parse_tsv_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .map(|line| line.trim_end_matches('\r'))
        .map(|line| line.split('\t').map(str::to_owned).collect())
        .collect()
}

fn parse_csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => {
                in_quotes = !in_quotes;
            }
            ',' if !in_quotes => {
                row.push(std::mem::take(&mut cell));
            }
            '\n' if !in_quotes => {
                row.push(cell.trim_end_matches('\r').to_owned());
                cell.clear();
                rows.push(std::mem::take(&mut row));
            }
            _ => cell.push(ch),
        }
    }
    row.push(cell.trim_end_matches('\r').to_owned());
    rows.push(row);
    rows
}

fn table_payload_for_clipboard_range(
    source: &cditor_core::rich_text::TablePayload,
    range: TableRange,
) -> Option<cditor_core::rich_text::TablePayload> {
    let rows = source
        .rows
        .get(range.start_row..=range.end_row)?
        .iter()
        .enumerate()
        .map(|(local_row, row)| cditor_core::rich_text::TableRowPayload {
            cells: row
                .cells
                .get(range.start_col..=range.end_col)
                .unwrap_or(&[])
                .iter()
                .enumerate()
                .map(|(local_col, cell)| {
                    remap_clipboard_cell_merge(cell.clone(), range, local_row, local_col)
                })
                .collect(),
            height: row.height,
        })
        .collect::<Vec<_>>();
    let columns = source
        .columns
        .get(range.start_col..=range.end_col)
        .unwrap_or(&[])
        .to_vec();
    let mut table = cditor_core::rich_text::TablePayload {
        rows,
        columns,
        header_rows: source
            .header_rows
            .saturating_sub(range.start_row)
            .min(range.row_count()),
        header_cols: source
            .header_cols
            .saturating_sub(range.start_col)
            .min(range.col_count()),
        header_style: source.header_style.clone(),
    };
    table.normalize();
    Some(table)
}

fn remap_clipboard_cell_merge(
    mut cell: cditor_core::rich_text::TableCellPayload,
    range: TableRange,
    local_row: usize,
    local_col: usize,
) -> cditor_core::rich_text::TableCellPayload {
    match cell.merge {
        TableCellMerge::Origin { row_span, col_span }
            if range_contains_span(
                range,
                range.start_row + local_row,
                range.start_col + local_col,
                row_span,
                col_span,
            ) =>
        {
            cell.merge = TableCellMerge::Origin { row_span, col_span };
        }
        TableCellMerge::Covered {
            origin_row,
            origin_col,
        } if origin_row >= range.start_row
            && origin_row <= range.end_row
            && origin_col >= range.start_col
            && origin_col <= range.end_col =>
        {
            cell.merge = TableCellMerge::Covered {
                origin_row: origin_row - range.start_row,
                origin_col: origin_col - range.start_col,
            };
        }
        _ => {
            cell.merge = TableCellMerge::Unmerged;
        }
    }
    cell
}

fn range_contains_span(
    range: TableRange,
    origin_row: usize,
    origin_col: usize,
    row_span: usize,
    col_span: usize,
) -> bool {
    origin_row >= range.start_row
        && origin_col >= range.start_col
        && origin_row + row_span.saturating_sub(1) <= range.end_row
        && origin_col + col_span.saturating_sub(1) <= range.end_col
}

fn table_clipboard_markdown(table: &cditor_core::rich_text::TablePayload) -> Option<String> {
    let first = table.rows.first()?;
    let columns = first.cells.len();
    if columns == 0 {
        return None;
    }
    let mut lines = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let cells = row
            .cells
            .iter()
            .map(|cell| {
                let text = cditor_core::rich_text::plain_text_from_spans(&cell.spans);
                text.replace('|', "\\|").replace('\n', "<br>")
            })
            .collect::<Vec<_>>();
        lines.push(format!("| {} |", cells.join(" | ")));
        if row_index == 0 {
            lines.push(format!("| {} |", vec!["---"; columns].join(" | ")));
        }
    }
    Some(lines.join("\n"))
}
