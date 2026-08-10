//! Block 插入与 Enter split（从 structure_edit.rs 拆出，保持 700 行上限）。
//!
//! 全部创建 block 的路径都必须记录结构 undo step（P4-009）：undo 移除新块
//! 并恢复原块，redo 按记录的 preorder 位置精确还原。

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnterSplitMode {
    InheritV1Kind,
    #[cfg_attr(not(test), allow(dead_code))]
    ForceParagraph,
}

impl DocumentRuntime {
    pub(crate) fn insert_paragraph_after_focused(&mut self) -> Result<BlockId, String> {
        let block_id = self
            .focused_block_id()
            .or_else(|| {
                self.document
                    .visible_index
                    .visible_block_ids
                    .last()
                    .copied()
            })
            .ok_or_else(|| "cannot insert a paragraph into an empty document".to_owned())?;
        self.insert_paragraph_after_block(block_id)
    }

    pub fn insert_paragraph_after_block(&mut self, block_id: BlockId) -> Result<BlockId, String> {
        let before_selection = self.document_selection_snapshot();
        let current_index = self
            .document
            .index
            .index_of(block_id)
            .ok_or_else(|| format!("block {block_id} is missing from index"))?;
        let new_block_id = self
            .document
            .index
            .block_ids
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let parent_id = self.document.index.parent_ids[current_index];
        let depth = self.document.index.depths[current_index];
        let insert_at = self.subtree_end(current_index);
        let mut payload =
            BlockPayloadRecord::rich_text(new_block_id, RichBlockKind::Paragraph, String::new());
        payload.content_version = 0;
        let record = BlockIndexRecord::new(
            new_block_id,
            parent_id,
            depth,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            0,
        )
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(
            new_block_id,
            estimate_payload_height(&payload, insert_at),
        ));
        self.apply_local_insert_blocks_transaction(LocalInsertBlocksTransaction {
            index: insert_at,
            blocks: vec![record],
            payloads: vec![payload],
            kind: EditTransactionKind::BlockStructureChange,
            origin: cditor_core::edit::ChangeOrigin::User,
            before_selection,
            after_selection: Some(DocumentSelection::caret(TextPosition {
                block_id: new_block_id,
                offset: 0,
                affinity: TextAffinity::Downstream,
            })),
        })?;
        self.focus_block_at_offset(new_block_id, 0)?;
        Ok(new_block_id)
    }

    pub(super) fn insert_heading_after_folded_section(
        &mut self,
        block_id: BlockId,
        level: u8,
    ) -> Result<BlockId, String> {
        let current_index = self
            .document
            .index
            .index_of(block_id)
            .ok_or_else(|| format!("block {block_id} is missing from index"))?;
        let insert_at = self
            .document
            .visible_index
            .fold_end_index(&self.document.index, block_id)
            .unwrap_or_else(|| current_index.saturating_add(1));
        let new_block_id = self
            .document
            .index
            .block_ids
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let kind = RichBlockKind::Heading {
            level: level.clamp(1, 6),
        };
        let mut payload = BlockPayloadRecord::rich_text(new_block_id, kind.clone(), String::new());
        payload.content_version = 0;
        let record = BlockIndexRecord::new(
            new_block_id,
            self.document.index.parent_ids[current_index],
            self.document.index.depths[current_index],
            kind_tag_for_rich_block_kind(&kind),
            0,
        )
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(
            new_block_id,
            estimate_payload_height(&payload, insert_at),
        ));

        self.apply_local_insert_blocks_transaction(LocalInsertBlocksTransaction {
            index: insert_at,
            blocks: vec![record],
            payloads: vec![payload],
            kind: EditTransactionKind::BlockStructureChange,
            origin: cditor_core::edit::ChangeOrigin::User,
            before_selection: self.document_selection_snapshot(),
            after_selection: Some(DocumentSelection::caret(TextPosition {
                block_id: new_block_id,
                offset: 0,
                affinity: TextAffinity::Downstream,
            })),
        })?;
        self.focus_block_at_offset(new_block_id, 0)?;
        Ok(new_block_id)
    }

    pub(crate) fn focus_or_create_down_placer_paragraph(&mut self) -> Result<bool, String> {
        let Some(last_block_id) = self
            .document
            .visible_index
            .visible_block_ids
            .last()
            .copied()
        else {
            return Ok(false);
        };
        let text_len = self
            .document
            .text_models
            .get(&last_block_id)
            .map(PieceTableTextModel::len)
            .or_else(|| {
                self.document
                    .payload_window
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
        let before_selection = self.document_selection_snapshot();
        let Some(current_block_id) = self.focused_block_id() else {
            let first = self
                .document
                .visible_index
                .visible_block_ids
                .first()
                .copied()
                .unwrap_or(1);
            self.focus_block(first);
            return Ok(first);
        };
        let current_index = self
            .document
            .index
            .index_of(current_block_id)
            .ok_or_else(|| format!("focused block {current_block_id} is missing from index"))?;
        let current_kind = self
            .document
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
            .session
            .as_ref()
            .map(EditingSession::focus_offset)
            .unwrap_or_else(|| self.focused_text().map(str::len).unwrap_or(0));
        let (leading_payload, trailing_payload) = {
            let current_payload = self
                .document
                .payload_window
                .get(current_block_id)
                .ok_or_else(|| format!("missing payload for focused block {current_block_id}"))?;
            split_payload_for_enter(&current_payload.payload, caret, &new_kind)?
        };

        let before_current_payload = self
            .document
            .payload_window
            .get(current_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for focused block {current_block_id}"))?;
        let before_content_version = before_current_payload.content_version;
        let new_block_id = self
            .document
            .index
            .block_ids
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let parent_id = self.document.index.parent_ids[current_index];
        let depth = self.document.index.depths[current_index];
        let insert_at = self.subtree_end(current_index);
        let new_payload = BlockPayloadRecord {
            block_id: new_block_id,
            content_version: 0,
            kind: new_kind.clone(),
            payload: trailing_payload,
        };
        // Estimate the trailing block from the source block's real measured
        // height when available; the synthetic-width estimate mispredicts the
        // wrap count of soft-wrapped text and makes following blocks jump.
        let trailing_height_estimate = self.document.index.layout_meta[current_index]
            .measured_height
            .filter(|_| new_kind == before_current_payload.kind)
            .and_then(|measured| {
                let total_len = editable_text_len_for_payload(&before_current_payload.payload)?;
                let trailing_len = editable_text_len_for_payload(&new_payload.payload)?;
                split_height::scaled_split_height_estimate(
                    &new_kind,
                    measured,
                    total_len,
                    trailing_len,
                )
            })
            .unwrap_or_else(|| estimate_payload_height(&new_payload, insert_at));
        let record = BlockIndexRecord::new(
            new_block_id,
            parent_id,
            depth,
            kind_tag_for_rich_block_kind(&new_kind),
            0,
        )
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(
            new_block_id,
            trailing_height_estimate,
        ));
        let after_selection = Some(DocumentSelection::caret(TextPosition {
            block_id: new_block_id,
            offset: 0,
            affinity: TextAffinity::Downstream,
        }));
        let after_current_kind = before_current_payload.kind.clone();
        let ops = vec![
            EditOperation::Block(cditor_core::edit::BlockEditOperation::ReplacePayload {
                block_id: current_block_id,
                before_kind: before_current_payload.kind.clone(),
                before_payload: before_current_payload.payload.clone(),
                after_kind: after_current_kind.clone(),
                after_payload: leading_payload.clone(),
            }),
            EditOperation::InsertBlocks {
                index: insert_at,
                blocks: vec![record],
                payloads: vec![new_payload],
            },
        ];
        let inverse_ops = vec![
            EditOperation::DeleteBlockRange {
                range: insert_at..insert_at + 1,
            },
            EditOperation::Block(cditor_core::edit::BlockEditOperation::ReplacePayload {
                block_id: current_block_id,
                before_kind: after_current_kind,
                before_payload: leading_payload,
                after_kind: before_current_payload.kind,
                after_payload: before_current_payload.payload,
            }),
        ];
        self.apply_local_structure_transaction(
            EditTransactionKind::BlockStructureChange,
            cditor_core::edit::ChangeOrigin::User,
            ops,
            inverse_ops,
            vec![
                TransactionPrecondition::StructureVersion(self.structure_version()),
                TransactionPrecondition::BlockContentVersion {
                    block_id: current_block_id,
                    version: before_content_version,
                },
                TransactionPrecondition::BlockAbsent(new_block_id),
            ],
            before_selection,
            after_selection,
            self.selected_block_ids_snapshot(),
            Vec::new(),
        )?;
        self.focus_block_at_offset(new_block_id, 0)?;
        Ok(new_block_id)
    }
}
