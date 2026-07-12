use super::*;

impl DocumentRuntime {
    pub fn replace_text_in_focused_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
    ) -> Result<bool, String> {
        let explicit_range = range.clone();
        let Some(edit) = self.resolve_focused_text_edit(range) else {
            return Ok(false);
        };
        if matches!(edit.target, InputTarget::TableCell { .. }) {
            return self.apply_focused_table_cell_text_edit(edit, text);
        }
        let InputTarget::BlockText { block_id } = edit.target else {
            return Ok(false);
        };
        if self.focused_block_is_table() {
            self.cancel_composition();
            return Ok(false);
        }
        let range = edit.range;
        trace_input(
            "replace_text_in_focused_range.range",
            format_args!(
                "block={block_id} explicit_range={explicit_range:?} resolved_range={range:?} insert_len={} caret_before={:?} focused_selection={:?} active_composition={:?}",
                text.len(),
                self.caret_offset_for_block(block_id),
                self.focused_text_selection_range(),
                self.active_composition()
                    .map(|composition| composition.range_start as usize
                        ..composition.range_end as usize)
            ),
        );
        if text == " "
            && range.is_empty()
            && self.try_apply_space_block_markdown_shortcut(block_id, range.start)?
        {
            trace_input(
                "replace_text_in_focused_range.space_shortcut",
                format_args!("block={block_id} shortcut_offset={}", range.start),
            );
            return Ok(true);
        }

        self.cancel_composition();
        self.document_selection = None;
        self.focused_text_selection = None;
        self.push_undo_snapshot(block_id)?;
        let replaced_range = range.clone();
        let (content_version, text_len_after) = {
            let model = self
                .text_models
                .get_mut(&block_id)
                .ok_or_else(|| format!("missing text model for block {block_id}"))?;
            let inserted = model
                .replace_range(range, text)
                .map_err(|error| format!("{error:?}"))?;
            let editing = self.editing.as_mut().expect("editing session exists");
            editing.content_version += 1;
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_collapsed_selection(inserted.end);
            sync_payload_from_model_after_replace(
                &mut self.payload_window,
                block_id,
                editing.content_version,
                model,
                replaced_range,
                text,
            );
            (editing.content_version, model.len())
        };
        self.apply_inline_markdown_shortcut(block_id)?;
        trace_input(
            "replace_text_in_focused_range.end",
            format_args!(
                "block={block_id} caret_after={:?} content_version={content_version} text_len={text_len_after}",
                self.caret_offset_for_block(block_id)
            ),
        );
        Ok(true)
    }

    pub fn replace_focused_range_with_rich_text_spans(
        &mut self,
        inserted_spans: &[InlineSpan],
    ) -> Result<bool, String> {
        if self.focused_table_cell.is_some() || inserted_spans.is_empty() {
            return Ok(false);
        }
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        if self.editing.is_none() {
            self.focus_block(block_id);
        }
        let Some(edit) = self.resolve_focused_text_edit(None) else {
            return Ok(false);
        };
        let InputTarget::BlockText {
            block_id: target_block_id,
        } = edit.target
        else {
            return Ok(false);
        };
        if target_block_id != block_id {
            return Err(format!(
                "focused rich text target mismatch: focused={block_id} edit={target_block_id}"
            ));
        }
        let range = edit.range;
        let Some(existing_spans) = self.payload_window.get(block_id).and_then(|payload| {
            if let BlockPayload::RichText { spans } = &payload.payload {
                Some(spans.clone())
            } else {
                None
            }
        }) else {
            return Ok(false);
        };
        let inserted_text = inserted_spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>();
        self.cancel_composition();
        self.document_selection = None;
        self.focused_text_selection = None;
        self.push_undo_snapshot(block_id)?;
        let replaced_range = range.clone();
        let (content_version, text_len_after) = {
            let model = self
                .text_models
                .get_mut(&block_id)
                .ok_or_else(|| format!("missing text model for block {block_id}"))?;
            let inserted = model
                .replace_range(range, &inserted_text)
                .map_err(|error| format!("{error:?}"))?;
            let editing = self.editing.as_mut().expect("editing session exists");
            editing.content_version += 1;
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_collapsed_selection(inserted.end);
            let spans =
                replace_rich_text_spans_with_spans(&existing_spans, replaced_range, inserted_spans);
            if let Some(payload) = self.payload_window.payloads.get_mut(&block_id) {
                payload.content_version = editing.content_version;
                payload.payload = BlockPayload::RichText { spans };
            }
            (editing.content_version, model.len())
        };
        trace_input(
            "replace_focused_range_with_rich_text_spans.end",
            format_args!(
                "block={block_id} caret_after={:?} content_version={content_version} text_len={text_len_after}",
                self.caret_offset_for_block(block_id)
            ),
        );
        Ok(true)
    }

    pub fn insert_char(&mut self, ch: char) -> Result<(), String> {
        if self.focused_table_cell.is_some() {
            self.replace_text_in_focused_range(None, &ch.to_string())?;
            return Ok(());
        }
        if self.focused_text_selection_range().is_some() {
            self.replace_text_in_focused_range(None, &ch.to_string())?;
            return Ok(());
        }
        let block_id = self.focused_block_id().unwrap_or(1);
        if self.editing.is_none() {
            self.focus_block(block_id);
        }
        if self.focused_table_cell.is_none() && self.focused_block_is_table() {
            return Ok(());
        }
        self.push_undo_snapshot(block_id)?;
        self.selected_block_ids.clear();
        let model = self
            .text_models
            .get_mut(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        let offset = self
            .editing
            .as_ref()
            .map(|editing| editing.caret_anchor.text_offset as usize)
            .unwrap_or_else(|| model.len());
        let offset = normalized_grapheme_offset(model.text(), offset);
        let editing = self.editing.as_mut().expect("editing session exists");
        self.hot_path
            .handle_insert_char(editing, model, offset, ch)
            .map_err(|error| format!("{error:?}"))?;
        sync_payload_from_model_after_replace(
            &mut self.payload_window,
            block_id,
            editing.content_version,
            model,
            offset..offset,
            &ch.to_string(),
        );
        self.apply_inline_markdown_shortcut(block_id)?;
        Ok(())
    }

    pub fn insert_space_or_markdown_shortcut(&mut self) -> Result<(), String> {
        if self.focused_table_cell.is_some() {
            self.replace_text_in_focused_range(None, " ")?;
            return Ok(());
        }
        let block_id = self.focused_block_id().unwrap_or(1);
        if self.editing.is_none() {
            self.focus_block(block_id);
        }
        if self.focused_table_cell.is_none() && self.focused_block_is_table() {
            return Ok(());
        }
        let caret = {
            let model = self
                .text_models
                .get(&block_id)
                .ok_or_else(|| format!("missing text model for block {block_id}"))?;
            let caret = self
                .editing
                .as_ref()
                .map(|editing| editing.caret_anchor.text_offset as usize)
                .unwrap_or_else(|| model.len());
            normalized_grapheme_offset(model.text(), caret)
        };
        if self.try_apply_space_block_markdown_shortcut(block_id, caret)? {
            return Ok(());
        }
        self.insert_char(' ')
    }

    pub fn insert_soft_line_break(&mut self) -> Result<(), String> {
        self.insert_char('\n')?;
        if self.focused_table_cell.is_some() {
            return Ok(());
        }
        let _ = self.refresh_focused_text_block_height()?;
        Ok(())
    }

    pub(super) fn refresh_focused_text_block_height(&mut self) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(document_index) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let Some(visible_index) = self.visible_index.visible_index_of(block_id) else {
            return Ok(false);
        };
        let kind = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or_else(|| RichBlockKind::Paragraph);
        let text = self
            .text_models
            .get(&block_id)
            .map(|model| model.text().to_owned())
            .unwrap_or_default();
        let next_height = estimate_text_block_height_for_text(&kind, &text);
        let previous_height = self.index.layout_meta[document_index].effective_height();
        if (previous_height - next_height).abs() < 0.5 {
            return Ok(false);
        }

        self.index.layout_meta[document_index].update_height(next_height);
        let height_change = self
            .height_index
            .update_height(visible_index, next_height)
            .map_err(|error| error.to_string())?;
        if let Some(page_index) = self.page_layout.page_for_block_index(visible_index) {
            let next_page_height = self.page_layout.pages[page_index].height + height_change.delta;
            self.page_layout
                .update_page_height(page_index, next_page_height)
                .map_err(|error| error.to_string())?;
        }
        let total_height = self.scroll_extent_height(self.height_index.total_height());
        self.scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        self.scroll
            .set_displayed_total_height(total_height)
            .map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub(super) fn insert_soft_tab_in_focused_block(&mut self) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let caret = self
            .editing
            .as_ref()
            .map(|editing| editing.caret_anchor.text_offset as usize)
            .unwrap_or_else(|| self.focused_text().map(str::len).unwrap_or(0));
        let changed = self.replace_text_in_focused_range(Some(caret..caret), "    ")?;
        if changed {
            let _ = self.refresh_focused_text_block_height()?;
            self.focus_block_at_offset(block_id, caret + 4)?;
        }
        Ok(changed)
    }

    pub(super) fn outdent_soft_tab_in_focused_block(&mut self) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(text) = self.focused_text().map(ToOwned::to_owned) else {
            return Ok(false);
        };
        let caret = self
            .editing
            .as_ref()
            .map(|editing| editing.caret_anchor.text_offset as usize)
            .unwrap_or(text.len())
            .min(text.len());
        let caret = normalized_grapheme_offset(&text, caret);
        let line_start = text[..caret].rfind('\n').map_or(0, |index| index + 1);
        let remove_len = text[line_start..]
            .chars()
            .take_while(|ch| *ch == ' ')
            .take(4)
            .map(char::len_utf8)
            .sum::<usize>();
        if remove_len == 0 {
            return Ok(false);
        }
        let changed =
            self.replace_text_in_focused_range(Some(line_start..line_start + remove_len), "")?;
        if changed {
            let _ = self.refresh_focused_text_block_height()?;
            let next_caret = caret.saturating_sub(remove_len);
            self.focus_block_at_offset(block_id, next_caret)?;
        }
        Ok(changed)
    }

    pub fn delete_backward(&mut self) -> Result<bool, String> {
        if self.focused_table_cell.is_some() {
            return self.delete_backward_in_focused_table_cell();
        }
        if self.focused_block_is_table() {
            return Ok(false);
        }

        // Check for whole block selection first (before document_selection)
        if !self.selected_block_ids.is_empty() {
            return self.delete_selected_blocks();
        }

        if self
            .document_selection
            .is_some_and(|selection| !selection.is_caret())
        {
            return self.delete_document_selection();
        }
        if self.focused_text_selection_range().is_some() {
            return self.replace_text_in_focused_range(None, "");
        }
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        // Non-text blocks (whiteboard, image, etc.) have no text model.
        // Backspace deletes them outright.
        let Some(text_model) = self.text_models.get(&block_id) else {
            return self.delete_focused_empty_block_backward();
        };
        let text = text_model.text().to_owned();
        let caret = self
            .editing
            .as_ref()
            .map(|editing| editing.caret_anchor.text_offset as usize)
            .unwrap_or_else(|| text.len());
        let caret = normalized_grapheme_offset(&text, caret);
        if caret == 0 && self.try_reset_focused_block_style_at_start(block_id)? {
            return Ok(true);
        }
        if caret == 0 {
            if text.is_empty() {
                return self.delete_focused_empty_block_backward();
            }
            return self.merge_focused_block_into_previous();
        }
        let offsets = TextOffsetMap::build(&text);
        let Some(range) = offsets
            .backspace_range(InternalTextOffset(caret))
            .map_err(|error| format!("{error:?}"))?
        else {
            return Ok(false);
        };
        self.push_undo_snapshot(block_id)?;
        self.selected_block_ids.clear();
        let model = self
            .text_models
            .get_mut(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?;
        model
            .replace_range(range.start.0..range.end.0, "")
            .map_err(|error| format!("{error:?}"))?;
        let editing = self
            .editing
            .as_mut()
            .expect("focused block has editing session");
        editing.content_version += 1;
        editing.caret_anchor.text_offset = range.start.0 as u64;
        editing.set_input_target(InputTarget::BlockText { block_id });
        editing.set_collapsed_selection(range.start.0);
        sync_payload_from_model_after_replace(
            &mut self.payload_window,
            block_id,
            editing.content_version,
            model,
            range.start.0..range.end.0,
            "",
        );
        Ok(true)
    }

    fn try_reset_focused_block_style_at_start(
        &mut self,
        block_id: BlockId,
    ) -> Result<bool, String> {
        let kind = self.kind_for_block(block_id);
        if !backspace_at_start_resets_kind_to_paragraph(&kind) {
            return Ok(false);
        }
        let text = self
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();
        self.cancel_composition();
        self.push_undo_snapshot(block_id)?;
        self.selected_block_ids.clear();
        self.replace_block_kind_and_payload(
            block_id,
            RichBlockKind::Paragraph,
            BlockPayload::RichText {
                spans: vec![InlineSpan::plain(text)],
            },
        )?;
        self.focus_block_at_offset(block_id, 0)?;
        Ok(true)
    }

    pub fn delete_forward(&mut self) -> Result<bool, String> {
        if self.focused_table_cell.is_some() {
            return self.delete_forward_in_focused_table_cell();
        }
        if self.focused_block_is_table() {
            return Ok(false);
        }

        // Check for whole block selection first
        if !self.selected_block_ids.is_empty() {
            return self.delete_selected_blocks();
        }

        // Check for cross-block document selection
        if self
            .document_selection
            .is_some_and(|selection| !selection.is_caret())
        {
            return self.delete_document_selection();
        }

        if self.focused_text_selection_range().is_some() {
            return self.replace_text_in_focused_range(None, "");
        }
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        // Non-text blocks (whiteboard, image, etc.) have no text model.
        // Delete key removes them outright.
        let Some(model) = self.text_models.get(&block_id) else {
            return self.delete_focused_empty_block_forward();
        };
        let caret = self
            .editing
            .as_ref()
            .map(|editing| editing.caret_anchor.text_offset as usize)
            .unwrap_or_else(|| model.len());
        let caret = normalized_grapheme_offset(model.text(), caret);
        let next = next_grapheme_boundary(model.text(), caret);
        if caret == next {
            if model.text().is_empty() {
                return self.delete_focused_empty_block_forward();
            }
            if caret == model.len()
                && let Some(next_id) = self.adjacent_visible_block_id(block_id, 1)
            {
                return self.merge_block_into_previous(next_id, block_id);
            }
            return Ok(false);
        }
        self.replace_text_in_focused_range(Some(caret..next), "")
    }

    fn focused_block_is_table(&self) -> bool {
        self.focused_block_id()
            .is_some_and(|block_id| matches!(self.kind_for_block(block_id), RichBlockKind::Table))
    }

    /// Delete all blocks in selected_block_ids
    /// This handles whole-block multi-selection deletion (e.g., via gutter drag)
    pub(super) fn delete_selected_blocks(&mut self) -> Result<bool, String> {
        if self.selected_block_ids.is_empty() {
            return Ok(false);
        }

        // Sort block IDs by document order for safe deletion
        let mut blocks_to_delete: Vec<BlockId> = self.selected_block_ids.iter().copied().collect();
        blocks_to_delete.sort_by_key(|&id| self.index.index_of(id).unwrap_or(usize::MAX));

        // Filter out blocks that don't exist in index
        blocks_to_delete.retain(|&id| self.index.index_of(id).is_some());

        if blocks_to_delete.is_empty() {
            self.selected_block_ids.clear();
            return Ok(false);
        }

        // Find a safe place to put the caret after deletion
        // Try the block before the first deleted block, or after the last deleted block
        let first_delete_index = self.index.index_of(blocks_to_delete[0]).unwrap();
        let focus_target = if first_delete_index > 0 {
            self.index.block_ids.get(first_delete_index - 1).copied()
        } else {
            // If deleting from start, try the block after the deletion range
            let last_delete_index = self
                .index
                .index_of(*blocks_to_delete.last().unwrap())
                .unwrap();
            self.index.block_ids.get(last_delete_index + 1).copied()
        };

        // Push undo snapshot (using first block as reference)
        self.push_undo_snapshot(blocks_to_delete[0])?;

        // Delete blocks in reverse order to avoid index shifts
        for &block_id in blocks_to_delete.iter().rev() {
            self.delete_block_by_id(block_id)?;
        }

        // Clear selection state
        self.selected_block_ids.clear();
        self.document_selection = None;
        self.focused_text_selection = None;

        // If we deleted everything, ensure we have at least one empty paragraph
        if self.index.block_ids.is_empty() {
            // Create a new empty paragraph
            let new_block_id = 1u64; // Start with ID 1 for empty document
            let payload = BlockPayloadRecord::rich_text(
                new_block_id,
                RichBlockKind::Paragraph,
                String::new(),
            );
            let record = BlockIndexRecord::new(
                new_block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            );
            self.rebuild_structure_index(vec![record])?;
            self.payload_window.insert(payload);
            self.focus_block_at_offset(new_block_id, 0)?;
        } else if let Some(target_id) = focus_target {
            // Focus the target block
            if let Some(model) = self.text_models.get(&target_id) {
                let text_len = model.len();
                self.focus_block_at_offset(target_id, text_len)?;
            } else {
                // For non-text blocks, just focus them
                self.focus_block(target_id);
            }
        } else {
            // Fallback: focus the first remaining block
            if let Some(&first_id) = self.index.block_ids.first() {
                self.focus_block_at_offset(first_id, 0)?;
            }
        }

        Ok(true)
    }
}
