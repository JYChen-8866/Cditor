use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichTextSelectionSnapshot {
    pub text: String,
    pub spans: Vec<InlineSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FocusedTextSelection {
    pub(super) anchor: usize,
    pub(super) focus: usize,
}

impl FocusedTextSelection {
    pub(super) fn range(self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }

    pub(super) fn is_collapsed(self) -> bool {
        self.anchor == self.focus
    }
}

impl DocumentRuntime {
    pub(super) fn input_session_is_current(&self) -> bool {
        let Some(editing) = self.editing.as_ref() else {
            return false;
        };
        self.block_content_version(editing.block_id)
            .is_some_and(|content_version| content_version == editing.content_version)
    }

    pub fn input_session_target(&self) -> Option<InputTarget> {
        self.input_session_is_current()
            .then(|| self.editing.as_ref().map(|editing| editing.input_target))
            .flatten()
    }

    pub fn input_session_selected_range(&self) -> Option<Range<usize>> {
        self.input_session_is_current()
            .then(|| {
                self.editing
                    .as_ref()
                    .map(|editing| editing.selected_range.clone())
            })
            .flatten()
    }

    pub fn input_session_selection_reversed(&self) -> bool {
        self.input_session_is_current()
            && self
                .editing
                .as_ref()
                .is_some_and(|editing| editing.selection_reversed)
    }

    pub fn input_session_marked_range(&self) -> Option<Range<usize>> {
        self.input_session_is_current()
            .then(|| {
                self.editing
                    .as_ref()
                    .and_then(|editing| editing.marked_range.clone())
            })
            .flatten()
    }

    pub fn focused_text_selection_range(&self) -> Option<Range<usize>> {
        self.focused_text_selection
            .map(FocusedTextSelection::range)
            .filter(|range| !range.is_empty())
            .or_else(|| self.focused_document_selection_range())
    }

    fn focused_document_selection_range(&self) -> Option<Range<usize>> {
        let block_id = self.focused_block_id()?;
        let selection = self.document_selection?;
        if selection.anchor.block_id != block_id || selection.focus.block_id != block_id {
            return None;
        }
        let range = if selection.anchor.offset <= selection.focus.offset {
            selection.anchor.offset..selection.focus.offset
        } else {
            selection.focus.offset..selection.anchor.offset
        };
        (!range.is_empty()).then_some(range)
    }

    pub fn set_document_text_selection(
        &mut self,
        anchor_block_id: BlockId,
        anchor_offset: usize,
        focus_block_id: BlockId,
        focus_offset: usize,
    ) -> Result<bool, String> {
        let anchor_offset = self.clamp_text_offset(anchor_block_id, anchor_offset)?;
        let focus_offset = self.clamp_text_offset(focus_block_id, focus_offset)?;
        trace_input(
            "set_document_text_selection.start",
            format_args!(
                "anchor={anchor_block_id}:{anchor_offset} focus={focus_block_id}:{focus_offset} previous_focus={:?}",
                self.focused_block_id()
            ),
        );
        if self.focused_block_id() != Some(focus_block_id) {
            self.focus_block(focus_block_id);
        }
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(InputTarget::BlockText {
                block_id: focus_block_id,
            });
        }
        self.selected_block_ids.clear();
        self.focused_table_cell = None;
        self.document_selection = Some(DocumentSelection {
            anchor: TextPosition::downstream(anchor_block_id, anchor_offset),
            focus: TextPosition::downstream(focus_block_id, focus_offset),
        });
        self.focused_text_selection = if anchor_block_id == focus_block_id {
            Some(FocusedTextSelection {
                anchor: anchor_offset,
                focus: focus_offset,
            })
            .filter(|selection| !selection.is_collapsed())
        } else {
            None
        };
        if let Some(editing) = self.editing.as_mut() {
            if anchor_block_id == focus_block_id {
                let range = anchor_offset.min(focus_offset)..anchor_offset.max(focus_offset);
                if range.is_empty() {
                    editing.set_collapsed_selection(focus_offset);
                } else {
                    editing.set_selected_range(range, focus_offset < anchor_offset);
                }
            } else {
                editing.set_collapsed_selection(focus_offset);
            }
        }
        if self
            .document_selection
            .is_some_and(|selection| selection.is_caret())
        {
            self.document_selection = None;
            self.focused_text_selection = None;
            if let Some(editing) = self.editing.as_mut() {
                editing.set_collapsed_selection(focus_offset);
            }
        }
        trace_input(
            "set_document_text_selection.end",
            format_args!(
                "focus={:?} caret={:?} focused_text_selection={:?} document_selection={:?}",
                self.focused_block_id(),
                self.caret_offset_for_block(focus_block_id),
                self.focused_text_selection,
                self.document_selection
            ),
        );
        Ok(true)
    }

    fn clamp_text_offset(&self, block_id: BlockId, offset: usize) -> Result<usize, String> {
        let model = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        Ok(previous_char_boundary(
            model.text(),
            offset.min(model.len()),
        ))
    }

    pub fn select_focused_text_all(&mut self) -> bool {
        let Some(block_id) = self.focused_block_id() else {
            return false;
        };
        let Some(model) = self.text_models.get(&block_id) else {
            return false;
        };
        let len = model.len();
        self.focused_table_cell = None;
        self.focused_text_selection = Some(FocusedTextSelection {
            anchor: 0,
            focus: len,
        });
        self.document_selection = Some(DocumentSelection {
            anchor: TextPosition::downstream(block_id, 0),
            focus: TextPosition::downstream(block_id, len),
        });
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_selected_range(0..len, false);
        }
        true
    }

    pub fn selected_focused_text(&self) -> Option<String> {
        if let Some(text) = self.selected_document_text() {
            return Some(text);
        }
        let block_id = self.focused_block_id()?;
        let model = self.text_models.get(&block_id)?;
        let range = self.focused_text_selection_range()?;
        model.text().get(range).map(ToOwned::to_owned)
    }

    pub fn selected_focused_rich_text(&self) -> Option<RichTextSelectionSnapshot> {
        if self.has_cross_block_text_selection() {
            return None;
        }
        let block_id = self.focused_block_id()?;
        let model = self.text_models.get(&block_id)?;
        let range = safe_char_range(model.text(), self.focused_text_selection_range()?);
        if range.is_empty() {
            return None;
        }
        let payload = self.payload_window.get(block_id)?;
        let BlockPayload::RichText { spans } = &payload.payload else {
            return None;
        };
        let spans = slice_rich_text_spans(spans, range);
        let text = spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>();
        (!text.is_empty()).then_some(RichTextSelectionSnapshot { text, spans })
    }

    pub fn has_cross_block_text_selection(&self) -> bool {
        self.document_selection.is_some_and(|selection| {
            !selection.is_caret() && selection.anchor.block_id != selection.focus.block_id
        })
    }

    pub fn selected_document_text(&self) -> Option<String> {
        let selection = self.document_selection?;
        let normalized = selection.normalize(&self.index).ok()?;
        if normalized.start.block_id == normalized.end.block_id {
            let model = self.text_models.get(&normalized.start.block_id)?;
            let range = normalized.start.offset..normalized.end.offset;
            return model.text().get(range).map(ToOwned::to_owned);
        }
        let start_index = self.index.index_of(normalized.start.block_id)?;
        let end_index = self.index.index_of(normalized.end.block_id)?;
        let mut parts = Vec::new();
        for index in start_index..=end_index {
            let block_id = self.index.block_ids[index];
            let model = self.text_models.get(&block_id)?;
            let text = model.text();
            let range = if block_id == normalized.start.block_id {
                normalized.start.offset..text.len()
            } else if block_id == normalized.end.block_id {
                0..normalized.end.offset
            } else {
                0..text.len()
            };
            parts.push(text.get(range)?.to_owned());
        }
        Some(parts.join("\n"))
    }

    pub fn select_all_visible_blocks(&mut self) -> bool {
        self.focused_table_cell = None;
        self.selected_block_ids = self
            .visible_index
            .visible_block_ids
            .iter()
            .copied()
            .collect();
        true
    }

    pub fn has_selected_blocks(&self) -> bool {
        !self.selected_block_ids.is_empty()
    }

    pub fn select_visible_block_range(&mut self, anchor: BlockId, focus: BlockId) -> bool {
        let Some(anchor_index) = self.visible_index.visible_index_of(anchor) else {
            return false;
        };
        let Some(focus_index) = self.visible_index.visible_index_of(focus) else {
            return false;
        };
        let start = anchor_index.min(focus_index);
        let end = anchor_index.max(focus_index);
        self.focused_table_cell = None;
        self.selected_block_ids.clear();
        for index in start..=end {
            if let Some(block_id) = self.visible_index.id_at_visible_index(index) {
                self.selected_block_ids.insert(block_id);
            }
        }
        self.editing = None;
        true
    }
}
