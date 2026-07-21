use super::*;

impl DocumentRuntime {
    pub fn resize_columns_boundary(
        &mut self,
        group_id: BlockId,
        boundary: usize,
        delta_px: f64,
        available_width: f64,
    ) -> Result<bool, String> {
        let before = self
            .payload_window
            .get(group_id)
            .cloned()
            .ok_or_else(|| format!("missing columns payload for block {group_id}"))?;
        let BlockPayload::Columns(mut columns) = before.payload.clone() else {
            return Err(format!("block {group_id} is not a columns group"));
        };
        let mut model = columns
            .layout_model(group_id)
            .map_err(|error| format!("invalid columns payload: {error:?}"))?;
        if !model
            .resize_boundary(boundary, delta_px, available_width, columns.gap_px())
            .map_err(|error| format!("columns resize failed: {error:?}"))?
        {
            return Ok(false);
        }
        columns.columns = model.columns().to_vec();
        self.apply_local_block_payload_transaction(
            group_id,
            EditTransactionKind::BlockStructureChange,
            RichBlockKind::ColumnsGroup,
            BlockPayload::Columns(columns),
        )
    }

    pub fn columns_layout_snapshot(
        &self,
        group_id: BlockId,
        available_width: f64,
    ) -> Result<cditor_core::layout::ColumnsLayout, String> {
        let payload = self
            .payload_window
            .get(group_id)
            .ok_or_else(|| format!("missing columns payload for block {group_id}"))?;
        let BlockPayload::Columns(columns) = &payload.payload else {
            return Err(format!("block {group_id} is not a columns group"));
        };
        let records = self.index_records();
        cditor_core::rich_text::validate_columns_structure(&records, std::slice::from_ref(payload))
            .map_err(|error| format!("invalid columns structure: {error:?}"))?;
        let model = columns
            .layout_model(group_id)
            .map_err(|error| format!("invalid columns payload: {error:?}"))?;
        let content_heights = model
            .columns()
            .iter()
            .map(|column| {
                let index = self
                    .index
                    .index_of(column.block_id)
                    .ok_or_else(|| format!("missing column block {}", column.block_id))?;
                Ok(self.index.layout_meta[index + 1..self.subtree_end(index)]
                    .iter()
                    .map(cditor_core::layout::BlockLayoutMeta::effective_height)
                    .sum::<f64>())
            })
            .collect::<Result<Vec<_>, String>>()?;
        model
            .layout(available_width, columns.gap_px(), &content_heights)
            .map_err(|error| format!("columns layout failed: {error:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::layout::ColumnSpec;
    use cditor_core::rich_text::ColumnsGroupPayload;

    fn runtime_with_columns() -> DocumentRuntime {
        let kinds = [
            RichBlockKind::ColumnsGroup,
            RichBlockKind::Column,
            RichBlockKind::Paragraph,
            RichBlockKind::Column,
            RichBlockKind::Paragraph,
            RichBlockKind::Paragraph,
        ];
        let parents = [None, Some(1), Some(2), Some(1), Some(4), Some(4)];
        let depths = [0, 1, 2, 1, 2, 2];
        let heights = [96.0, 0.0, 80.0, 0.0, 120.0, 100.0];
        let records = (0..kinds.len())
            .map(|index| {
                BlockIndexRecord::new(
                    index as u64 + 1,
                    parents[index],
                    depths[index],
                    kind_tag_for_rich_block_kind(&kinds[index]),
                    0,
                )
                .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(
                    index as u64 + 1,
                    heights[index],
                ))
            })
            .collect::<Vec<_>>();
        let payloads = kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| BlockPayloadRecord {
                block_id: index as u64 + 1,
                content_version: 1,
                kind: kind.clone(),
                payload: if index == 0 {
                    BlockPayload::Columns(ColumnsGroupPayload {
                        columns: vec![
                            ColumnSpec {
                                block_id: 2,
                                weight: 500_000,
                            },
                            ColumnSpec {
                                block_id: 4,
                                weight: 500_000,
                            },
                        ],
                        gap_milli: 24_000,
                    })
                } else if matches!(kind, RichBlockKind::Paragraph) {
                    BlockPayload::RichText {
                        spans: vec![InlineSpan::plain("content")],
                    }
                } else {
                    BlockPayload::Empty
                },
            })
            .collect();
        DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0)
    }

    #[test]
    fn runtime_columns_layout_uses_subtree_heights_and_document_order() {
        let runtime = runtime_with_columns();

        let layout = runtime.columns_layout_snapshot(1, 624.0).unwrap();
        assert_eq!(layout.height, 220.0);
        assert_eq!(layout.tracks[0].block_id, 2);
        assert_eq!(layout.tracks[0].content_height, 80.0);
        assert_eq!(layout.tracks[1].block_id, 4);
        assert_eq!(layout.tracks[1].content_height, 220.0);
    }

    #[test]
    fn columns_resize_is_one_typed_undoable_payload_transaction() {
        let mut runtime = runtime_with_columns();
        let before = runtime.block_payload_record(1).unwrap();

        assert!(runtime.resize_columns_boundary(1, 0, 120.0, 624.0).unwrap());
        let after = runtime.block_payload_record(1).unwrap();
        assert_ne!(after.payload, before.payload);
        assert_eq!(runtime.pending_structure_transaction_count(), 1);

        assert!(runtime.undo_focused_block().unwrap());
        assert_eq!(
            runtime.block_payload_record(1).unwrap().payload,
            before.payload
        );
        assert!(runtime.redo_focused_block().unwrap());
        assert_eq!(
            runtime.block_payload_record(1).unwrap().payload,
            after.payload
        );
    }

    #[test]
    fn columns_clipboard_remaps_group_column_ids_with_the_subtree() {
        let mut runtime = runtime_with_columns();
        runtime.selected_block_ids.insert(1);
        let clipboard = runtime.clipboard_selection_snapshot().unwrap();
        let ClipboardSelection::Blocks { blocks } = &clipboard else {
            panic!("expected block clipboard")
        };
        assert_eq!(blocks.len(), 6);

        assert!(runtime.paste_clipboard_selection(&clipboard).unwrap());
        let BlockPayload::Columns(columns) = runtime.block_payload_record(7).unwrap().payload
        else {
            panic!("expected copied columns payload")
        };
        assert_eq!(
            columns
                .columns
                .iter()
                .map(|column| column.block_id)
                .collect::<Vec<_>>(),
            vec![8, 10]
        );
        cditor_core::rich_text::validate_columns_structure(
            &runtime.index_records_snapshot(),
            &runtime.loaded_payload_records_snapshot(),
        )
        .unwrap();
    }
}
