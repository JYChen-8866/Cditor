use super::*;

impl DocumentRuntime {
    pub fn insert_image_asset_after_focused(
        &mut self,
        image: ImagePayload,
    ) -> Result<(BlockId, BlockId), String> {
        let current_block_id = self
            .focused_block_id()
            .or_else(|| self.index.block_ids.last().copied())
            .ok_or_else(|| "missing focused block".to_owned())?;
        let current_index = self
            .index
            .index_of(current_block_id)
            .ok_or_else(|| format!("missing block index for {current_block_id}"))?;
        let parent_id = self.index.parent_ids[current_index];
        let depth = self.index.depths[current_index];
        let insert_at = self.subtree_end(current_index);
        let image_block_id = self.next_available_block_id();
        let trailing_block_id = image_block_id.saturating_add(1);

        let image_payload = BlockPayloadRecord {
            block_id: image_block_id,
            content_version: 0,
            kind: RichBlockKind::Image,
            payload: BlockPayload::Image(image),
        };
        let image_record = BlockIndexRecord::new(
            image_block_id,
            parent_id,
            depth,
            kind_tag_for_rich_block_kind(&RichBlockKind::Image),
            0,
        )
        .with_layout_meta(BlockLayoutMeta::new(
            image_block_id,
            estimate_payload_height(&image_payload, insert_at),
        ));
        let mut paragraph_payload =
            BlockPayloadRecord::rich_text(trailing_block_id, RichBlockKind::Paragraph, "");
        paragraph_payload.content_version = 0;
        let paragraph_record = BlockIndexRecord::new(
            trailing_block_id,
            parent_id,
            depth,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            0,
        )
        .with_layout_meta(BlockLayoutMeta::new(
            trailing_block_id,
            estimate_payload_height(&paragraph_payload, insert_at.saturating_add(1)),
        ));
        self.apply_local_insert_blocks_transaction(LocalInsertBlocksTransaction {
            index: insert_at,
            blocks: vec![image_record, paragraph_record],
            payloads: vec![image_payload, paragraph_payload],
            kind: EditTransactionKind::Paste,
            origin: cditor_core::edit::ChangeOrigin::Import,
            before_selection: self.document_selection_snapshot(),
            after_selection: Some(DocumentSelection::caret(TextPosition {
                block_id: trailing_block_id,
                offset: 0,
                affinity: TextAffinity::Downstream,
            })),
        })?;
        self.focus_block_at_offset(trailing_block_id, 0)?;
        Ok((image_block_id, trailing_block_id))
    }

    pub(crate) fn update_image_display_width_ratio(
        &mut self,
        block_id: BlockId,
        display_width_ratio_milli: u16,
    ) -> Result<bool, String> {
        let ratio = display_width_ratio_milli.clamp(200, 1000);
        let record = self
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let BlockPayload::Image(mut image) = record.payload else {
            return Ok(false);
        };
        if image.display_width_ratio_milli == Some(ratio) {
            return Ok(false);
        }
        image.display_width_ratio_milli = Some(ratio);
        self.apply_local_block_payload_transaction(
            block_id,
            EditTransactionKind::DragDrop,
            record.kind,
            BlockPayload::Image(image),
        )
    }
}
