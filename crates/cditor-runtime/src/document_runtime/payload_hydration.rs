use super::*;

impl DocumentRuntime {
    pub(super) fn hydrate_payload_runtime_state(&mut self, block_id: BlockId) {
        let Some(payload) = self.document.payload_window.get(block_id) else {
            return;
        };
        let needs_table_runtime = matches!(payload.kind, RichBlockKind::Table)
            && !self.document.table_runtimes.contains_key(&block_id);
        let needs_text_model = !self.document.text_models.contains_key(&block_id)
            && editable_text_len_for_payload(&payload.payload).is_some();
        if !needs_table_runtime && !needs_text_model {
            return;
        }

        let mut payload = payload.clone();
        self.sync_table_runtime_from_loaded_record(&mut payload);
        self.document.payload_window.insert(payload);
    }
}
