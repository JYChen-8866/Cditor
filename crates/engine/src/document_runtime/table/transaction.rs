use super::*;

impl DocumentRuntime {
    pub(in crate::document_runtime) fn commit_table_runtime_payload(
        &mut self,
        block_id: BlockId,
    ) -> Result<u64, String> {
        self.push_undo_snapshot(block_id)?;
        let next_table_payload = self
            .table_runtime(block_id)
            .ok_or_else(|| format!("missing table runtime for block {block_id}"))?
            .payload();
        let next_content_version = {
            let payload = self
                .payload_window
                .payloads
                .get_mut(&block_id)
                .ok_or_else(|| format!("missing payload for block {block_id}"))?;
            payload.payload = next_table_payload;
            payload.content_version = payload.content_version.saturating_add(1);
            payload.content_version
        };
        self.text_models.remove(&block_id);
        if let Some(editing) = self.editing.as_mut()
            && editing.block_id == block_id
        {
            editing.content_version = next_content_version;
            if let Some(focused) = self
                .focused_table_cell
                .filter(|focused| focused.block_id == block_id)
            {
                editing.set_input_target(InputTarget::TableCell {
                    block_id,
                    row: focused.row,
                    col: focused.col,
                });
                editing.set_selected_range(focused.selected_range(), focused.selection_reversed);
                if let Some(marked_range) = focused.marked_range() {
                    editing.set_marked_range(marked_range);
                } else {
                    editing.clear_composition();
                }
            } else {
                editing.set_input_target(InputTarget::BlockText { block_id });
                editing.set_collapsed_selection(0);
            }
        }
        let _ = self.refresh_table_block_height(block_id)?;
        Ok(next_content_version)
    }

    pub(in crate::document_runtime) fn refresh_table_block_height(
        &mut self,
        block_id: BlockId,
    ) -> Result<bool, String> {
        let Some(payload) = self.payload_window.get(block_id) else {
            return Ok(false);
        };
        if !matches!(payload.kind, RichBlockKind::Table) {
            return Ok(false);
        }
        let Some(table_runtime) = self.table_runtime(block_id) else {
            return Ok(false);
        };
        let table_height = table_payload_projected_height_px(table_runtime.table());
        let block_height = f64::from(table_height).max(120.0);
        self.apply_measured_height(block_id, payload.content_version, block_height)
    }
}
