use super::*;

impl DocumentRuntime {
    pub(crate) fn set_video_source(
        &mut self,
        block_id: BlockId,
        source: String,
        title: String,
        media_type: Option<String>,
        asset: Option<cditor_core::edit::AssetSnapshot>,
    ) -> Result<bool, String> {
        let record = self
            .document
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        if !matches!(record.kind, RichBlockKind::Video) {
            return Err("video source can only be set on a video block".to_owned());
        }
        let next = BlockPayload::Video(cditor_core::rich_text::VideoPayload {
            source,
            title,
            media_type,
            ..Default::default()
        });
        if record.payload == next {
            return Ok(false);
        }
        let replacement =
            EditOperation::Block(cditor_core::edit::BlockEditOperation::ReplacePayload {
                block_id,
                before_kind: record.kind.clone(),
                before_payload: record.payload.clone(),
                after_kind: record.kind.clone(),
                after_payload: next.clone(),
            });
        let inverse_replacement =
            EditOperation::Block(cditor_core::edit::BlockEditOperation::ReplacePayload {
                block_id,
                before_kind: record.kind.clone(),
                before_payload: next,
                after_kind: record.kind,
                after_payload: record.payload,
            });
        let previous_assets = self
            .document
            .block_asset_ids
            .get(&block_id)
            .into_iter()
            .flatten()
            .filter_map(|asset_id| self.document.assets.get(asset_id).cloned())
            .collect::<Vec<_>>();
        let mut ops = vec![replacement];
        let mut inverse_ops = vec![inverse_replacement];
        for previous in &previous_assets {
            ops.push(EditOperation::Asset(
                cditor_core::edit::AssetEditOperation::Detach {
                    block_id,
                    asset: previous.clone(),
                },
            ));
            inverse_ops.insert(
                0,
                EditOperation::Asset(cditor_core::edit::AssetEditOperation::Attach {
                    block_id,
                    asset: previous.clone(),
                }),
            );
        }
        if let Some(asset) = asset {
            ops.push(EditOperation::Asset(
                cditor_core::edit::AssetEditOperation::Attach {
                    block_id,
                    asset: asset.clone(),
                },
            ));
            inverse_ops.insert(
                0,
                EditOperation::Asset(cditor_core::edit::AssetEditOperation::Detach {
                    block_id,
                    asset,
                }),
            );
        }
        self.apply_local_block_operations_transaction(LocalBlockOperationsTransaction {
            block_id,
            kind: EditTransactionKind::Paste,
            origin: cditor_core::edit::ChangeOrigin::User,
            ops,
            inverse_ops,
            before_selection: None,
            after_selection: None,
        })
    }

    pub(crate) fn insert_image_asset_after_focused_with_asset(
        &mut self,
        image: ImagePayload,
        asset: Option<cditor_core::edit::AssetSnapshot>,
        after_block_id: Option<BlockId>,
    ) -> Result<(BlockId, BlockId), String> {
        self.insert_asset_block_after_focused(
            RichBlockKind::Image,
            BlockPayload::Image(image),
            asset,
            after_block_id,
            EditTransactionKind::Paste,
        )
    }

    pub(crate) fn insert_video_asset_after_focused_with_asset(
        &mut self,
        video: VideoPayload,
        asset: Option<cditor_core::edit::AssetSnapshot>,
        after_block_id: Option<BlockId>,
    ) -> Result<(BlockId, BlockId), String> {
        self.insert_asset_block_after_focused(
            RichBlockKind::Video,
            BlockPayload::Video(video),
            asset,
            after_block_id,
            EditTransactionKind::DragDrop,
        )
    }

    fn insert_asset_block_after_focused(
        &mut self,
        kind: RichBlockKind,
        payload: BlockPayload,
        asset: Option<cditor_core::edit::AssetSnapshot>,
        after_block_id: Option<BlockId>,
        transaction_kind: EditTransactionKind,
    ) -> Result<(BlockId, BlockId), String> {
        let current_block_id = after_block_id
            .or_else(|| self.focused_block_id())
            .or_else(|| self.document.index.block_ids.last().copied())
            .ok_or_else(|| "missing focused block".to_owned())?;
        let current_index = self
            .document
            .index
            .index_of(current_block_id)
            .ok_or_else(|| format!("missing block index for {current_block_id}"))?;
        let parent_id = self.document.index.parent_ids[current_index];
        let depth = self.document.index.depths[current_index];
        let insert_at = self.subtree_end(current_index);
        let media_block_id = self.next_available_block_id();
        let trailing_block_id = media_block_id.saturating_add(1);

        let media_payload = BlockPayloadRecord {
            block_id: media_block_id,
            content_version: 0,
            kind: kind.clone(),
            payload,
        };
        let media_record = BlockIndexRecord::new(
            media_block_id,
            parent_id,
            depth,
            kind_tag_for_rich_block_kind(&kind),
            0,
        )
        .with_layout_meta(BlockLayoutMeta::new(
            media_block_id,
            estimate_payload_height(&media_payload, insert_at),
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
        let before_selection = self.document_selection_snapshot();
        let after_selection = Some(DocumentSelection::caret(TextPosition {
            block_id: trailing_block_id,
            offset: 0,
            affinity: TextAffinity::Downstream,
        }));
        if let Some(asset) = asset {
            let attach = cditor_core::edit::AssetEditOperation::Attach {
                block_id: media_block_id,
                asset: asset.clone(),
            };
            let detach = cditor_core::edit::AssetEditOperation::Detach {
                block_id: media_block_id,
                asset,
            };
            self.apply_local_structure_transaction(
                transaction_kind,
                cditor_core::edit::ChangeOrigin::Import,
                vec![
                    EditOperation::InsertBlocks {
                        index: insert_at,
                        blocks: vec![media_record, paragraph_record],
                        payloads: vec![media_payload, paragraph_payload],
                    },
                    EditOperation::Asset(attach),
                ],
                vec![
                    EditOperation::Asset(detach),
                    EditOperation::DeleteBlockRange {
                        range: insert_at..insert_at.saturating_add(2),
                    },
                ],
                vec![
                    TransactionPrecondition::StructureVersion(self.structure_version()),
                    TransactionPrecondition::BlockAbsent(media_block_id),
                    TransactionPrecondition::BlockAbsent(trailing_block_id),
                ],
                before_selection,
                after_selection,
                self.selected_block_ids_snapshot(),
                Vec::new(),
            )?;
        } else {
            self.apply_local_insert_blocks_transaction(LocalInsertBlocksTransaction {
                index: insert_at,
                blocks: vec![media_record, paragraph_record],
                payloads: vec![media_payload, paragraph_payload],
                kind: transaction_kind,
                origin: cditor_core::edit::ChangeOrigin::Import,
                before_selection,
                after_selection,
            })?;
        }
        self.focus_block_at_offset(trailing_block_id, 0)?;
        Ok((media_block_id, trailing_block_id))
    }

    pub(crate) fn update_image_display_width_ratio(
        &mut self,
        block_id: BlockId,
        display_width_ratio_milli: u16,
    ) -> Result<bool, String> {
        let ratio = display_width_ratio_milli.clamp(200, 1000);
        let record = self
            .document
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
