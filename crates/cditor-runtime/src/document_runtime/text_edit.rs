use super::markdown_transaction::payload_replace_operation;
use super::*;

impl DocumentRuntime {
    pub(crate) fn replace_focused_range_with_rich_text_spans(
        &mut self,
        inserted_spans: &[InlineSpan],
    ) -> Result<bool, String> {
        if self.selection.focused_table_cell.is_some() || inserted_spans.is_empty() {
            return Ok(false);
        }
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        if self.editing.session.is_none() {
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
        let surface_id = SurfaceId::Block(block_id);
        let Some(snapshot) = self.text_surface_base_snapshot(surface_id) else {
            return Ok(false);
        };
        if !snapshot.capabilities.rich_text {
            return Ok(false);
        }
        let inserted_text = inserted_spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>();
        self.cancel_composition();
        let next_offset = range.start.saturating_add(inserted_text.len());
        let next_spans = replace_rich_text_spans_with_spans(&snapshot.spans, range, inserted_spans);
        if !self.apply_text_surface_paste_transaction(
            surface_id,
            snapshot.spans,
            next_spans,
            next_offset,
        )? {
            return Ok(false);
        }
        self.selection.document_selection = None;
        self.selection.focused_text_selection = None;
        if let Some(editing) = self.editing.session.as_mut() {
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_collapsed_selection(next_offset);
        }
        let content_version = self.block_content_version(block_id).unwrap_or_default();
        let text_len_after = self
            .document
            .text_models
            .get(&block_id)
            .map(PieceTableTextModel::len)
            .unwrap_or_default();
        trace_input(
            "replace_focused_range_with_rich_text_spans.end",
            format_args!(
                "block={block_id} caret_after={:?} content_version={content_version} text_len={text_len_after}",
                self.caret_offset_for_block(block_id)
            ),
        );
        Ok(true)
    }

    pub(crate) fn insert_char(&mut self, ch: char) -> Result<(), String> {
        if self.selection.focused_table_cell.is_some() {
            self.replace_text_from_platform(None, &ch.to_string())?;
            return Ok(());
        }
        if self.focused_text_selection_range().is_some() {
            self.replace_text_from_platform(None, &ch.to_string())?;
            return Ok(());
        }
        let block_id = self.focused_block_id().unwrap_or(1);
        if self.editing.session.is_none() {
            self.focus_block(block_id);
        }
        if self.selection.focused_table_cell.is_none() && self.focused_block_is_table() {
            return Ok(());
        }
        let offset = {
            let model = self
                .document
                .text_models
                .get(&block_id)
                .ok_or_else(|| format!("missing text model for block {block_id}"))?;
            let offset = self
                .editing
                .session
                .as_ref()
                .map(EditingSession::focus_offset)
                .unwrap_or_else(|| model.len());
            normalized_grapheme_offset(model.text(), offset)
        };
        let surface_id = SurfaceId::Block(block_id);
        let typing_marks = self.typing_marks_for(surface_id, offset);
        self.editing
            .hot_path
            .preflight_insert_char(
                self.document
                    .text_models
                    .get(&block_id)
                    .expect("text model was resolved above"),
                offset,
            )
            .map_err(|error| format!("{error:?}"))?;
        let shortcut_before = if matches!(ch, '*' | '_' | '~' | '+' | '`' | ')' | ']') {
            let before = (
                self.document
                    .payload_window
                    .get(block_id)
                    .cloned()
                    .ok_or_else(|| format!("missing payload for block {block_id}"))?,
                self.text_surface_base_snapshot(surface_id)
                    .ok_or_else(|| format!("missing text surface {surface_id:?}"))?,
                self.document_selection_snapshot(),
                self.capture_undo_scroll_snapshot().anchor,
                self.document
                    .index
                    .index_of(block_id)
                    .map(|index| self.document.index.layout_meta[index].layout_version)
                    .ok_or_else(|| format!("missing layout metadata for block {block_id}"))?,
            );
            super::local_transaction::validate_preapplied_text_preflight(
                surface_id,
                &before.0,
                &before.1,
                offset..offset,
            )?;
            Some(before)
        } else {
            None
        };
        self.selection.selected_block_ids.clear();
        let coalesces_typing = !matches!(ch, '\r' | '\n');
        if coalesces_typing {
            self.prepare_typing_undo(surface_id, offset);
        } else {
            self.break_typing_coalescing();
        }
        self.push_undo_snapshot(block_id)?;
        let transaction_id = self.transactions.next_id;
        let hot_path_result = {
            let model = self
                .document
                .text_models
                .get_mut(&block_id)
                .ok_or_else(|| format!("missing text model for block {block_id}"))?;
            let editing = self
                .editing
                .session
                .as_mut()
                .expect("editing session exists");
            let result = self
                .editing
                .hot_path
                .handle_insert_char_with_transaction(
                    editing,
                    model,
                    offset,
                    ch,
                    transaction_id,
                    transaction_id,
                )
                .map_err(|error| format!("{error:?}"))?;
            super::typing_marks::sync_payload_after_replace_with_typing_marks(
                &mut self.document.payload_window,
                block_id,
                editing.content_version,
                model,
                offset..offset,
                &ch.to_string(),
                typing_marks,
            );
            result
        };
        self.transactions.next_id = self.transactions.next_id.saturating_add(1);
        if coalesces_typing {
            let next_offset = self.text_surface_caret_offset(surface_id);
            self.finish_typing_undo(surface_id, next_offset, true);
        }
        let applied_shortcut = self.apply_inline_markdown_shortcut(block_id)?;
        if !applied_shortcut {
            self.advance_typing_mark_override(
                surface_id,
                offset,
                self.caret_offset_for_block(block_id).unwrap_or(offset),
            );
        }
        if applied_shortcut {
            if let Some((
                before_record,
                before_surface,
                before_selection,
                before_anchor,
                before_layout_version,
            )) = shortcut_before
            {
                let after_record = self
                    .document
                    .payload_window
                    .get(block_id)
                    .cloned()
                    .expect("pre-applied shortcut must preserve its owner payload");
                self.record_preapplied_text_replacement(
                    super::local_transaction::PreappliedTextEdit {
                        transaction_id,
                        kind: EditTransactionKind::Typing,
                        origin: cditor_core::edit::ChangeOrigin::User,
                        surface_id,
                        replaced_range: offset..offset,
                        inserted_text: ch.to_string(),
                        before_record,
                        after_record,
                        before_surface,
                        after_surface: self.text_surface_base_snapshot(surface_id),
                        before_selection,
                        after_selection: self.document_selection_snapshot(),
                        before_anchor,
                        after_anchor: self.capture_undo_scroll_snapshot().anchor,
                        before_layout_version,
                    },
                );
            }
        } else {
            self.record_preapplied_insert_transaction(
                hot_path_result.transaction,
                surface_id,
                offset,
                ch.len_utf8(),
            );
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn insert_space_or_markdown_shortcut(&mut self) -> Result<(), String> {
        self.replace_text_from_platform(None, " ")?;
        Ok(())
    }

    pub(crate) fn insert_soft_line_break(&mut self) -> Result<(), String> {
        match self.focused_text_surface_id() {
            Some(cditor_core::ids::SurfaceId::ImageCaption { .. }) => {
                self.replace_text_in_focused_range(None, "\n")?;
                return Ok(());
            }
            Some(cditor_core::ids::SurfaceId::CollectionTitle { .. }) => return Ok(()),
            _ => {}
        }
        self.insert_char('\n')?;
        if self.selection.focused_table_cell.is_some() {
            return Ok(());
        }
        let _ = self.refresh_focused_text_block_height()?;
        Ok(())
    }

    pub(super) fn refresh_focused_text_block_height(&mut self) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(document_index) = self.document.index.index_of(block_id) else {
            return Ok(false);
        };
        let Some(visible_index) = self.document.visible_index.visible_index_of(block_id) else {
            return Ok(false);
        };
        let kind = self
            .document
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or_else(|| RichBlockKind::Paragraph);
        let text = self
            .document
            .text_models
            .get(&block_id)
            .map(|model| model.text().to_owned())
            .unwrap_or_default();
        let next_height = estimate_text_block_height_for_text(&kind, &text);
        let previous_height = self.document.index.layout_meta[document_index].effective_height();
        if (previous_height - next_height).abs() < 0.5 {
            return Ok(false);
        }

        self.document.index.layout_meta[document_index].update_height(next_height);
        let height_change = self
            .layout
            .height_index
            .update_height(visible_index, next_height)
            .map_err(|error| error.to_string())?;
        if let Some(page_index) = self.layout.page_layout.page_for_block_index(visible_index) {
            let next_page_height =
                self.layout.page_layout.pages[page_index].height + height_change.delta;
            self.layout
                .page_layout
                .update_page_height(page_index, next_page_height)
                .map_err(|error| error.to_string())?;
        }
        let total_height = self.scroll_extent_height(self.layout.page_layout.total_height());
        self.layout
            .scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        self.layout
            .scroll
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
            .session
            .as_ref()
            .map(EditingSession::focus_offset)
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
            .session
            .as_ref()
            .map(EditingSession::focus_offset)
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

    pub(crate) fn delete_backward(&mut self) -> Result<bool, String> {
        if self.selection.focused_table_cell.is_some() {
            return self.delete_backward_in_focused_table_cell();
        }
        if let Some(changed) = self.delete_in_auxiliary_text_surface(true)? {
            return Ok(changed);
        }
        if self.focused_block_is_table() {
            return Ok(false);
        }

        if self.has_active_selection() {
            return self.delete_active_selection();
        }
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        // Non-text blocks (whiteboard, image, etc.) have no text model.
        // Backspace deletes them outright.
        let Some(text_model) = self.document.text_models.get(&block_id) else {
            return self.delete_block_by_id(block_id);
        };
        let text = text_model.text().to_owned();
        let caret = self
            .editing
            .session
            .as_ref()
            .map(EditingSession::focus_offset)
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
        self.replace_text_in_focused_range(Some(range.start.0..range.end.0), "")
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
            .document
            .text_models
            .get(&block_id)
            .ok_or_else(|| format!("missing text model for block {block_id}"))?
            .text()
            .to_owned();
        self.cancel_composition();
        self.selection.selected_block_ids.clear();
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::BlockStructureChange,
            RichBlockKind::Paragraph,
            BlockPayload::RichText {
                spans: vec![InlineSpan::plain(text)],
            },
        )?;
        self.focus_block_at_offset(block_id, 0)?;
        Ok(true)
    }

    pub(crate) fn delete_forward(&mut self) -> Result<bool, String> {
        if self.selection.focused_table_cell.is_some() {
            return self.delete_forward_in_focused_table_cell();
        }
        if let Some(changed) = self.delete_in_auxiliary_text_surface(false)? {
            return Ok(changed);
        }
        if self.focused_block_is_table() {
            return Ok(false);
        }

        if self.has_active_selection() {
            return self.delete_active_selection();
        }
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        // Non-text blocks (whiteboard, image, etc.) have no text model.
        // Delete key removes them outright.
        let Some(model) = self.document.text_models.get(&block_id) else {
            return self.delete_block_by_id(block_id);
        };
        let caret = self
            .editing
            .session
            .as_ref()
            .map(EditingSession::focus_offset)
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

    pub(super) fn focused_block_is_table(&self) -> bool {
        self.focused_block_id()
            .is_some_and(|block_id| matches!(self.kind_for_block(block_id), RichBlockKind::Table))
    }

    /// Delete all blocks in selected_block_ids
    /// This handles whole-block multi-selection deletion (e.g., via gutter drag)
    pub(super) fn delete_selected_blocks(&mut self) -> Result<bool, String> {
        if self.selection.selected_block_ids.is_empty() {
            return Ok(false);
        }

        let before_selection = self.document_selection_snapshot();
        let before_selected_blocks = self.selected_block_ids_snapshot();
        let records = self.index_records();
        let selected = &self.selection.selected_block_ids;
        let mut roots = selected
            .iter()
            .filter_map(|block_id| {
                self.document
                    .index
                    .index_of(*block_id)
                    .map(|index| (*block_id, index))
            })
            .filter(|(_, index)| {
                let mut parent = self.document.index.parent_ids[*index];
                while let Some(parent_id) = parent {
                    if selected.contains(&parent_id) {
                        return false;
                    }
                    parent = self
                        .document
                        .index
                        .index_of(parent_id)
                        .and_then(|position| self.document.index.parent_ids[position]);
                }
                true
            })
            .collect::<Vec<_>>();
        roots.sort_by_key(|(_, index)| *index);
        let Some((_, start)) = roots.first().copied() else {
            self.selection.selected_block_ids.clear();
            return Ok(false);
        };
        let mut end = start;
        for (_, root_index) in roots {
            if root_index != end {
                return Err("whole-block selection must be one contiguous range".to_owned());
            }
            end = self.subtree_end(root_index);
        }

        let all_selected = start == 0 && end == records.len();
        let current_index = if all_selected {
            0
        } else if start > 0 {
            start - 1
        } else {
            end
        };
        let current_block_id = records[current_index].id;
        let before_current_payload = self
            .document
            .payload_window
            .get(current_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {current_block_id}"))?;

        let deleted_range = if all_selected { 1..end } else { start..end };
        let deleted_records = records[deleted_range.clone()].to_vec();
        let deleted_payloads = deleted_records
            .iter()
            .map(|record| {
                self.document
                    .payload_window
                    .get(record.id)
                    .cloned()
                    .ok_or_else(|| format!("missing payload for block {}", record.id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (after_current_kind, after_current_payload) = if all_selected {
            (
                RichBlockKind::Paragraph,
                BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("")],
                },
            )
        } else {
            (
                before_current_payload.kind.clone(),
                before_current_payload.payload.clone(),
            )
        };
        let focus_offset = if start > 0 && !all_selected {
            after_current_payload.plain_text().len()
        } else {
            0
        };
        let after_selection = matches!(
            &after_current_payload,
            BlockPayload::RichText { .. } | BlockPayload::Code { .. } | BlockPayload::Html { .. }
        )
        .then_some(DocumentSelection::caret(TextPosition::downstream(
            current_block_id,
            focus_offset,
        )));

        let mut ops = Vec::with_capacity(2);
        let mut inverse_ops = Vec::with_capacity(2);
        if all_selected {
            ops.push(payload_replace_operation(
                current_block_id,
                before_current_payload.kind.clone(),
                before_current_payload.payload.clone(),
                after_current_kind.clone(),
                after_current_payload.clone(),
            ));
        }
        if !deleted_records.is_empty() {
            ops.push(EditOperation::DeleteBlockRange {
                range: deleted_range.clone(),
            });
            inverse_ops.push(EditOperation::InsertBlocks {
                index: deleted_range.start,
                blocks: deleted_records,
                payloads: deleted_payloads,
            });
        }
        if all_selected {
            inverse_ops.push(payload_replace_operation(
                current_block_id,
                after_current_kind,
                after_current_payload,
                before_current_payload.kind.clone(),
                before_current_payload.payload.clone(),
            ));
        }
        let mut preconditions = vec![TransactionPrecondition::StructureVersion(
            self.structure_version(),
        )];
        if all_selected {
            preconditions.push(TransactionPrecondition::BlockContentVersion {
                block_id: current_block_id,
                version: before_current_payload.content_version,
            });
        }
        self.cancel_composition();
        self.apply_local_structure_transaction(
            EditTransactionKind::BlockStructureChange,
            cditor_core::edit::ChangeOrigin::User,
            ops,
            inverse_ops,
            preconditions,
            before_selection,
            after_selection,
            before_selected_blocks,
            Vec::new(),
        )?;
        if after_selection.is_some() {
            self.focus_block_at_offset(current_block_id, focus_offset)?;
        } else {
            self.focus_block(current_block_id);
        }
        Ok(true)
    }
}
