use super::*;

impl DocumentRuntime {
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
        if crate::core::block::is_list_item_kind(&kind) && text.trim().is_empty() {
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

        if let Some(payload) = self.payload_window.payloads.get_mut(&current_block_id) {
            payload.content_version = content_version;
            payload.payload = leading_payload;
        }
        self.text_models.insert(
            current_block_id,
            PieceTableTextModel::new(
                self.payload_window
                    .get(current_block_id)
                    .map(BlockPayloadRecord::plain_text)
                    .unwrap_or_default(),
            ),
        );
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
        .with_layout_meta(crate::core::layout::BlockLayoutMeta::new(
            new_block_id,
            estimate_payload_height(&new_payload, insert_at),
        ));
        self.insert_runtime_block(insert_at, record, new_payload)?;
        self.focus_block_at_offset(new_block_id, 0)?;
        Ok(new_block_id)
    }

    pub fn indent_focused_block(&mut self) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let kind = self.kind_for_block(block_id);
        if uses_soft_tab(&kind) {
            return self.insert_soft_tab_in_focused_block();
        }
        self.indent_block(block_id)
    }

    pub fn outdent_focused_block(&mut self) -> Result<bool, String> {
        let Some(block_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let kind = self.kind_for_block(block_id);
        if uses_soft_tab(&kind) {
            return self.outdent_soft_tab_in_focused_block();
        }
        self.outdent_block(block_id)
    }

    pub fn indent_block(&mut self, block_id: BlockId) -> Result<bool, String> {
        let Some(index) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let parent_id = self.index.parent_ids[index];
        let Some(sibling_index) = self.direct_child_position(parent_id, block_id) else {
            return Ok(false);
        };
        if sibling_index == 0 {
            return Ok(false);
        }
        let siblings = self.direct_children(parent_id);
        let Some(previous_sibling_id) = siblings.get(sibling_index - 1).copied() else {
            return Ok(false);
        };
        let Some(previous_sibling_index) = self.index.index_of(previous_sibling_id) else {
            return Ok(false);
        };
        let previous_kind = self.kind_at_index(previous_sibling_index);
        if !crate::core::block::supports_list_children(&previous_kind) {
            return Ok(false);
        }
        let child_count = self.direct_children(Some(previous_sibling_id)).len();
        self.move_block_subtree_to_parent(block_id, Some(previous_sibling_id), child_count)
    }

    pub fn outdent_block(&mut self, block_id: BlockId) -> Result<bool, String> {
        let Some(index) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        let Some(parent_id) = self.index.parent_ids[index] else {
            return Ok(false);
        };
        let Some(parent_index) = self.index.index_of(parent_id) else {
            return Ok(false);
        };
        let grandparent_id = self.index.parent_ids[parent_index];
        let Some(parent_sibling_index) = self.direct_child_position(grandparent_id, parent_id)
        else {
            return Ok(false);
        };
        self.move_block_subtree_to_parent(block_id, grandparent_id, parent_sibling_index + 1)
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
        self.payload_window.payloads.values().cloned().collect()
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
            if !crate::core::block::supports_list_children(&parent_kind) {
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
        if self.focused_block_id() == Some(block_id) {
            self.focus_block(block_id);
        }
        Ok(true)
    }

    pub fn merge_focused_block_into_previous(&mut self) -> Result<bool, String> {
        let Some(current_id) = self.focused_block_id() else {
            return Ok(false);
        };
        let Some(previous_id) = self.adjacent_visible_block_id(current_id, -1) else {
            return Ok(false);
        };
        self.merge_block_into_previous(current_id, previous_id)
    }

    pub(super) fn merge_block_into_previous(
        &mut self,
        current_id: BlockId,
        previous_id: BlockId,
    ) -> Result<bool, String> {
        let Some(current_index) = self.index.index_of(current_id) else {
            return Ok(false);
        };
        if self.subtree_end(current_index) > current_index + 1 {
            return Ok(false);
        }
        let previous_text_len = self
            .text_models
            .get(&previous_id)
            .map(PieceTableTextModel::len)
            .unwrap_or(0);
        let current_text = self
            .text_models
            .get(&current_id)
            .ok_or_else(|| format!("missing text model for block {current_id}"))?
            .text()
            .to_owned();
        let before_previous_record = self.index_record_for_block(previous_id)?;
        let before_previous_payload = self
            .payload_window
            .get(previous_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {previous_id}"))?;
        let current_record = self.index_record_for_block(current_id)?;
        let current_payload = self
            .payload_window
            .get(current_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {current_id}"))?;
        let before_focus = self
            .focused_block_id()
            .map(|block_id| (block_id, self.caret_offset_for_block(block_id).unwrap_or(0)));

        let previous_kind = before_previous_payload.kind.clone();
        let previous_payload =
            append_plain_text_to_payload(before_previous_payload.payload.clone(), current_text);
        self.replace_block_kind_and_payload(previous_id, previous_kind, previous_payload)?;
        let after_previous_record = self.index_record_for_block(previous_id)?;
        let after_previous_payload = self
            .payload_window
            .get(previous_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {previous_id}"))?;

        let mut records = self.index_records();
        records.retain(|record| record.id != current_id);
        self.payload_window.payloads.remove(&current_id);
        self.text_models.remove(&current_id);
        self.rebuild_structure_index(records)?;
        self.focus_block_at_offset(previous_id, previous_text_len)?;
        self.document_selection = Some(DocumentSelection {
            anchor: TextPosition::downstream(previous_id, previous_text_len),
            focus: TextPosition::downstream(
                previous_id,
                previous_text_len + current_payload.plain_text().len(),
            ),
        });
        self.focused_text_selection = Some(FocusedTextSelection {
            anchor: previous_text_len,
            focus: previous_text_len + current_payload.plain_text().len(),
        });
        self.record_structure_paste(StructurePasteUndoStep {
            current_block_id: previous_id,
            before_current_record: before_previous_record,
            before_current_payload: before_previous_payload,
            after_current_record: after_previous_record,
            after_current_payload: after_previous_payload,
            inserted_records: Vec::new(),
            inserted_payloads: Vec::new(),
            deleted_records: vec![current_record],
            deleted_payloads: vec![current_payload],
            before_focus,
            after_focus: Some((previous_id, previous_text_len)),
        });
        self.queue_merge_delete_transaction(previous_id, current_id, true);
        Ok(true)
    }

    pub fn delete_focused_empty_block_backward(&mut self) -> Result<bool, String> {
        self.delete_focused_empty_block(-1)
    }

    pub fn delete_focused_empty_block_forward(&mut self) -> Result<bool, String> {
        self.delete_focused_empty_block(1)
    }

    pub(super) fn delete_focused_empty_block(
        &mut self,
        preferred_direction: i32,
    ) -> Result<bool, String> {
        let Some(current_id) = self.focused_block_id() else {
            return Ok(false);
        };
        if self
            .text_models
            .get(&current_id)
            .map(|model| !model.text().is_empty())
            .unwrap_or(true)
        {
            return Ok(false);
        }
        if self.visible_index.total_visible_count() <= 1 {
            self.replace_block_kind_and_payload(
                current_id,
                RichBlockKind::Paragraph,
                BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("")],
                },
            )?;
            self.focus_block_at_offset(current_id, 0)?;
            return Ok(false);
        }
        let Some(current_index) = self.index.index_of(current_id) else {
            return Ok(false);
        };
        if self.subtree_end(current_index) > current_index + 1 {
            return Ok(false);
        }
        let target_id = self
            .adjacent_visible_block_id(current_id, preferred_direction)
            .or_else(|| self.adjacent_visible_block_id(current_id, -preferred_direction))
            .ok_or_else(|| "missing adjacent block for empty block delete".to_owned())?;
        let target_offset = if preferred_direction < 0 {
            self.text_models
                .get(&target_id)
                .map(PieceTableTextModel::len)
                .unwrap_or(0)
        } else {
            0
        };
        let before_target_record = self.index_record_for_block(target_id)?;
        let before_target_payload = self
            .payload_window
            .get(target_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {target_id}"))?;
        let deleted_record = self.index_record_for_block(current_id)?;
        let deleted_payload = self
            .payload_window
            .get(current_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {current_id}"))?;
        let before_focus = Some((current_id, 0));
        let mut records = self.index_records();
        records.retain(|record| record.id != current_id);
        self.payload_window.payloads.remove(&current_id);
        self.text_models.remove(&current_id);
        self.rebuild_structure_index(records)?;
        self.focus_block_at_offset(target_id, target_offset)?;
        let after_target_record = self.index_record_for_block(target_id)?;
        let after_target_payload = self
            .payload_window
            .get(target_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {target_id}"))?;
        self.record_structure_paste(StructurePasteUndoStep {
            current_block_id: target_id,
            before_current_record: before_target_record,
            before_current_payload: before_target_payload,
            after_current_record: after_target_record,
            after_current_payload: after_target_payload,
            inserted_records: Vec::new(),
            inserted_payloads: Vec::new(),
            deleted_records: vec![deleted_record],
            deleted_payloads: vec![deleted_payload],
            before_focus,
            after_focus: Some((target_id, target_offset)),
        });
        self.queue_merge_delete_transaction(target_id, current_id, false);
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
        if let Some(editing) = self.editing.as_mut() {
            editing.caret_anchor.text_offset = range.end as u64;
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
        let plain_text = payload.plain_text();
        if let Some(index) = self.index.index_of(block_id) {
            self.index.kind_tags[index] = kind_tag_for_rich_block_kind(&kind);
            self.index.layout_meta[index].estimated_height =
                estimate_block_height(&kind, &payload, DEFAULT_LAYOUT_WIDTH_PX).height;
            self.list_projection_cache = ListProjectionCache::build(&self.index);
        }
        {
            let model = self
                .text_models
                .get_mut(&block_id)
                .ok_or_else(|| format!("missing text model for block {block_id}"))?;
            model
                .replace_range(0..model.len(), &plain_text)
                .map_err(|error| format!("{error:?}"))?;
        }
        let content_version = self
            .payload_window
            .get(block_id)
            .map(|payload| payload.content_version.saturating_add(1))
            .unwrap_or(1);
        if let Some(record) = self.payload_window.payloads.get_mut(&block_id) {
            record.kind = kind;
            record.payload = payload;
            record.content_version = content_version;
        }
        if let Some(editing) = self.editing.as_mut() {
            editing.content_version = content_version;
            editing.caret_anchor.text_offset = plain_text.len() as u64;
        }
        Ok(())
    }

    pub(super) fn kind_for_block(&self, block_id: BlockId) -> RichBlockKind {
        self.payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .or_else(|| {
                self.index
                    .index_of(block_id)
                    .map(|index| rich_block_kind_from_tag(self.index.kind_tags[index]))
            })
            .unwrap_or_else(|| RichBlockKind::Paragraph)
    }

    pub(super) fn kind_at_index(&self, index: usize) -> RichBlockKind {
        self.index
            .block_ids
            .get(index)
            .and_then(|block_id| self.payload_window.get(*block_id))
            .map(|payload| payload.kind.clone())
            .unwrap_or_else(|| rich_block_kind_from_tag(self.index.kind_tags[index]))
    }

    pub(super) fn subtree_end(&self, index: usize) -> usize {
        let depth = self.index.depths[index];
        let mut end = index + 1;
        while end < self.index.block_ids.len() && self.index.depths[end] > depth {
            end += 1;
        }
        end
    }

    pub(super) fn direct_children(&self, parent_id: Option<BlockId>) -> Vec<BlockId> {
        self.index
            .block_ids
            .iter()
            .enumerate()
            .filter_map(|(index, block_id)| {
                (self.index.parent_ids[index] == parent_id).then_some(*block_id)
            })
            .collect()
    }

    pub(super) fn direct_child_position(
        &self,
        parent_id: Option<BlockId>,
        block_id: BlockId,
    ) -> Option<usize> {
        self.direct_children(parent_id)
            .iter()
            .position(|candidate| *candidate == block_id)
    }

    pub(super) fn index_record_for_block(
        &self,
        block_id: BlockId,
    ) -> Result<BlockIndexRecord, String> {
        let index = self
            .index
            .index_of(block_id)
            .ok_or_else(|| format!("missing block {block_id} in index"))?;
        Ok(BlockIndexRecord::new(
            block_id,
            self.index.parent_ids[index],
            self.index.depths[index],
            self.index.kind_tags[index],
            self.index.flags[index],
        )
        .with_layout_meta(self.index.layout_meta[index]))
    }

    pub(super) fn index_records(&self) -> Vec<BlockIndexRecord> {
        self.index
            .block_ids
            .iter()
            .enumerate()
            .map(|(index, block_id)| {
                BlockIndexRecord::new(
                    *block_id,
                    self.index.parent_ids[index],
                    self.index.depths[index],
                    self.index.kind_tags[index],
                    self.index.flags[index],
                )
                .with_layout_meta(self.index.layout_meta[index])
            })
            .collect()
    }

    pub(super) fn rebuild_structure_index(
        &mut self,
        records: Vec<BlockIndexRecord>,
    ) -> Result<(), String> {
        self.index = DocumentIndex::new(
            self.document_id,
            records,
            self.index.structure_version.saturating_add(1),
        )
        .map_err(|error| error.to_string())?;
        self.visible_index = VisibleDocumentIndex::from_document_index(&self.index);
        self.list_projection_cache = ListProjectionCache::build(&self.index);
        self.payload_window.block_range = 0..self.visible_index.total_visible_count();
        self.rebuild_height_indexes_from_layout_meta()?;
        self.selected_block_ids.clear();
        self.last_successful_projection = None;
        Ok(())
    }

    pub(super) fn rebuild_height_indexes_from_layout_meta(&mut self) -> Result<(), String> {
        let height_estimates = self
            .index
            .layout_meta
            .iter()
            .map(|meta| {
                HeightEstimate::new(meta.effective_height(), HeightConfidence::Historical, 4.0)
            })
            .collect::<Vec<_>>();
        self.height_index =
            BlockHeightIndex::new(height_estimates).map_err(|error| error.to_string())?;
        self.page_layout =
            PageLayoutIndex::from_block_height_index(&self.height_index, PagePolicy::default())
                .map_err(|error| error.to_string())?;
        let total_height = self.height_index.total_height();
        self.scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        self.scroll
            .set_displayed_total_height(total_height)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(super) fn next_available_block_id(&self) -> BlockId {
        self.index
            .block_ids
            .iter()
            .copied()
            .chain(self.payload_window.payloads.keys().copied())
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    pub(super) fn replace_existing_block_from_record(
        &mut self,
        block_id: BlockId,
        block: RichBlockRecord,
    ) -> Result<(), String> {
        let payload = block.to_payload_record();
        self.replace_block_kind_and_payload(block_id, block.kind, block.payload)?;
        if let Some(record) = self.payload_window.payloads.get_mut(&block_id) {
            record.content_version = payload.content_version;
        }
        Ok(())
    }

    pub(super) fn replace_text_in_block_with_plain(
        &mut self,
        block_id: BlockId,
        text: String,
    ) -> Result<(), String> {
        let Some(payload) = self.payload_window.payloads.get(&block_id) else {
            return Err(format!("missing payload for block {block_id}"));
        };
        let kind = payload.kind.clone();
        self.replace_block_kind_and_payload(
            block_id,
            kind.clone(),
            payload_for_kind_from_plain_text(&kind, text),
        )
    }

    pub(super) fn insert_runtime_block(
        &mut self,
        insert_at: usize,
        record: BlockIndexRecord,
        payload: BlockPayloadRecord,
    ) -> Result<(), String> {
        let mut records = self
            .index
            .block_ids
            .iter()
            .enumerate()
            .map(|(index, block_id)| {
                BlockIndexRecord::new(
                    *block_id,
                    self.index.parent_ids[index],
                    self.index.depths[index],
                    self.index.kind_tags[index],
                    self.index.flags[index],
                )
                .with_layout_meta(self.index.layout_meta[index])
            })
            .collect::<Vec<_>>();
        let insert_at = insert_at.min(records.len());
        records.insert(insert_at, record);

        self.payload_window.insert(payload.clone());
        self.text_models.insert(
            payload.block_id,
            PieceTableTextModel::new(payload.plain_text()),
        );
        self.index = DocumentIndex::new(
            self.document_id,
            records,
            self.index.structure_version.saturating_add(1),
        )
        .map_err(|error| error.to_string())?;
        self.visible_index = VisibleDocumentIndex::from_document_index(&self.index);
        self.list_projection_cache = ListProjectionCache::build(&self.index);
        self.payload_window.block_range = 0..self.visible_index.total_visible_count();
        let height_estimates = self
            .index
            .layout_meta
            .iter()
            .map(|meta| {
                HeightEstimate::new(meta.effective_height(), HeightConfidence::Historical, 4.0)
            })
            .collect::<Vec<_>>();
        self.height_index =
            BlockHeightIndex::new(height_estimates).map_err(|error| error.to_string())?;
        self.page_layout =
            PageLayoutIndex::from_block_height_index(&self.height_index, PagePolicy::default())
                .map_err(|error| error.to_string())?;
        let total_height = self.height_index.total_height();
        self.scroll
            .set_model_total_height(total_height)
            .map_err(|error| error.to_string())?;
        self.scroll
            .set_displayed_total_height(total_height)
            .map_err(|error| error.to_string())?;
        self.selected_block_ids.clear();
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

    pub(super) fn queue_merge_delete_transaction(
        &mut self,
        survivor_block_id: BlockId,
        deleted_block_id: BlockId,
        merge: bool,
    ) {
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        let operation = if merge {
            EditOperation::MergeBlocks {
                previous: survivor_block_id,
                current: deleted_block_id,
            }
        } else {
            EditOperation::DeleteBlock {
                block_id: deleted_block_id,
            }
        };
        self.pending_structure_transactions
            .push(EditTransaction::new(
                transaction_id,
                EditTransactionKind::BlockStructureChange,
                transaction_id,
                vec![operation],
                vec![EditOperation::InsertBlock {
                    index: self.index.index_of(survivor_block_id).unwrap_or(0),
                    block: self
                        .index_record_for_block(survivor_block_id)
                        .unwrap_or_else(|_| {
                            BlockIndexRecord::new(survivor_block_id, None, 0, 0, 0)
                        }),
                }],
            ));
    }

    pub(super) fn queue_delete_selection_transaction(&mut self, survivor_block_id: BlockId) {
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        self.pending_structure_transactions
            .push(EditTransaction::new(
                transaction_id,
                EditTransactionKind::BlockStructureChange,
                transaction_id,
                vec![EditOperation::DeleteBlockRange { range: 0..0 }],
                vec![EditOperation::InsertBlock {
                    index: self.index.index_of(survivor_block_id).unwrap_or(0),
                    block: self
                        .index_record_for_block(survivor_block_id)
                        .unwrap_or_else(|_| {
                            BlockIndexRecord::new(survivor_block_id, None, 0, 0, 0)
                        }),
                }],
            ));
    }
}
