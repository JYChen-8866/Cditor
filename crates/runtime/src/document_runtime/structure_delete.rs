use super::*;

impl DocumentRuntime {
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
        self.payload_window.remove(current_id);
        self.text_models.remove(&current_id);
        self.table_runtimes.remove(&current_id);
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
        self.payload_window.remove(current_id);
        self.text_models.remove(&current_id);
        self.table_runtimes.remove(&current_id);
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

    /// Delete any block by ID, moving focus to an adjacent block.
    pub fn delete_block_by_id(&mut self, block_id: BlockId) -> Result<bool, String> {
        if self.visible_index.total_visible_count() <= 1 {
            // Last block — reset to empty paragraph instead of deleting.
            self.replace_block_kind_and_payload(
                block_id,
                RichBlockKind::Paragraph,
                BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("")],
                },
            )?;
            self.focus_block_at_offset(block_id, 0)?;
            return Ok(true);
        }
        let Some(current_index) = self.index.index_of(block_id) else {
            return Ok(false);
        };
        // Don't delete blocks that have children (subtree).
        if self.subtree_end(current_index) > current_index + 1 {
            return Ok(false);
        }
        let target_id = self
            .adjacent_visible_block_id(block_id, -1)
            .or_else(|| self.adjacent_visible_block_id(block_id, 1))
            .ok_or_else(|| "no adjacent block for delete".to_owned())?;
        let target_offset = self
            .text_models
            .get(&target_id)
            .map(PieceTableTextModel::len)
            .unwrap_or(0);
        let before_target_record = self.index_record_for_block(target_id)?;
        let before_target_payload = self
            .payload_window
            .get(target_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {target_id}"))?;
        let deleted_record = self.index_record_for_block(block_id)?;
        let deleted_payload = self
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let before_focus = Some((block_id, 0));
        let mut records = self.index_records();
        records.retain(|record| record.id != block_id);
        self.payload_window.remove(block_id);
        self.text_models.remove(&block_id);
        self.table_runtimes.remove(&block_id);
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
        self.queue_merge_delete_transaction(target_id, block_id, false);
        Ok(true)
    }
}
