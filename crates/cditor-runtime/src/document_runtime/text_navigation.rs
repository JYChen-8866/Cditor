use super::*;

impl DocumentRuntime {
    pub(crate) fn move_focused_caret_by_word(
        &mut self,
        forward: bool,
        extend_selection: bool,
    ) -> Result<bool, String> {
        let Some(surface_id) = self.focused_text_surface_id() else {
            return Ok(false);
        };
        let snapshot = self
            .text_surface_snapshot(surface_id)
            .ok_or_else(|| format!("missing text surface {surface_id:?}"))?;
        let text = snapshot.plain_text();
        let caret = self
            .text_surface_caret_offset(surface_id)
            .unwrap_or(text.len());
        let target = word_boundary(&text, caret, forward);
        self.move_focused_text_surface_to_offset(
            surface_id,
            target,
            TextAffinity::Downstream,
            extend_selection,
        )
    }

    pub(crate) fn move_caret_to_document_boundary(
        &mut self,
        to_end: bool,
        extend_selection: bool,
    ) -> Result<bool, String> {
        let Some(target_id) = (if to_end {
            self.document.index.block_ids.last().copied()
        } else {
            self.document.index.block_ids.first().copied()
        }) else {
            return Ok(false);
        };
        let target_offset = if to_end {
            self.document
                .text_models
                .get(&target_id)
                .map(PieceTableTextModel::len)
                .or_else(|| {
                    self.document
                        .payload_window
                        .get(target_id)
                        .map(|payload| payload.plain_text().len())
                })
                .ok_or_else(|| format!("document boundary payload {target_id} is not hydrated"))?
        } else {
            0
        };
        let previous = self.focused_block_id().zip(
            self.focused_block_id()
                .and_then(|block_id| self.caret_offset_for_block(block_id)),
        );
        let anchor = if extend_selection {
            self.selection
                .document_selection
                .map(|selection| selection.anchor)
                .or_else(|| {
                    previous.map(|(block_id, offset)| TextPosition::downstream(block_id, offset))
                })
        } else {
            None
        };
        self.focus_block_at_offset(target_id, target_offset)?;
        if let Some(anchor) = anchor {
            self.selection.document_selection = Some(DocumentSelection {
                anchor,
                focus: TextPosition::downstream(target_id, target_offset),
            });
            self.selection.focused_text_selection = None;
        }
        Ok(previous != Some((target_id, target_offset)) || extend_selection)
    }

    /// Move the caret to the logical line boundary used by native Home/End.
    /// This works for rich-text blocks, code/quote soft lines, and table cells,
    /// and preserves Shift-selection semantics on every platform.
    pub(crate) fn move_focused_caret_to_line_boundary(
        &mut self,
        to_end: bool,
        extend_selection: bool,
    ) -> Result<bool, String> {
        let Some((block_id, text)) = self.focused_text_for_platform_input() else {
            return Ok(false);
        };
        let caret = self
            .selection
            .focused_table_cell
            .filter(|cell| cell.block_id == block_id)
            .map(|cell| cell.offset)
            .or_else(|| self.caret_offset_for_block(block_id))
            .unwrap_or(0);
        let caret = normalized_grapheme_offset(&text, caret);
        let target = logical_line_boundary(&text, caret, to_end);

        if let Some(focused) = self
            .selection
            .focused_table_cell
            .filter(|cell| cell.block_id == block_id)
        {
            let anchor =
                if extend_selection && focused.selected_range_start != focused.selected_range_end {
                    if focused.selection_reversed {
                        focused.selected_range_end
                    } else {
                        focused.selected_range_start
                    }
                } else {
                    caret
                };
            let (selection, reversed) = if extend_selection {
                (anchor.min(target)..anchor.max(target), target < anchor)
            } else {
                (target..target, false)
            };
            if let Some(cell) = self.selection.focused_table_cell.as_mut() {
                *cell = cell
                    .with_selected_range(selection.clone(), reversed)
                    .with_marked_range(None);
            }
            if let Some(editing) = self.editing.session.as_mut() {
                editing.set_input_target(InputTarget::TableCell {
                    block_id,
                    row: focused.row,
                    col: focused.col,
                });
                if extend_selection {
                    editing.set_selected_range(selection, reversed);
                } else {
                    editing.set_collapsed_selection(target);
                }
                editing.clear_composition();
            }
            return Ok(target != caret || extend_selection);
        }

        self.move_focused_caret_to_offset(block_id, target, extend_selection)
    }

    pub(crate) fn move_caret_left(&mut self, extend_selection: bool) -> Result<bool, String> {
        self.move_caret_horizontally(false, extend_selection)
    }

    pub(crate) fn move_caret_right(&mut self, extend_selection: bool) -> Result<bool, String> {
        self.move_caret_horizontally(true, extend_selection)
    }

    pub(crate) fn move_caret_up(&mut self, extend_selection: bool) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        if extend_selection {
            self.extend_selection_to_adjacent_visible_block(block_id, -1, true)
        } else {
            self.focus_adjacent_visible_block(block_id, -1, true)
        }
    }

    pub(crate) fn move_caret_down(&mut self, extend_selection: bool) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        if extend_selection {
            self.extend_selection_to_adjacent_visible_block(block_id, 1, false)
        } else {
            self.focus_adjacent_visible_block(block_id, 1, false)
        }
    }

    pub(crate) fn move_focused_caret_to_offset(
        &mut self,
        block_id: BlockId,
        offset: usize,
        extend_selection: bool,
    ) -> Result<bool, String> {
        self.break_typing_coalescing();
        if self.focused_block_id() != Some(block_id) {
            return Ok(false);
        }
        let model = self
            .document
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let previous = self
            .editing
            .session
            .as_ref()
            .map(EditingSession::focus_offset)
            .unwrap_or_else(|| model.len())
            .min(model.len());
        let previous = normalized_grapheme_offset(model.text(), previous);
        let offset = normalized_grapheme_offset(model.text(), offset);
        if extend_selection {
            let anchor = self
                .selection
                .focused_text_selection
                .map(|selection| selection.anchor)
                .unwrap_or(previous);
            self.selection.focused_text_selection = Some(FocusedTextSelection {
                anchor,
                focus: offset,
            });
            self.selection.document_selection = Some(DocumentSelection {
                anchor: TextPosition::downstream(block_id, anchor),
                focus: TextPosition::downstream(block_id, offset),
            });
            if self
                .selection
                .focused_text_selection
                .is_some_and(FocusedTextSelection::is_collapsed)
            {
                self.selection.focused_text_selection = None;
                self.selection.document_selection = None;
            }
        } else {
            self.selection.focused_text_selection = None;
            self.selection.document_selection = None;
        }
        if let Some(editing) = self.editing.session.as_mut() {
            editing.set_input_target(InputTarget::BlockText { block_id });
            if extend_selection {
                if let Some(selection) = self.selection.focused_text_selection {
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

    pub(crate) fn move_focused_caret_to_text_position(
        &mut self,
        position: TextPosition,
        extend_selection: bool,
    ) -> Result<bool, String> {
        let previous_selection = self.document_selection_snapshot();
        let changed = self.move_focused_caret_to_offset(
            position.block_id,
            position.offset,
            extend_selection,
        )?;
        let offset = self
            .caret_offset_for_block(position.block_id)
            .unwrap_or(position.offset);
        let position = TextPosition { offset, ..position };
        self.remember_visual_caret(position);
        if extend_selection && let Some(selection) = self.selection.document_selection.as_mut() {
            if let Some(previous) = previous_selection {
                selection.anchor.affinity = previous.anchor.affinity;
            }
            selection.focus = position;
        }
        Ok(changed || position.affinity != TextAffinity::Downstream)
    }

    fn move_caret_horizontally(
        &mut self,
        forward: bool,
        extend_selection: bool,
    ) -> Result<bool, String> {
        self.break_typing_coalescing();
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let model = self
            .document
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let caret = self
            .editing
            .session
            .as_ref()
            .map(EditingSession::focus_offset)
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
                .selection
                .focused_text_selection
                .map(|selection| selection.anchor)
                .unwrap_or(caret);
            self.selection.focused_text_selection = Some(FocusedTextSelection {
                anchor,
                focus: next,
            });
            self.selection.document_selection = Some(DocumentSelection {
                anchor: TextPosition::downstream(block_id, anchor),
                focus: TextPosition::downstream(block_id, next),
            });
            if self
                .selection
                .focused_text_selection
                .is_some_and(FocusedTextSelection::is_collapsed)
            {
                self.selection.focused_text_selection = None;
                self.selection.document_selection = None;
            }
        } else {
            self.selection.focused_text_selection = None;
            self.selection.document_selection = None;
        }
        if let Some(editing) = self.editing.session.as_mut() {
            editing.set_input_target(InputTarget::BlockText { block_id });
            if extend_selection {
                if let Some(selection) = self.selection.focused_text_selection {
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

    pub(crate) fn focus_adjacent_visible_block(
        &mut self,
        block_id: BlockId,
        direction: i32,
        focus_end: bool,
    ) -> Result<bool, String> {
        let Some(target_id) = self.adjacent_visible_block_id(block_id, direction) else {
            return Ok(false);
        };
        let target_len = self
            .document
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
            self.document
                .text_models
                .get(&block_id)
                .map(PieceTableTextModel::len)
                .unwrap_or(0)
        });
        let anchor = self
            .selection
            .document_selection
            .map(|selection| selection.anchor)
            .unwrap_or_else(|| TextPosition::downstream(block_id, caret));
        let target_offset = if target_end {
            self.document
                .text_models
                .get(&target_id)
                .map(PieceTableTextModel::len)
                .unwrap_or(0)
        } else {
            0
        };
        self.focus_block_at_offset(target_id, target_offset)?;
        self.selection.document_selection = Some(DocumentSelection {
            anchor,
            focus: TextPosition::downstream(target_id, target_offset),
        });
        self.selection.focused_text_selection = None;
        Ok(true)
    }

    pub(super) fn adjacent_visible_block_id(
        &self,
        block_id: BlockId,
        direction: i32,
    ) -> Option<BlockId> {
        let index = self.document.visible_index.visible_index_of(block_id)?;
        let target = if direction < 0 {
            index.checked_sub(1)?
        } else {
            index.checked_add(1)?
        };
        self.document.visible_index.id_at_visible_index(target)
    }
}

fn logical_line_boundary(text: &str, caret: usize, to_end: bool) -> usize {
    let caret = normalized_grapheme_offset(text, caret);
    if to_end {
        text[caret..]
            .char_indices()
            .find(|(_, ch)| matches!(ch, '\r' | '\n' | '\u{2028}' | '\u{2029}'))
            .map(|(index, _)| caret + index)
            .unwrap_or(text.len())
    } else {
        text[..caret]
            .char_indices()
            .rev()
            .find(|(_, ch)| matches!(ch, '\r' | '\n' | '\u{2028}' | '\u{2029}'))
            .map(|(index, ch)| index + ch.len_utf8())
            .unwrap_or(0)
    }
}

fn word_boundary(text: &str, caret: usize, forward: bool) -> usize {
    use unicode_segmentation::UnicodeSegmentation;

    let caret = normalized_grapheme_offset(text, caret);
    if forward {
        text.unicode_word_indices()
            .find_map(|(start, word)| {
                let end = start + word.len();
                (end > caret).then_some(end)
            })
            .unwrap_or(text.len())
    } else {
        text.unicode_word_indices()
            .take_while(|(start, _)| *start < caret)
            .map(|(start, _)| start)
            .last()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod word_tests {
    use super::word_boundary;

    #[test]
    fn unicode_word_boundaries_cross_punctuation_cjk_and_emoji_without_byte_splits() {
        let text = "hello, 世界 👩‍💻 rust";
        assert_eq!(word_boundary(text, 0, true), 5);
        let cjk_start = text.find('世').unwrap();
        let first_cjk_end = cjk_start + '世'.len_utf8();
        let cjk_end = cjk_start + "世界".len();
        assert_eq!(word_boundary(text, 5, true), first_cjk_end);
        let rust_start = text.find("rust").unwrap();
        assert_eq!(word_boundary(text, text.len(), false), rust_start);
        assert_eq!(word_boundary(text, cjk_end, false), first_cjk_end);
    }
}
