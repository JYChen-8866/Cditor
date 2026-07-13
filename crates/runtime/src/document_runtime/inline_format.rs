use super::*;

impl DocumentRuntime {
    pub fn toggle_inline_mark_on_selection(&mut self, mark: InlineMark) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(range) = self.focused_text_selection_range() else {
            return Ok(false);
        };
        let text = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();
        let range = safe_char_range(&text, range);
        if range.is_empty() {
            return Ok(false);
        }
        let (kind, current_spans) = self
            .payload_window
            .get(block_id)
            .and_then(|payload| match &payload.payload {
                BlockPayload::RichText { spans } => Some((payload.kind.clone(), spans.clone())),
                _ => None,
            })
            .ok_or_else(|| format!("block {block_id} does not support inline marks"))?;
        self.push_undo_snapshot(block_id)?;
        let spans = toggle_mark_for_range(&current_spans, range.clone(), mark);
        self.replace_block_kind_and_spans(block_id, kind, spans)?;
        self.focused_text_selection = Some(FocusedTextSelection {
            anchor: range.start,
            focus: range.end,
        });
        if let Some(editing) = self
            .editing
            .as_mut()
            .filter(|editing| editing.block_id == block_id)
        {
            editing.caret_anchor.text_offset = range.end as u64;
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_selected_range(range, false);
        }
        Ok(true)
    }

    pub fn set_inline_color_on_selection(
        &mut self,
        target: InlineColorTarget,
        color: Option<&str>,
    ) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(range) = self.focused_text_selection_range() else {
            return Ok(false);
        };
        let original_selection = self.focused_text_selection;
        let selection_reversed = self.input_session_selection_reversed();
        let changed = self.set_inline_color_for_range(block_id, range.clone(), target, color)?;
        if !changed {
            return Ok(false);
        }
        self.focused_text_selection = original_selection.or(Some(FocusedTextSelection {
            anchor: range.start,
            focus: range.end,
        }));
        if let Some(editing) = self
            .editing
            .as_mut()
            .filter(|editing| editing.block_id == block_id)
        {
            editing.caret_anchor.text_offset = if selection_reversed {
                range.start as u64
            } else {
                range.end as u64
            };
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_selected_range(range, selection_reversed);
        }
        Ok(true)
    }

    pub fn set_inline_color_for_range(
        &mut self,
        block_id: BlockId,
        range: std::ops::Range<usize>,
        target: InlineColorTarget,
        color: Option<&str>,
    ) -> Result<bool, String> {
        let text = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();
        let range = safe_char_range(&text, range);
        if range.is_empty() {
            return Ok(false);
        }
        let (kind, current_spans) = self
            .payload_window
            .get(block_id)
            .and_then(|payload| match &payload.payload {
                BlockPayload::RichText { spans } => Some((payload.kind.clone(), spans.clone())),
                _ => None,
            })
            .ok_or_else(|| format!("block {block_id} does not support inline colors"))?;
        let spans = set_color_mark_for_range(&current_spans, range, target, color);
        if spans == current_spans {
            return Ok(false);
        }
        let focused_selection = self.focused_text_selection;
        let editing_selection = self
            .editing
            .as_ref()
            .filter(|editing| editing.block_id == block_id)
            .map(|editing| {
                (
                    editing.selected_range.clone(),
                    editing.selection_reversed,
                    editing.caret_anchor.text_offset,
                )
            });
        self.push_undo_snapshot(block_id)?;
        self.replace_block_kind_and_spans(block_id, kind, spans)?;
        self.focused_text_selection = focused_selection;
        if let (Some(editing), Some((selected_range, selection_reversed, caret_offset))) = (
            self.editing
                .as_mut()
                .filter(|editing| editing.block_id == block_id),
            editing_selection,
        ) {
            editing.caret_anchor.text_offset = caret_offset;
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_selected_range(selected_range, selection_reversed);
        }
        Ok(true)
    }
}
