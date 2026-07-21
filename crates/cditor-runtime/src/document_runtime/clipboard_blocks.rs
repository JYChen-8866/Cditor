//! Payload-carrying whole-block clipboard transactions.

use super::*;

impl DocumentRuntime {
    pub(super) fn paste_clipboard_blocks(
        &mut self,
        blocks: &[ClipboardBlock],
    ) -> Result<bool, String> {
        if blocks.is_empty() {
            return Ok(false);
        }
        let before_selection = self.document_selection_snapshot();
        let anchor_id = self
            .selected_block_ids
            .iter()
            .filter_map(|block_id| {
                self.index
                    .index_of(*block_id)
                    .map(|index| (index, *block_id))
            })
            .max_by_key(|pair| pair.0)
            .map(|pair| pair.1)
            .or_else(|| self.focused_block_id())
            .or_else(|| self.index.block_ids.last().copied())
            .ok_or_else(|| "document has no paste anchor".to_owned())?;
        let anchor_index = self
            .index
            .index_of(anchor_id)
            .ok_or_else(|| "paste anchor is missing".to_owned())?;
        let insert_at = self.subtree_end(anchor_index);
        let target_parent = self.index.parent_ids[anchor_index];
        let target_depth = self.index.depths[anchor_index];
        let mut next_id = self.next_available_block_id();
        let mut id_map = HashMap::new();
        for block in blocks {
            id_map.insert(block.source_id, next_id);
            next_id = next_id.saturating_add(1);
        }
        let root_depth = blocks
            .iter()
            .filter(|block| block.parent_source_id.is_none())
            .map(|block| block.depth)
            .min()
            .unwrap_or(blocks[0].depth);
        let mut inserted_records = Vec::with_capacity(blocks.len());
        let mut inserted_payloads = Vec::with_capacity(blocks.len());
        for (offset, block) in blocks.iter().enumerate() {
            let block_id = id_map[&block.source_id];
            let parent_id = block
                .parent_source_id
                .and_then(|source_id| id_map.get(&source_id).copied())
                .or(target_parent);
            let depth = target_depth.saturating_add(block.depth.saturating_sub(root_depth));
            let mut remapped_payload = block.payload.clone();
            if let BlockPayload::Columns(columns) = &mut remapped_payload {
                for column in &mut columns.columns {
                    column.block_id = id_map.get(&column.block_id).copied().ok_or_else(|| {
                        format!(
                            "columns clipboard is missing referenced column {}",
                            column.block_id
                        )
                    })?;
                }
            }
            let payload = BlockPayloadRecord {
                block_id,
                // The applier advances every touched payload exactly once on
                // commit, so a newly inserted block starts from version zero.
                content_version: 0,
                kind: block.kind.clone(),
                payload: remapped_payload,
            };
            let record = BlockIndexRecord::new(
                block_id,
                parent_id,
                depth,
                kind_tag_for_rich_block_kind(&block.kind),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(
                block_id,
                estimate_payload_height(&payload, insert_at + offset),
            ));
            inserted_records.push(record);
            inserted_payloads.push(payload);
        }
        let first_id = inserted_records[0].id;
        let first_text_len =
            editable_text_for_payload(&inserted_payloads[0].payload).map(|text| text.len());
        let after_selection = first_text_len.map(|offset| {
            DocumentSelection::caret(TextPosition {
                block_id: first_id,
                offset,
                affinity: TextAffinity::Downstream,
            })
        });
        self.apply_local_insert_blocks_transaction(LocalInsertBlocksTransaction {
            index: insert_at,
            blocks: inserted_records,
            payloads: inserted_payloads,
            kind: EditTransactionKind::Paste,
            origin: cditor_core::edit::ChangeOrigin::Import,
            before_selection,
            after_selection,
        })?;
        if let Some(text_len) = first_text_len {
            self.focus_block_at_offset(first_id, text_len)?;
        } else {
            self.focus_block(first_id);
        }
        Ok(true)
    }
}
