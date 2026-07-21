use super::*;

impl DocumentRuntime {
    pub fn table_horizontal_scroll_offset_px(&self, block_id: BlockId) -> f32 {
        self.table_horizontal_scroll_offsets
            .get(&block_id)
            .copied()
            .unwrap_or(0.0)
    }

    pub fn set_table_horizontal_scroll_offset_px(
        &mut self,
        block_id: BlockId,
        offset_px: f32,
    ) -> Result<bool, String> {
        if self.table_runtime(block_id).is_none() {
            return Ok(false);
        }
        let offset_px = offset_px.min(0.0);
        if self.table_horizontal_scroll_offset_px(block_id) == offset_px {
            return Ok(false);
        }
        self.table_horizontal_scroll_offsets
            .insert(block_id, offset_px);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{
        BlockPayload, BlockPayloadRecord, RichBlockKind, TableCellPayload, TablePayload,
        TableRowPayload,
    };

    use super::*;

    #[test]
    fn table_horizontal_scroll_offset_is_runtime_state_and_projection_truth() {
        let payload = BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(TablePayload {
                rows: vec![TableRowPayload {
                    cells: vec![TableCellPayload::plain("a"), TableCellPayload::plain("b")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        };
        let mut runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);

        assert!(
            runtime
                .set_table_horizontal_scroll_offset_px(1, -120.0)
                .unwrap()
        );

        let projection = runtime.projection_for_window();
        assert_eq!(
            projection.blocks[0]
                .table_view
                .as_ref()
                .map(|view| view.horizontal_scroll_offset_px),
            Some(-120.0)
        );
        assert!(
            !runtime
                .set_table_horizontal_scroll_offset_px(99, -10.0)
                .unwrap()
        );
    }
}
