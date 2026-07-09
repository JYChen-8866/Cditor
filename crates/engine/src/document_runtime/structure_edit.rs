use super::*;

impl DocumentRuntime {
    pub fn convert_focused_block_kind(&mut self, kind: RichBlockKind) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(record) = self.payload_window.get(block_id).cloned() else {
            return Ok(false);
        };
        if record.kind == kind {
            return Ok(false);
        }
        let text = record.plain_text();
        self.push_undo_snapshot(block_id)?;
        let payload = payload_for_converted_kind(&kind, text);
        self.replace_block_kind_and_payload(block_id, kind, payload)?;
        Ok(true)
    }

    pub fn set_code_block_language(
        &mut self,
        block_id: BlockId,
        language: Option<String>,
    ) -> Result<bool, String> {
        let language = language.and_then(|language| {
            let trimmed = language.trim().to_lowercase();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        let Some(record) = self.payload_window.get(block_id).cloned() else {
            return Ok(false);
        };
        let BlockPayload::Code { text, .. } = record.payload else {
            return Ok(false);
        };
        if matches!(&record.kind, RichBlockKind::Code { language: current } if current == &language)
            && matches!(self.payload_window.get(block_id).map(|record| &record.payload), Some(BlockPayload::Code { language: current, .. }) if current == &language)
        {
            return Ok(false);
        }
        self.push_undo_snapshot(block_id)?;
        self.replace_block_kind_and_payload(
            block_id,
            RichBlockKind::Code {
                language: language.clone(),
            },
            BlockPayload::Code { language, text },
        )?;
        Ok(true)
    }

    pub fn toggle_todo_checked(&mut self, block_id: BlockId) -> Result<bool, String> {
        let Some(record) = self.payload_window.payloads.get_mut(&block_id) else {
            return Ok(false);
        };
        let checked = match &record.kind {
            RichBlockKind::Todo { checked } => *checked,
            _ => return Ok(false),
        };
        record.kind = RichBlockKind::Todo { checked: !checked };
        record.content_version = record.content_version.saturating_add(1);
        if let Some(editing) = self
            .editing
            .as_mut()
            .filter(|editing| editing.block_id == block_id)
        {
            editing.content_version = record.content_version;
        }
        if let Some(index) = self.index.index_of(block_id) {
            self.index.kind_tags[index] = kind_tag_for_rich_block_kind(&record.kind);
        }
        Ok(true)
    }

    pub fn handle_enter(&mut self) -> Result<(), String> {
        let Some(block_id) = self.focused_block_id() else {
            self.insert_paragraph_after_focused()?;
            return Ok(());
        };
        let kind = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or_else(|| RichBlockKind::Paragraph);
        if matches!(kind, RichBlockKind::Table) {
            if self.focused_table_cell.is_some() {
                self.insert_soft_line_break()?;
            }
            return Ok(());
        }
        let text = self
            .text_models
            .get(&block_id)
            .map(|model| model.text().to_owned())
            .unwrap_or_default();
        if matches!(kind, RichBlockKind::Paragraph)
            && let Some(RichBlockKind::Code { language }) = code_fence_shortcut(&text)
        {
            self.push_undo_snapshot(block_id)?;
            self.replace_block_kind_and_payload(
                block_id,
                RichBlockKind::Code {
                    language: language.clone(),
                },
                BlockPayload::Code {
                    language,
                    text: String::new(),
                },
            )?;
            return Ok(());
        }
        if cditor_core::block::is_list_item_kind(&kind) && text.trim().is_empty() {
            let depth = self
                .index
                .index_of(block_id)
                .and_then(|index| self.index.depths.get(index).copied())
                .unwrap_or_default();
            if depth == 0 {
                self.replace_block_kind_and_payload(
                    block_id,
                    RichBlockKind::Paragraph,
                    BlockPayload::RichText { spans: Vec::new() },
                )?;
            } else {
                let _ = self.outdent_block(block_id)?;
            }
            return Ok(());
        }
        if matches!(
            kind,
            RichBlockKind::Code { .. }
                | RichBlockKind::Quote
                | RichBlockKind::Callout { .. }
                | RichBlockKind::RawMarkdown
        ) {
            self.insert_soft_line_break()?;
            self.refresh_focused_text_block_height()?;
            Ok(())
        } else {
            self.split_focused_block_at_caret(EnterSplitMode::InheritV1Kind)?;
            Ok(())
        }
    }

    pub fn insert_paragraph_after_focused(&mut self) -> Result<BlockId, String> {
        self.split_focused_block_at_caret(EnterSplitMode::ForceParagraph)
    }

    pub fn focus_or_create_down_placer_paragraph(&mut self) -> Result<bool, String> {
        let Some(last_block_id) = self.visible_index.visible_block_ids.last().copied() else {
            return Ok(false);
        };
        let text_len = self
            .text_models
            .get(&last_block_id)
            .map(PieceTableTextModel::len)
            .or_else(|| {
                self.payload_window
                    .get(last_block_id)
                    .map(BlockPayloadRecord::plain_text)
                    .map(|text| text.len())
            })
            .unwrap_or(0);
        let is_empty_paragraph =
            matches!(self.kind_for_block(last_block_id), RichBlockKind::Paragraph) && text_len == 0;
        if is_empty_paragraph {
            self.focus_block_at_offset(last_block_id, 0)?;
            return Ok(false);
        }

        self.focus_block_at_offset(last_block_id, text_len)?;
        self.insert_paragraph_after_focused()?;
        Ok(true)
    }

    pub(super) fn split_focused_block_at_caret(
        &mut self,
        mode: EnterSplitMode,
    ) -> Result<BlockId, String> {
        let Some(current_block_id) = self.focused_block_id() else {
            let first = self
                .visible_index
                .visible_block_ids
                .first()
                .copied()
                .unwrap_or(1);
            self.focus_block(first);
            return Ok(first);
        };
        let current_index = self
            .index
            .index_of(current_block_id)
            .ok_or_else(|| format!("focused block {current_block_id} is missing from index"))?;
        let current_kind = self
            .payload_window
            .get(current_block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or_else(|| RichBlockKind::Paragraph);
        let new_kind = match mode {
            EnterSplitMode::InheritV1Kind => newline_sibling_kind_for_v1(&current_kind),
            EnterSplitMode::ForceParagraph => RichBlockKind::Paragraph,
        };
        let caret = self
            .editing
            .as_ref()
            .map(|editing| editing.caret_anchor.text_offset as usize)
            .unwrap_or_else(|| self.focused_text().map(str::len).unwrap_or(0));
        let (leading_payload, trailing_payload) = {
            let current_payload = self
                .payload_window
                .get(current_block_id)
                .ok_or_else(|| format!("missing payload for focused block {current_block_id}"))?;
            split_payload_for_enter(&current_payload.payload, caret, &new_kind)
        };

        self.push_undo_snapshot(current_block_id)?;
        let new_block_id = self
            .index
            .block_ids
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let parent_id = self.index.parent_ids[current_index];
        let depth = self.index.depths[current_index];
        let insert_at = self.subtree_end(current_index);
        let content_version = self
            .payload_window
            .get(current_block_id)
            .map(|payload| payload.content_version.saturating_add(1))
            .unwrap_or(2);

        let mut updated_current_payload = None;
        if let Some(payload) = self.payload_window.payloads.get_mut(&current_block_id) {
            payload.content_version = content_version;
            payload.payload = leading_payload;
            updated_current_payload = Some(payload.clone());
        }
        if let Some(mut payload) = updated_current_payload {
            self.sync_table_runtime_from_loaded_record(&mut payload);
            self.payload_window.insert(payload);
        }
        if let Some(editing) = self
            .editing
            .as_mut()
            .filter(|editing| editing.block_id == current_block_id)
        {
            editing.content_version = content_version;
            editing.caret_anchor.text_offset = caret.min(
                self.text_models
                    .get(&current_block_id)
                    .map(PieceTableTextModel::len)
                    .unwrap_or(0),
            ) as u64;
        }

        let new_payload = BlockPayloadRecord {
            block_id: new_block_id,
            content_version: 1,
            kind: new_kind.clone(),
            payload: trailing_payload,
        };
        let record = BlockIndexRecord::new(
            new_block_id,
            parent_id,
            depth,
            kind_tag_for_rich_block_kind(&new_kind),
            0,
        )
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(
            new_block_id,
            estimate_payload_height(&new_payload, insert_at),
        ));
        self.insert_runtime_block(insert_at, record, new_payload)?;
        self.focus_block_at_offset(new_block_id, 0)?;
        Ok(new_block_id)
    }

    pub fn pending_structure_transaction_count(&self) -> usize {
        self.pending_structure_transactions.len()
    }

    pub fn structure_version(&self) -> u64 {
        self.index.structure_version
    }

    pub fn index_records_snapshot(&self) -> Vec<BlockIndexRecord> {
        self.index_records()
    }

    pub fn loaded_payload_records_snapshot(&self) -> Vec<BlockPayloadRecord> {
        self.payload_window
            .payloads
            .values()
            .cloned()
            .map(|payload| self.table_runtime_payload_record(payload.block_id, payload))
            .collect()
    }

    pub fn drain_pending_structure_transactions(&mut self) -> Vec<EditTransaction> {
        self.pending_structure_transactions.drain(..).collect()
    }

    pub fn move_block_subtree_before(
        &mut self,
        block_id: BlockId,
        before_block_id: Option<BlockId>,
    ) -> Result<bool, String> {
        let Some(source_start) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let source_parent = self.index.parent_ids[source_start];
        let source_sibling_index = self.direct_child_position(source_parent, block_id);
        let target_parent = before_block_id
            .and_then(|before_block_id| self.index.index_of(before_block_id))
            .map(|index| self.index.parent_ids[index])
            .unwrap_or(source_parent);
        let sibling_index = match before_block_id {
            Some(before_block_id) => {
                let before_position = self
                    .direct_child_position(target_parent, before_block_id)
                    .unwrap_or_else(|| self.direct_children(target_parent).len());
                if target_parent == source_parent
                    && source_sibling_index.is_some_and(|source| source < before_position)
                {
                    before_position.saturating_sub(1)
                } else {
                    before_position
                }
            }
            None => {
                let len = self.direct_children(target_parent).len();
                if target_parent == source_parent {
                    len.saturating_sub(1)
                } else {
                    len
                }
            }
        };
        self.move_block_subtree_to_parent(block_id, target_parent, sibling_index)
    }

    pub fn move_block_subtree_to_parent(
        &mut self,
        block_id: BlockId,
        new_parent_id: Option<BlockId>,
        sibling_index: usize,
    ) -> Result<bool, String> {
        let Some(source_start) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let old_parent_id = self.index.parent_ids[source_start];
        let Some(old_sibling_index) = self.direct_child_position(old_parent_id, block_id) else {
            return Ok(false);
        };

        if !self.move_block_subtree_to_parent_untracked(block_id, new_parent_id, sibling_index)? {
            return Ok(false);
        }

        let new_sibling_index = self
            .direct_child_position(new_parent_id, block_id)
            .unwrap_or(sibling_index);
        let step = StructureMoveUndoStep {
            block_id,
            old_parent_id,
            old_sibling_index,
            new_parent_id,
            new_sibling_index,
        };
        self.record_structure_move(step);
        self.queue_structure_move_transaction(step, true);
        Ok(true)
    }

    pub(super) fn move_block_subtree_to_parent_untracked(
        &mut self,
        block_id: BlockId,
        new_parent_id: Option<BlockId>,
        sibling_index: usize,
    ) -> Result<bool, String> {
        let restore_focus_offset = (self.focused_block_id() == Some(block_id))
            .then(|| self.caret_offset_for_block(block_id).unwrap_or(0));
        let Some(source_start) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let source_end = self.subtree_end(source_start);
        if let Some(new_parent_id) = new_parent_id {
            let Some(parent_index) = self.index.index_of(new_parent_id) else {
                return Ok(false);
            };
            if (source_start..source_end).contains(&parent_index) {
                return Ok(false);
            }
            let parent_kind = self.kind_at_index(parent_index);
            if !cditor_core::block::supports_list_children(&parent_kind) {
                return Ok(false);
            }
        }

        let old_parent_id = self.index.parent_ids[source_start];
        let old_sibling_index = self.direct_child_position(old_parent_id, block_id);
        if old_parent_id == new_parent_id && old_sibling_index == Some(sibling_index) {
            return Ok(false);
        }

        let mut records = self.index_records();
        let mut moved = records.drain(source_start..source_end).collect::<Vec<_>>();
        let new_parent_depth = new_parent_id
            .and_then(|parent_id| record_index_of(&records, parent_id))
            .map(|index| records[index].depth.saturating_add(1))
            .unwrap_or(0);
        let old_depth = moved[0].depth;
        apply_subtree_depth_delta(&mut moved, old_depth, new_parent_depth);
        moved[0].parent_id = new_parent_id;

        let insertion_index =
            insertion_index_for_parent_sibling(&records, new_parent_id, sibling_index);
        records.splice(insertion_index..insertion_index, moved);
        self.rebuild_structure_index(records)?;
        if let Some(offset) = restore_focus_offset {
            self.focus_block_at_offset(block_id, offset)?;
        }
        Ok(true)
    }

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
        self.push_undo_snapshot(block_id)?;
        let kind = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .unwrap_or(RichBlockKind::Paragraph);
        let spans = spans_with_mark_for_range(&text, range.clone(), mark);
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

    pub(super) fn replace_block_kind_and_spans(
        &mut self,
        block_id: BlockId,
        kind: RichBlockKind,
        spans: Vec<InlineSpan>,
    ) -> Result<(), String> {
        self.replace_block_kind_and_payload(block_id, kind, BlockPayload::RichText { spans })
    }

    pub(super) fn replace_block_kind_and_payload(
        &mut self,
        block_id: BlockId,
        kind: RichBlockKind,
        payload: BlockPayload,
    ) -> Result<(), String> {
        let payload = ensure_table_payload_for_kind(&kind, payload);
        let editable_text = editable_text_for_payload(&payload);
        if let Some(index) = self.index.index_of(block_id) {
            let height_estimate = estimate_block_height(&kind, &payload, DEFAULT_LAYOUT_WIDTH_PX);
            self.index.kind_tags[index] = kind_tag_for_rich_block_kind(&kind);
            self.index.layout_meta[index].estimated_height = height_estimate.height;
            self.index.layout_meta[index].measured_height = None;
            self.index.layout_meta[index].dirty = true;
            self.index.layout_meta[index].layout_version = self.index.layout_meta[index]
                .layout_version
                .saturating_add(1);
            self.pending_measured_heights.remove(&block_id);
            self.layout_dirty = true;
            if let Some(visible_index) = self.visible_index.visible_index_of(block_id) {
                let height_change = self
                    .height_index
                    .update_height(visible_index, height_estimate.height)
                    .map_err(|error| error.to_string())?;
                if let Some(page_index) = self.page_layout.page_for_block_index(visible_index) {
                    let next_page_height =
                        self.page_layout.pages[page_index].height + height_change.delta;
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
            }
            self.list_projection_cache = ListProjectionCache::build(&self.index);
        }
        let content_version = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.content_version.saturating_add(1))
            .unwrap_or(1);
        let mut updated_record = None;
        if let Some(record) = self.payload_window.payloads.get_mut(&block_id) {
            record.kind = kind;
            record.payload = payload;
            record.content_version = content_version;
            updated_record = Some(record.clone());
        }
        if let Some(mut record) = updated_record {
            self.sync_table_runtime_from_loaded_record(&mut record);
            self.payload_window.insert(record);
        }
        if let Some(editing) = self
            .editing
            .as_mut()
            .filter(|editing| editing.block_id == block_id)
        {
            let caret = editable_text.as_deref().map(str::len).unwrap_or(0);
            editing.content_version = content_version;
            editing.caret_anchor.text_offset = caret as u64;
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_collapsed_selection(caret);
        }
        Ok(())
    }

    pub fn delete_document_selection(&mut self) -> Result<bool, String> {
        let Some(selection) = self.document_selection else {
            return Ok(false);
        };
        if selection.is_caret() {
            return Ok(false);
        }
        let normalized = selection
            .normalize(&self.index)
            .map_err(|error| format!("{error:?}"))?;
        if normalized.start.block_id == normalized.end.block_id {
            self.focus_block_at_offset(normalized.start.block_id, normalized.start.offset)?;
            self.focused_text_selection = Some(FocusedTextSelection {
                anchor: normalized.start.offset,
                focus: normalized.end.offset,
            });
            return self.replace_text_in_focused_range(None, "");
        }
        let start_block_id = normalized.start.block_id;
        let before_current_record = self.index_record_for_block(start_block_id)?;
        let before_current_payload = self
            .payload_window
            .get(start_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {start_block_id}"))?;
        let before_focus = self
            .focused_block_id()
            .map(|block_id| (block_id, self.caret_offset_for_block(block_id).unwrap_or(0)));
        let Some((_block_id, deleted_records, deleted_payloads)) =
            self.collapse_cross_block_selection_for_paste()?
        else {
            return Ok(false);
        };
        self.document_selection = None;
        self.focused_text_selection = None;
        let after_current_record = self.index_record_for_block(start_block_id)?;
        let after_current_payload = self
            .payload_window
            .get(start_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {start_block_id}"))?;
        let after_focus = Some((start_block_id, normalized.start.offset));
        self.record_structure_paste(StructurePasteUndoStep {
            current_block_id: start_block_id,
            before_current_record,
            before_current_payload,
            after_current_record,
            after_current_payload,
            inserted_records: Vec::new(),
            inserted_payloads: Vec::new(),
            deleted_records,
            deleted_payloads,
            before_focus,
            after_focus,
        });
        self.queue_delete_selection_transaction(start_block_id);
        Ok(true)
    }
}

fn payload_for_converted_kind(kind: &RichBlockKind, text: String) -> BlockPayload {
    match kind {
        RichBlockKind::Code { language } => BlockPayload::Code {
            language: language.clone(),
            text,
        },
        RichBlockKind::Html => BlockPayload::Html {
            html: text,
            sanitized: false,
        },
        RichBlockKind::Table => default_table_payload(text),
        RichBlockKind::Divider | RichBlockKind::Separator => BlockPayload::Empty,
        RichBlockKind::Image
        | RichBlockKind::File
        | RichBlockKind::Attachment
        | RichBlockKind::Whiteboard
        | RichBlockKind::MindMap
        | RichBlockKind::Embed
        | RichBlockKind::Database => BlockPayload::Empty,
        _ => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(text)],
        },
    }
}
