use super::*;

impl DocumentRuntime {
    pub fn active_composition(&self) -> Option<&CompositionState> {
        if !self.input_session_is_current() {
            return None;
        }
        self.editing.as_ref()?.composition.as_ref()
    }

    pub fn composition_preview_text(&self) -> Option<String> {
        let composition = self.active_composition()?;
        let text = self.composition_base_text(composition.block_id)?;
        Some(composition_preview_for_text(&text, composition))
    }

    pub fn focused_text_for_platform_input(&self) -> Option<(BlockId, String)> {
        let target = self.input_session_target()?;
        let block_id = target.block_id();
        if self
            .active_composition()
            .is_some_and(|composition| composition.block_id == block_id)
        {
            return self.composition_preview_text().map(|text| (block_id, text));
        }
        match target {
            InputTarget::BlockText { block_id } => {
                if self.block_is_table(block_id) {
                    None
                } else {
                    self.text_models
                        .get(&block_id)
                        .map(|model| (block_id, model.text().to_owned()))
                }
            }
            InputTarget::TableCell { block_id, row, col } => self
                .table_cell_plain_text(block_id, row, col)
                .map(|text| (block_id, text)),
        }
    }

    pub fn active_composition_marked_range(&self) -> Option<std::ops::Range<usize>> {
        let composition = self.active_composition()?;
        let text = self.composition_base_text(composition.block_id)?;
        let range = safe_char_range(
            &text,
            composition.range_start as usize..composition.range_end as usize,
        );
        Some(range.start..range.start + composition.preview_text.len())
    }

    pub fn active_composition_selected_range(&self) -> Option<std::ops::Range<usize>> {
        let composition = self.active_composition()?;
        let start = composition.selected_range_start? as usize;
        let end = composition.selected_range_end? as usize;
        let preview_text = self.composition_preview_text()?;
        Some(safe_char_range(&preview_text, start..end))
    }

    pub fn begin_or_update_composition(
        &mut self,
        block_id: BlockId,
        range: Range<usize>,
        preview_text: impl Into<String>,
    ) -> Result<(), String> {
        self.begin_or_update_composition_with_selection(block_id, range, preview_text, None)
    }

    pub fn begin_or_update_composition_with_selection(
        &mut self,
        block_id: BlockId,
        range: Range<usize>,
        preview_text: impl Into<String>,
        selected_range: Option<Range<usize>>,
    ) -> Result<(), String> {
        if self.focused_block_id() != Some(block_id) {
            self.focus_block(block_id);
        }
        if self.block_is_table(block_id)
            && !self
                .focused_table_cell
                .is_some_and(|cell| cell.block_id == block_id)
        {
            self.cancel_composition();
            return Ok(());
        }
        let base_text = self
            .composition_base_text(block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let requested_range = range.clone();
        let range = safe_char_range(&base_text, range);
        let preview_text = preview_text.into();
        let marked_range = range.start..range.start + preview_text.len();
        let selected_range = selected_range
            .map(|selected| safe_char_range(&preview_text, selected))
            .map(|selected| marked_range.start + selected.start..marked_range.start + selected.end);
        let caret_offset = selected_range
            .as_ref()
            .map(|selected| selected.end)
            .unwrap_or(marked_range.end);
        trace_input(
            "begin_or_update_composition",
            format_args!(
                "block={block_id} requested_range={requested_range:?} clamped_range={range:?} preview_len={} selected_range={selected_range:?} caret_offset={caret_offset} text_len={}",
                preview_text.len(),
                base_text.len()
            ),
        );
        self.document_selection = None;
        self.focused_text_selection = None;
        let editing = self.editing.as_mut().expect("editing session exists");
        if let Some(focused) = self
            .focused_table_cell
            .filter(|cell| cell.block_id == block_id)
        {
            editing.set_input_target(InputTarget::TableCell {
                block_id,
                row: focused.row,
                col: focused.col,
            });
        } else {
            editing.set_input_target(InputTarget::BlockText { block_id });
        }
        editing
            .update_composition(CompositionState {
                block_id,
                range_start: range.start as u64,
                range_end: range.end as u64,
                preview_text,
                selected_range_start: selected_range.as_ref().map(|range| range.start as u64),
                selected_range_end: selected_range.as_ref().map(|range| range.end as u64),
            })
            .map_err(|error| format!("{error:?}"))?;
        let next_marked_range = marked_range;
        if let Some(selected_range) = selected_range {
            editing.set_selected_range(selected_range, false);
            editing.set_marked_range(next_marked_range);
        } else {
            editing.set_collapsed_selection(caret_offset);
            editing.set_marked_range(next_marked_range);
        }
        if let Some(cell) = self.focused_table_cell.as_mut()
            && cell.block_id == block_id
        {
            let selection = editing.selected_range.clone();
            *cell = cell
                .with_selected_range(selection, editing.selection_reversed)
                .with_marked_range(editing.marked_range.clone());
        }
        Ok(())
    }

    pub fn cancel_composition(&mut self) {
        if let Some(editing) = self.editing.as_mut() {
            editing.clear_composition();
        }
        if let Some(cell) = self.focused_table_cell.as_mut() {
            *cell = cell.with_marked_range(None);
        }
    }

    pub fn commit_composition(&mut self) -> Result<bool, String> {
        let Some(composition) = self
            .editing
            .as_ref()
            .and_then(|editing| editing.composition.clone())
        else {
            return Ok(false);
        };
        let block_id = composition.block_id;
        if self
            .focused_table_cell
            .filter(|cell| cell.block_id == block_id)
            .is_some()
        {
            let changed = self.replace_text_in_focused_table_cell_range(
                Some(composition.range_start as usize..composition.range_end as usize),
                &composition.preview_text,
            )?;
            if changed {
                self.cancel_composition();
            }
            return Ok(changed);
        }
        self.push_undo_snapshot(block_id)?;
        let model = self
            .text_models
            .get_mut(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let range = safe_char_range(
            model.text(),
            composition.range_start as usize..composition.range_end as usize,
        );
        let replaced_range = range.clone();
        let preview_text = composition.preview_text.clone();
        let inserted = model
            .replace_range(range, &preview_text)
            .map_err(|error| format!("{error:?}"))?;
        let editing = self.editing.as_mut().expect("editing session exists");
        editing.clear_composition();
        editing.content_version += 1;
        editing.set_input_target(InputTarget::BlockText { block_id });
        editing.set_collapsed_selection(inserted.end);
        sync_payload_from_model_after_replace(
            &mut self.payload_window,
            block_id,
            editing.content_version,
            model,
            replaced_range,
            &preview_text,
        );
        Ok(true)
    }

    pub(super) fn payload_with_composition_preview(
        &self,
        block_id: BlockId,
        mut payload: BlockPayloadRecord,
    ) -> BlockPayloadRecord {
        if self
            .active_composition()
            .is_some_and(|composition| composition.block_id == block_id)
            && let Some(preview_text) = self.composition_preview_text()
        {
            if let Some(focused) = self
                .focused_table_cell
                .filter(|cell| cell.block_id == block_id)
                && let Some(table_payload) = self.table_cell_payload_with_text(
                    focused.block_id,
                    focused.row,
                    focused.col,
                    preview_text.clone(),
                )
            {
                payload.payload = table_payload;
            } else if matches!(payload.payload, BlockPayload::Table(_)) {
                return payload;
            } else {
                payload.payload = text_payload_for_existing(&payload.payload, &preview_text);
            }
        }
        payload
    }

    fn composition_base_text(&self, block_id: BlockId) -> Option<String> {
        match self.input_session_target()? {
            InputTarget::TableCell {
                block_id: target_block_id,
                row,
                col,
            } if target_block_id == block_id => self.table_cell_plain_text(block_id, row, col),
            InputTarget::BlockText {
                block_id: target_block_id,
            } if target_block_id == block_id => self
                .text_models
                .get(&block_id)
                .map(|model| model.text().to_owned()),
            _ => None,
        }
    }

    fn block_is_table(&self, block_id: BlockId) -> bool {
        self.payload_window
            .get(block_id)
            .is_some_and(|payload| matches!(payload.kind, RichBlockKind::Table))
    }
}

fn composition_preview_for_text(text: &str, composition: &CompositionState) -> String {
    let range = safe_char_range(
        text,
        composition.range_start as usize..composition.range_end as usize,
    );
    let mut preview = String::with_capacity(
        text.len() - (range.end - range.start) + composition.preview_text.len(),
    );
    preview.push_str(&text[..range.start]);
    preview.push_str(&composition.preview_text);
    preview.push_str(&text[range.end..]);
    preview
}
