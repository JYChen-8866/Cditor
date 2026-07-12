use super::*;

impl DocumentRuntime {
    pub fn plan_payload_window_load(
        &mut self,
        block_range: Range<usize>,
    ) -> PayloadWindowLoadRequest {
        self.payload_window_generation = self.payload_window_generation.saturating_add(1);
        let generation = self.payload_window_generation;
        let bounded_range = block_range
            .start
            .min(self.visible_index.total_visible_count())
            ..block_range
                .end
                .min(self.visible_index.total_visible_count());
        self.payload_window.block_range = bounded_range.clone();

        let mut block_ids = Vec::new();
        if let Some(block_id) = self.focused_block_id() {
            push_unique(&mut block_ids, block_id);
        }
        if !self.selected_block_ids.is_empty() {
            if let Some(first) = self.selected_block_ids.iter().min().copied() {
                push_unique(&mut block_ids, first);
            }
            if let Some(last) = self.selected_block_ids.iter().max().copied() {
                push_unique(&mut block_ids, last);
            }
        }
        for visible_index in bounded_range.clone() {
            if let Some(block_id) = self.visible_index.id_at_visible_index(visible_index) {
                push_unique(&mut block_ids, block_id);
            }
        }

        for block_id in &block_ids {
            if !self.payload_window.payloads.contains_key(block_id) {
                self.payload_window.mark_loading(*block_id);
            }
        }

        PayloadWindowLoadRequest {
            generation,
            block_range: bounded_range,
            block_ids,
        }
    }

    pub async fn load_payload_window_request(
        payload_store: &PostgresPayloadStore,
        request: PayloadWindowLoadRequest,
    ) -> PostgresStorageResult<PayloadWindowLoadResult> {
        let loaded = payload_store
            .load_block_payloads(&request.block_ids)
            .await?;
        Ok(PayloadWindowLoadResult {
            request,
            records: loaded.records,
            missing_block_ids: loaded.missing_block_ids,
        })
    }

    pub async fn load_payload_window_from_store(
        &mut self,
        payload_store: &PostgresPayloadStore,
        block_range: Range<usize>,
    ) -> PostgresStorageResult<PayloadWindowApplyDecision> {
        let request = self.plan_payload_window_load(block_range);
        let result = Self::load_payload_window_request(payload_store, request).await?;
        Ok(self.apply_payload_window_result(result))
    }

    pub fn apply_payload_window_result(
        &mut self,
        result: PayloadWindowLoadResult,
    ) -> PayloadWindowApplyDecision {
        if result.request.generation != self.payload_window_generation {
            return PayloadWindowApplyDecision::DiscardedStaleGeneration {
                expected: self.payload_window_generation,
                actual: result.request.generation,
            };
        }

        self.payload_window.block_range = result.request.block_range;
        for payload in result.records {
            let mut payload = normalize_payload_record_for_kind(payload);
            self.sync_table_runtime_from_loaded_record(&mut payload);
            self.payload_window.insert(payload);
        }
        for block_id in result.missing_block_ids {
            self.payload_window
                .mark_failed(block_id, "payload missing from store");
        }
        PayloadWindowApplyDecision::Applied
    }

    pub fn payload_window_generation(&self) -> u64 {
        self.payload_window_generation
    }
}
