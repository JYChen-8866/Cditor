use super::*;

impl DocumentRuntime {
    pub fn has_pending_composition(&self) -> bool {
        self.editing
            .as_ref()
            .is_some_and(|editing| editing.composition.is_some())
    }

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
                    self.document
                        .text_models
                        .get(&block_id)
                        .map(|model| (block_id, model.text().to_owned()))
                }
            }
            InputTarget::TableCell { block_id, row, col } => self
                .table_cell_plain_text(block_id, row, col)
                .map(|text| (block_id, text)),
            InputTarget::ImageCaption { block_id } | InputTarget::CollectionTitle { block_id } => {
                self.base_text_for_target(target)
                    .map(|text| (block_id, text))
            }
            // Complex blocks and block chrome don't have text composition
            InputTarget::ComplexBlock { .. } | InputTarget::BlockChrome { .. } => None,
        }
    }

    pub fn active_composition_marked_range(&self) -> Option<std::ops::Range<usize>> {
        let composition = self.active_composition()?;
        let text = self.composition_base_text(composition.block_id)?;
        let range = normalized_grapheme_range(
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
        Some(normalized_grapheme_range(&preview_text, start..end))
    }

    #[cfg(test)]
    pub(crate) fn begin_or_update_composition(
        &mut self,
        block_id: BlockId,
        range: Range<usize>,
        preview_text: impl Into<String>,
    ) -> Result<(), String> {
        self.begin_or_update_composition_with_selection(block_id, range, preview_text, None)
    }

    pub(crate) fn begin_or_update_composition_with_selection(
        &mut self,
        block_id: BlockId,
        range: Range<usize>,
        preview_text: impl Into<String>,
        selected_range: Option<Range<usize>>,
    ) -> Result<(), String> {
        if self.active_composition().is_none() {
            self.break_typing_coalescing();
        }
        if self.focused_block_id() != Some(block_id) {
            self.focus_block(block_id);
        }
        if self.block_is_table(block_id)
            && self
                .selection
                .focused_table_cell
                .is_none_or(|cell| cell.block_id != block_id)
        {
            self.cancel_composition();
            return Ok(());
        }
        let base_text = self
            .composition_base_text(block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let requested_range = range.clone();
        let range = normalized_grapheme_range(&base_text, range);
        let preview_text = preview_text.into();
        let marked_range = range.start..range.start + preview_text.len();
        let selected_range = selected_range
            .map(|selected| normalized_grapheme_range(&preview_text, selected))
            .map(|selected| marked_range.start + selected.start..marked_range.start + selected.end);
        let caret_offset = selected_range
            .as_ref()
            .map(|selected| selected.end)
            .unwrap_or(marked_range.end);
        let base_selection = self
            .active_composition()
            .filter(|composition| composition.block_id == block_id)
            .map(|composition| composition.base_selection)
            .unwrap_or_else(|| self.capture_composition_base_selection(block_id));
        trace_input(
            "begin_or_update_composition",
            format_args!(
                "block={block_id} requested_range={requested_range:?} clamped_range={range:?} preview_len={} selected_range={selected_range:?} caret_offset={caret_offset} text_len={}",
                preview_text.len(),
                base_text.len()
            ),
        );
        self.selection.document_selection = None;
        self.selection.focused_text_selection = None;
        let previous_target = self
            .editing
            .as_ref()
            .map(|editing| editing.input_target)
            .expect("editing session exists");
        let editing = self.editing.as_mut().expect("editing session exists");
        if let Some(focused) = self
            .selection
            .focused_table_cell
            .filter(|cell| cell.block_id == block_id)
        {
            editing.set_input_target(InputTarget::TableCell {
                block_id,
                row: focused.row,
                col: focused.col,
            });
        } else if previous_target.block_id() == block_id && previous_target.accepts_text_caret() {
            editing.set_input_target(previous_target);
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
                base_selection,
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
        if let Some(cell) = self.selection.focused_table_cell.as_mut()
            && cell.block_id == block_id
        {
            let selection = editing.selected_range.clone();
            *cell = cell
                .with_selected_range(selection, editing.selection_reversed)
                .with_marked_range(editing.marked_range.clone());
        }
        Ok(())
    }

    pub(crate) fn cancel_composition(&mut self) {
        let restore = self.editing.as_ref().and_then(|editing| {
            editing
                .composition
                .as_ref()
                .map(|composition| (editing.input_target, composition.base_selection))
        });
        if let Some(editing) = self.editing.as_mut() {
            editing.clear_composition();
        }
        let Some((target, base_selection)) = restore else {
            if let Some(cell) = self.selection.focused_table_cell.as_mut() {
                *cell = cell.with_marked_range(None);
            }
            return;
        };
        match target {
            InputTarget::BlockText { block_id } => {
                self.restore_block_composition_selection(block_id, base_selection);
            }
            InputTarget::TableCell { block_id, row, col } => {
                self.restore_table_composition_selection(block_id, row, col, base_selection);
            }
            InputTarget::ImageCaption { block_id } => {
                self.restore_auxiliary_composition_selection(
                    InputTarget::ImageCaption { block_id },
                    base_selection,
                );
            }
            InputTarget::CollectionTitle { block_id } => {
                self.restore_auxiliary_composition_selection(
                    InputTarget::CollectionTitle { block_id },
                    base_selection,
                );
            }
            InputTarget::ComplexBlock { .. } | InputTarget::BlockChrome { .. } => {}
        }
    }

    pub(crate) fn commit_composition(&mut self) -> Result<bool, String> {
        let Some(composition) = self
            .editing
            .as_ref()
            .and_then(|editing| editing.composition.clone())
        else {
            return Ok(false);
        };
        self.validate_composition_commit_ready(&composition)?;
        let changed = self.replace_text_from_composition(
            composition.range_start as usize..composition.range_end as usize,
            &composition.preview_text,
        )?;
        if changed {
            self.cancel_composition();
        }
        Ok(changed)
    }

    pub(super) fn validate_composition_commit_ready(
        &self,
        composition: &CompositionState,
    ) -> Result<(), String> {
        let editing = self
            .editing
            .as_ref()
            .ok_or_else(|| "active composition has no editing session".to_owned())?;
        if editing.block_id != composition.block_id {
            return Err(format!(
                "composition block {} does not match editing block {}",
                composition.block_id, editing.block_id
            ));
        }
        let target = editing.input_target;
        if !matches!(
            target,
            InputTarget::BlockText { block_id }
                | InputTarget::TableCell { block_id, .. }
                | InputTarget::ImageCaption { block_id }
                | InputTarget::CollectionTitle { block_id }
                if block_id == composition.block_id
        ) {
            return Err(format!(
                "composition target {target:?} does not accept block {} text",
                composition.block_id
            ));
        }
        let payload_version = self
            .block_content_version(composition.block_id)
            .ok_or_else(|| {
                format!(
                    "missing payload for composition block {}",
                    composition.block_id
                )
            })?;
        if payload_version != editing.content_version {
            return Err(format!(
                "stale composition content version: session={} payload={payload_version}",
                editing.content_version
            ));
        }
        let text = self
            .base_text_for_target(target)
            .ok_or_else(|| format!("missing text for composition target {target:?}"))?;
        let range = composition.range_start as usize..composition.range_end as usize;
        if range.start > range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            return Err(format!(
                "invalid composition range {range:?} for text length {}",
                text.len()
            ));
        }
        Ok(())
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
                .selection
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
            } else if matches!(
                self.input_session_target(),
                Some(InputTarget::ImageCaption { .. } | InputTarget::CollectionTitle { .. })
            ) || matches!(payload.payload, BlockPayload::Table(_))
            {
                return payload;
            } else {
                payload.payload = text_payload_for_existing(&payload.payload, &preview_text);
            }
        }
        payload
    }

    fn composition_base_text(&self, block_id: BlockId) -> Option<String> {
        let target = self.input_session_target()?;
        (target.block_id() == block_id)
            .then(|| self.base_text_for_target(target))
            .flatten()
    }

    fn block_is_table(&self, block_id: BlockId) -> bool {
        self.document
            .payload_window
            .get(block_id)
            .is_some_and(|payload| matches!(payload.kind, RichBlockKind::Table))
    }

    fn capture_composition_base_selection(&self, block_id: BlockId) -> CompositionBaseSelection {
        let Some(editing) = self.editing.as_ref() else {
            return CompositionBaseSelection::default();
        };
        if let Some(cell) = self
            .selection
            .focused_table_cell
            .filter(|cell| cell.block_id == block_id)
        {
            let selected = cell.selected_range();
            return CompositionBaseSelection {
                selected_range_start: selected.start,
                selected_range_end: selected.end,
                selection_reversed: cell.selection_reversed,
                affinity: cell.affinity,
                document_selection: None,
            };
        }
        let affinity = self
            .selection
            .document_selection
            .filter(|selection| selection.focus.block_id == block_id)
            .map(|selection| selection.focus.affinity)
            .or_else(|| {
                self.selection
                    .visual_caret_position
                    .filter(|caret| caret.position.block_id == block_id)
                    .map(|caret| caret.position.affinity)
            })
            .unwrap_or(TextAffinity::Downstream);
        CompositionBaseSelection {
            selected_range_start: editing.selected_range.start,
            selected_range_end: editing.selected_range.end,
            selection_reversed: editing.selection_reversed,
            affinity,
            document_selection: self.selection.document_selection,
        }
    }

    fn restore_block_composition_selection(
        &mut self,
        block_id: BlockId,
        base: CompositionBaseSelection,
    ) {
        let selected = base.selected_range();
        let focus_offset = if base.selection_reversed {
            selected.start
        } else {
            selected.end
        };
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(InputTarget::BlockText { block_id });
            if selected.is_empty() {
                editing.set_collapsed_selection(focus_offset);
            } else {
                editing.set_selected_range(selected.clone(), base.selection_reversed);
            }
        }
        self.selection.document_selection = base.document_selection;
        self.selection.focused_text_selection = base
            .document_selection
            .filter(|selection| {
                selection.anchor.block_id == block_id
                    && selection.focus.block_id == block_id
                    && !selection.is_caret()
            })
            .map(|selection| FocusedTextSelection {
                anchor: selection.anchor.offset,
                focus: selection.focus.offset,
            })
            .or_else(|| {
                (!selected.is_empty()).then_some(FocusedTextSelection {
                    anchor: if base.selection_reversed {
                        selected.end
                    } else {
                        selected.start
                    },
                    focus: focus_offset,
                })
            });
        let position = base
            .document_selection
            .map(|selection| selection.focus)
            .unwrap_or(TextPosition {
                block_id,
                offset: focus_offset,
                affinity: base.affinity,
            });
        let content_version = self
            .editing
            .as_ref()
            .map(|editing| editing.content_version)
            .unwrap_or_default();
        self.selection.visual_caret_position = Some(VisualCaretPosition {
            position,
            content_version,
        });
    }

    fn restore_table_composition_selection(
        &mut self,
        block_id: BlockId,
        row: usize,
        col: usize,
        base: CompositionBaseSelection,
    ) {
        let selected = base.selected_range();
        if let Some(cell) = self.selection.focused_table_cell.as_mut()
            && cell.block_id == block_id
            && cell.row == row
            && cell.col == col
        {
            *cell = cell
                .with_selected_range(selected.clone(), base.selection_reversed)
                .with_affinity(base.affinity)
                .with_marked_range(None);
        }
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(InputTarget::TableCell { block_id, row, col });
            if selected.is_empty() {
                editing.set_collapsed_selection(selected.start);
            } else {
                editing.set_selected_range(selected, base.selection_reversed);
            }
        }
        self.selection.document_selection = None;
        self.selection.focused_text_selection = None;
        self.selection.visual_caret_position = None;
    }

    fn restore_auxiliary_composition_selection(
        &mut self,
        target: InputTarget,
        base: CompositionBaseSelection,
    ) {
        let selected = base.selected_range();
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(target);
            if selected.is_empty() {
                editing.set_collapsed_selection(selected.start);
            } else {
                editing.set_selected_range(selected, base.selection_reversed);
            }
        }
        self.selection.document_selection = None;
        self.selection.focused_text_selection = None;
        self.selection.visual_caret_position = None;
    }
}

fn composition_preview_for_text(text: &str, composition: &CompositionState) -> String {
    let range = normalized_grapheme_range(
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
