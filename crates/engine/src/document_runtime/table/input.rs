use super::*;

impl DocumentRuntime {
    pub(in crate::document_runtime) fn replace_text_in_focused_table_cell_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
    ) -> Result<bool, String> {
        let Some(focused) = self.focused_table_cell else {
            return Ok(false);
        };
        trace_table(
            "replace_table_cell.begin",
            format_args!(
                "block={} row={} col={} range={range:?} insert_len={}",
                focused.block_id,
                focused.row,
                focused.col,
                text.len()
            ),
        );
        let current = self
            .table_cell_plain_text(focused.block_id, focused.row, focused.col)
            .ok_or_else(|| {
                format!(
                    "missing focused table cell {}:{} in block {}",
                    focused.row, focused.col, focused.block_id
                )
            })?;
        let range = range
            .map(|range| safe_char_range(&current, range))
            .or_else(|| {
                self.active_composition()
                    .filter(|composition| composition.block_id == focused.block_id)
                    .map(|composition| {
                        safe_char_range(
                            &current,
                            composition.range_start as usize..composition.range_end as usize,
                        )
                    })
            })
            .or_else(|| {
                self.input_session_selected_range()
                    .map(|range| safe_char_range(&current, range))
            })
            .unwrap_or_else(|| {
                let offset = previous_char_boundary(&current, focused.offset.min(current.len()));
                offset..offset
            });
        if range.is_empty() && text.is_empty() {
            return Ok(false);
        }

        self.push_undo_snapshot(focused.block_id)?;
        let mut next = current;
        next.replace_range(range.clone(), text);
        let next_offset = range.start + text.len();
        self.cancel_composition();
        self.document_selection = None;
        self.focused_text_selection = None;
        self.selected_block_ids.clear();

        let next_table_payload = {
            let runtime = self
                .table_runtime_mut(focused.block_id)
                .ok_or_else(|| format!("missing table runtime for block {}", focused.block_id))?;
            runtime
                .set_cell_plain_text(focused.row, focused.col, next)
                .ok_or_else(|| {
                    format!(
                        "missing table cell {}:{} in block {}",
                        focused.row, focused.col, focused.block_id
                    )
                })?;
            runtime.payload()
        };
        let next_content_version = {
            let payload = self
                .payload_window
                .payloads
                .get_mut(&focused.block_id)
                .ok_or_else(|| format!("missing payload for block {}", focused.block_id))?;
            payload.payload = next_table_payload;
            payload.content_version = payload.content_version.saturating_add(1);
            payload.content_version
        };

        self.text_models.remove(&focused.block_id);
        if let Some(editing) = self.editing.as_mut()
            && editing.block_id == focused.block_id
        {
            editing.content_version = next_content_version;
            editing.set_input_target(InputTarget::TableCell {
                block_id: focused.block_id,
                row: focused.row,
                col: focused.col,
            });
            editing.set_collapsed_selection(next_offset);
        }
        self.focused_table_cell = Some(
            focused
                .with_selected_range(next_offset..next_offset, false)
                .with_marked_range(None),
        );
        trace_table(
            "replace_table_cell.end",
            format_args!(
                "block={} row={} col={} next_offset={next_offset} content_version={next_content_version}",
                focused.block_id, focused.row, focused.col
            ),
        );
        let _ = self.refresh_table_block_height(focused.block_id)?;
        Ok(true)
    }

    pub(in crate::document_runtime) fn delete_backward_in_focused_table_cell(
        &mut self,
    ) -> Result<bool, String> {
        let Some(focused) = self.focused_table_cell else {
            return Ok(false);
        };
        let Some(text) = self.table_cell_plain_text(focused.block_id, focused.row, focused.col)
        else {
            return Ok(false);
        };
        let caret = previous_char_boundary(&text, focused.offset.min(text.len()));
        if caret == 0 {
            return Ok(false);
        }
        let previous = previous_grapheme_boundary(&text, caret);
        self.replace_text_in_focused_table_cell_range(Some(previous..caret), "")
    }

    pub(in crate::document_runtime) fn delete_forward_in_focused_table_cell(
        &mut self,
    ) -> Result<bool, String> {
        let Some(focused) = self.focused_table_cell else {
            return Ok(false);
        };
        let Some(text) = self.table_cell_plain_text(focused.block_id, focused.row, focused.col)
        else {
            return Ok(false);
        };
        let caret = previous_char_boundary(&text, focused.offset.min(text.len()));
        let next = next_grapheme_boundary(&text, caret);
        if caret == next {
            return Ok(false);
        }
        self.replace_text_in_focused_table_cell_range(Some(caret..next), "")
    }

    pub(in crate::document_runtime) fn table_cell_plain_text(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
    ) -> Option<String> {
        self.table_runtime(block_id)?.cell_plain_text(row, col)
    }

    pub(in crate::document_runtime) fn table_cell_payload_with_text(
        &self,
        block_id: BlockId,
        row: usize,
        col: usize,
        text: String,
    ) -> Option<BlockPayload> {
        let mut runtime = self.table_runtime(block_id)?.clone();
        runtime.set_cell_plain_text(row, col, text)?;
        Some(runtime.payload())
    }
}
