use super::*;

impl DocumentRuntime {
    pub fn move_caret_left(&mut self, extend_selection: bool) -> Result<bool, String> {
        self.move_caret_horizontally(false, extend_selection)
    }

    pub fn move_caret_right(&mut self, extend_selection: bool) -> Result<bool, String> {
        self.move_caret_horizontally(true, extend_selection)
    }

    pub fn move_caret_up(&mut self, extend_selection: bool) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        if extend_selection {
            self.extend_selection_to_adjacent_visible_block(block_id, -1, true)
        } else {
            self.focus_adjacent_visible_block(block_id, -1, true)
        }
    }

    pub fn move_caret_down(&mut self, extend_selection: bool) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        if extend_selection {
            self.extend_selection_to_adjacent_visible_block(block_id, 1, false)
        } else {
            self.focus_adjacent_visible_block(block_id, 1, false)
        }
    }

    pub fn move_focused_caret_to_offset(
        &mut self,
        block_id: BlockId,
        offset: usize,
        extend_selection: bool,
    ) -> Result<bool, String> {
        if self.focused_block_id() != Some(block_id) {
            return Ok(false);
        }
        let model = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let previous = self
            .editing
            .as_ref()
            .map(|editing| editing.caret_anchor.text_offset as usize)
            .unwrap_or_else(|| model.len())
            .min(model.len());
        let previous = normalized_grapheme_offset(model.text(), previous);
        let offset = normalized_grapheme_offset(model.text(), offset);
        if extend_selection {
            let anchor = self
                .focused_text_selection
                .map(|selection| selection.anchor)
                .unwrap_or(previous);
            self.focused_text_selection = Some(FocusedTextSelection {
                anchor,
                focus: offset,
            });
            self.document_selection = Some(DocumentSelection {
                anchor: TextPosition::downstream(block_id, anchor),
                focus: TextPosition::downstream(block_id, offset),
            });
            if self
                .focused_text_selection
                .is_some_and(FocusedTextSelection::is_collapsed)
            {
                self.focused_text_selection = None;
                self.document_selection = None;
            }
        } else {
            self.focused_text_selection = None;
            self.document_selection = None;
        }
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(InputTarget::BlockText { block_id });
            if extend_selection {
                if let Some(selection) = self.focused_text_selection {
                    editing.set_selected_range(selection.range(), offset < selection.anchor);
                } else {
                    editing.set_collapsed_selection(offset);
                }
            } else {
                editing.set_collapsed_selection(offset);
            }
        }
        Ok(previous != offset || extend_selection)
    }

    fn move_caret_horizontally(
        &mut self,
        forward: bool,
        extend_selection: bool,
    ) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let model = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let caret = self
            .editing
            .as_ref()
            .map(|editing| editing.caret_anchor.text_offset as usize)
            .unwrap_or_else(|| model.len())
            .min(model.len());
        let caret = normalized_grapheme_offset(model.text(), caret);
        let next = if forward {
            next_grapheme_boundary(model.text(), caret)
        } else {
            previous_grapheme_boundary(model.text(), caret)
        };
        if next == caret {
            return if extend_selection {
                self.extend_selection_to_adjacent_visible_block(
                    block_id,
                    if forward { 1 } else { -1 },
                    !forward,
                )
            } else {
                self.focus_adjacent_visible_block(block_id, if forward { 1 } else { -1 }, !forward)
            };
        }
        if extend_selection {
            let anchor = self
                .focused_text_selection
                .map(|selection| selection.anchor)
                .unwrap_or(caret);
            self.focused_text_selection = Some(FocusedTextSelection {
                anchor,
                focus: next,
            });
            self.document_selection = Some(DocumentSelection {
                anchor: TextPosition::downstream(block_id, anchor),
                focus: TextPosition::downstream(block_id, next),
            });
            if self
                .focused_text_selection
                .is_some_and(FocusedTextSelection::is_collapsed)
            {
                self.focused_text_selection = None;
                self.document_selection = None;
            }
        } else {
            self.focused_text_selection = None;
            self.document_selection = None;
        }
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(InputTarget::BlockText { block_id });
            if extend_selection {
                if let Some(selection) = self.focused_text_selection {
                    editing.set_selected_range(selection.range(), next < selection.anchor);
                } else {
                    editing.set_collapsed_selection(next);
                }
            } else {
                editing.set_collapsed_selection(next);
            }
        }
        Ok(caret != next)
    }

    pub fn focus_adjacent_visible_block(
        &mut self,
        block_id: BlockId,
        direction: i32,
        focus_end: bool,
    ) -> Result<bool, String> {
        let Some(target_id) = self.adjacent_visible_block_id(block_id, direction) else {
            return Ok(false);
        };
        let target_len = self
            .text_models
            .get(&target_id)
            .map(PieceTableTextModel::len)
            .unwrap_or(0);
        self.focus_block_at_offset(target_id, if focus_end { target_len } else { 0 })?;
        Ok(true)
    }

    fn extend_selection_to_adjacent_visible_block(
        &mut self,
        block_id: BlockId,
        direction: i32,
        target_end: bool,
    ) -> Result<bool, String> {
        let Some(target_id) = self.adjacent_visible_block_id(block_id, direction) else {
            return Ok(false);
        };
        let caret = self.caret_offset_for_block(block_id).unwrap_or_else(|| {
            self.text_models
                .get(&block_id)
                .map(PieceTableTextModel::len)
                .unwrap_or(0)
        });
        let anchor = self
            .document_selection
            .map(|selection| selection.anchor)
            .unwrap_or_else(|| TextPosition::downstream(block_id, caret));
        let target_offset = if target_end {
            self.text_models
                .get(&target_id)
                .map(PieceTableTextModel::len)
                .unwrap_or(0)
        } else {
            0
        };
        self.focus_block_at_offset(target_id, target_offset)?;
        self.document_selection = Some(DocumentSelection {
            anchor,
            focus: TextPosition::downstream(target_id, target_offset),
        });
        self.focused_text_selection = None;
        Ok(true)
    }

    pub(super) fn adjacent_visible_block_id(
        &self,
        block_id: BlockId,
        direction: i32,
    ) -> Option<BlockId> {
        let index = self.visible_index.visible_index_of(block_id)?;
        let target = if direction < 0 {
            index.checked_sub(1)?
        } else {
            index.checked_add(1)?
        };
        self.visible_index.id_at_visible_index(target)
    }
}
