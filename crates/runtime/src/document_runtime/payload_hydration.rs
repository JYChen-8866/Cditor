use super::*;

impl DocumentRuntime {
    pub(super) fn hydrate_payload_runtime_state(&mut self, block_id: BlockId) {
        let Some(payload) = self.payload_window.get(block_id) else {
            return;
        };
        let needs_table_runtime = matches!(payload.kind, RichBlockKind::Table)
            && !self.table_runtimes.contains_key(&block_id);
        let needs_text_model = editable_text_for_payload(&payload.payload).is_some()
            && !self.text_models.contains_key(&block_id);
        if !needs_table_runtime && !needs_text_model {
            return;
        }

        let mut payload = payload.clone();
        self.sync_table_runtime_from_loaded_record(&mut payload);
        self.payload_window.insert(payload);
    }

    pub(super) fn hydrate_payload_runtime_state_for_range(&mut self, range: Range<usize>) {
        let block_ids = range
            .filter_map(|visible_index| self.visible_index.id_at_visible_index(visible_index))
            .collect::<Vec<_>>();
        for block_id in block_ids {
            self.hydrate_payload_runtime_state(block_id);
        }
    }
}
