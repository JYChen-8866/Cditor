use super::markdown_transaction::{
    append_spans_to_payload, editable_payload_spans, payload_replace_operation,
};
use super::*;

impl DocumentRuntime {
    pub(crate) fn merge_focused_block_into_previous(&mut self) -> Result<bool, String> {
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
        let Some(current_index) = self.document.index.index_of(current_id) else {
            return Ok(false);
        };
        if self.subtree_end(current_index) > current_index + 1 {
            return Ok(false);
        }
        let before_selection = self.document_selection_snapshot();
        let before_selected_blocks = self.selected_block_ids_snapshot();
        let before_previous = self
            .document
            .payload_window
            .get(previous_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {previous_id}"))?;
        let current_record = self.index_record_for_block(current_id)?;
        let current_payload = self
            .document
            .payload_window
            .get(current_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {current_id}"))?;
        let previous_text_len = before_previous.plain_text().len();
        let current_text_len = current_payload.plain_text().len();
        let after_previous_payload = append_spans_to_payload(
            before_previous.payload.clone(),
            editable_payload_spans(&current_payload.payload)?,
        );
        let after_selection = Some(DocumentSelection {
            anchor: TextPosition::downstream(previous_id, previous_text_len),
            focus: TextPosition::downstream(previous_id, previous_text_len + current_text_len),
        });
        let ops = vec![
            payload_replace_operation(
                previous_id,
                before_previous.kind.clone(),
                before_previous.payload.clone(),
                before_previous.kind.clone(),
                after_previous_payload.clone(),
            ),
            EditOperation::DeleteBlockRange {
                range: current_index..current_index + 1,
            },
        ];
        let inverse_ops = vec![
            EditOperation::InsertBlocks {
                index: current_index,
                blocks: vec![current_record],
                payloads: vec![current_payload],
            },
            payload_replace_operation(
                previous_id,
                before_previous.kind.clone(),
                after_previous_payload,
                before_previous.kind.clone(),
                before_previous.payload,
            ),
        ];
        self.cancel_composition();
        self.apply_local_structure_transaction(
            EditTransactionKind::BlockStructureChange,
            cditor_core::edit::ChangeOrigin::User,
            ops,
            inverse_ops,
            vec![
                TransactionPrecondition::StructureVersion(self.structure_version()),
                TransactionPrecondition::BlockContentVersion {
                    block_id: previous_id,
                    version: before_previous.content_version,
                },
            ],
            before_selection,
            after_selection,
            before_selected_blocks,
            Vec::new(),
        )?;
        self.focus_block_at_offset(previous_id, previous_text_len)?;
        self.selection.document_selection = after_selection;
        self.selection.focused_text_selection = Some(FocusedTextSelection {
            anchor: previous_text_len,
            focus: previous_text_len + current_text_len,
        });
        Ok(true)
    }

    pub(crate) fn delete_focused_empty_block_backward(&mut self) -> Result<bool, String> {
        self.delete_focused_empty_block(-1)
    }

    pub(crate) fn delete_focused_empty_block_forward(&mut self) -> Result<bool, String> {
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
            .document
            .text_models
            .get(&current_id)
            .map(|model| !model.text().is_empty())
            .unwrap_or(true)
        {
            return Ok(false);
        }
        if self.document.visible_index.total_visible_count() <= 1 {
            self.reset_last_block_to_empty_text_block(current_id)?;
            return Ok(false);
        }
        let Some(current_index) = self.document.index.index_of(current_id) else {
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
            self.document
                .payload_window
                .get(target_id)
                .map(BlockPayloadRecord::plain_text)
                .map(|text| text.len())
                .unwrap_or(0)
        } else {
            0
        };
        self.delete_leaf_block_transaction(current_id, target_id, target_offset)
    }

    /// Delete any block by ID, moving focus to an adjacent block.
    pub fn delete_block_by_id(&mut self, block_id: BlockId) -> Result<bool, String> {
        if self.document.visible_index.total_visible_count() <= 1 {
            self.reset_last_block_to_empty_text_block(block_id)?;
            return Ok(true);
        }
        let Some(current_index) = self.document.index.index_of(block_id) else {
            return Ok(false);
        };
        if self.subtree_end(current_index) > current_index + 1 {
            return Ok(false);
        }
        let target_id = self
            .adjacent_visible_block_id(block_id, -1)
            .or_else(|| self.adjacent_visible_block_id(block_id, 1))
            .ok_or_else(|| "no adjacent block for delete".to_owned())?;
        let target_offset = self
            .document
            .payload_window
            .get(target_id)
            .map(BlockPayloadRecord::plain_text)
            .map(|text| text.len())
            .unwrap_or(0);
        self.delete_leaf_block_transaction(block_id, target_id, target_offset)
    }

    fn reset_last_block_to_empty_text_block(&mut self, block_id: BlockId) -> Result<(), String> {
        let kind = if self.document.index.block_ids.first() == Some(&block_id)
            && matches!(
                self.block_kind(block_id),
                Some(RichBlockKind::Heading { level: 1 })
            ) {
            RichBlockKind::Heading { level: 1 }
        } else {
            RichBlockKind::Paragraph
        };
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::BlockStructureChange,
            kind,
            BlockPayload::RichText {
                spans: vec![InlineSpan::plain("")],
            },
        )?;
        self.focus_block_at_offset(block_id, 0)
    }

    fn delete_leaf_block_transaction(
        &mut self,
        block_id: BlockId,
        target_id: BlockId,
        target_offset: usize,
    ) -> Result<bool, String> {
        let index = self
            .document
            .index
            .index_of(block_id)
            .ok_or_else(|| format!("missing block {block_id}"))?;
        let deleted_record = self.index_record_for_block(block_id)?;
        let deleted_payload = self
            .document
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let before_selection = self.document_selection_snapshot();
        let before_selected_blocks = self.selected_block_ids_snapshot();
        let after_selection = Some(DocumentSelection::caret(TextPosition::downstream(
            target_id,
            target_offset,
        )));
        self.cancel_composition();
        self.apply_local_structure_transaction(
            EditTransactionKind::BlockStructureChange,
            cditor_core::edit::ChangeOrigin::User,
            vec![EditOperation::DeleteBlockRange {
                range: index..index + 1,
            }],
            vec![EditOperation::InsertBlocks {
                index,
                blocks: vec![deleted_record],
                payloads: vec![deleted_payload],
            }],
            vec![TransactionPrecondition::StructureVersion(
                self.structure_version(),
            )],
            before_selection,
            after_selection,
            before_selected_blocks,
            Vec::new(),
        )?;
        self.focus_block_at_offset(target_id, target_offset)?;
        Ok(true)
    }
}
