use super::*;

impl DocumentRuntime {
    pub fn activate_payload_window_if_resident(&mut self, block_range: Range<usize>) -> bool {
        let bounded_range = self.bounded_payload_window_range(block_range);
        if self.payload_window.block_range == bounded_range {
            return false;
        }
        let block_ids = self.payload_window_block_ids(&bounded_range);
        let all_resident = block_ids
            .iter()
            .all(|block_id| self.payload_window.payloads.contains_key(block_id));
        if !all_resident {
            return false;
        }
        self.payload_window.block_range = bounded_range;
        for block_id in block_ids {
            self.payload_window.touch(block_id);
        }
        true
    }

    pub fn plan_payload_window_load_if_needed(
        &mut self,
        block_range: Range<usize>,
    ) -> Option<PayloadWindowLoadRequest> {
        let bounded_range = self.bounded_payload_window_range(block_range);
        let block_ids = self.payload_window_block_ids(&bounded_range);
        let range_changed = self.payload_window.block_range != bounded_range;
        let has_missing = block_ids
            .iter()
            .any(|block_id| !self.payload_window.payloads.contains_key(block_id));
        let missing_block_ids = block_ids
            .iter()
            .copied()
            .filter(|block_id| {
                !self.payload_window.payloads.contains_key(block_id)
                    && !self.payload_window.loading.contains(block_id)
                    && self.payload_window.can_retry(*block_id)
            })
            .collect::<Vec<_>>();

        if !range_changed && missing_block_ids.is_empty() {
            return None;
        }
        // A previously visited window can already be resident even though it is
        // not the active range. Switch to it without invalidating an in-flight
        // generation. Likewise, if every missing block is already loading, keep
        // that generation alive and wait for its result.
        if missing_block_ids.is_empty() {
            if !has_missing {
                self.payload_window.block_range = bounded_range;
                for block_id in block_ids {
                    self.payload_window.touch(block_id);
                }
            }
            return None;
        }

        self.payload_window_generation = self.payload_window_generation.saturating_add(1);
        let generation = self.payload_window_generation;
        self.payload_window.block_range = bounded_range.clone();
        for &block_id in &block_ids {
            if self.payload_window.payloads.contains_key(&block_id) {
                self.payload_window.touch(block_id);
            }
        }
        for block_id in &missing_block_ids {
            self.payload_window.mark_loading(*block_id, generation);
        }

        Some(PayloadWindowLoadRequest {
            generation,
            block_range: bounded_range,
            block_ids: missing_block_ids,
        })
    }

    pub fn plan_payload_window_load(
        &mut self,
        block_range: Range<usize>,
    ) -> PayloadWindowLoadRequest {
        self.payload_window_generation = self.payload_window_generation.saturating_add(1);
        let generation = self.payload_window_generation;
        let bounded_range = self.bounded_payload_window_range(block_range);
        self.payload_window.block_range = bounded_range.clone();
        let block_ids = self.payload_window_block_ids(&bounded_range);

        for block_id in &block_ids {
            if self.payload_window.payloads.contains_key(block_id) {
                self.payload_window.touch(*block_id);
            } else {
                self.payload_window.mark_loading(*block_id, generation);
            }
        }

        PayloadWindowLoadRequest {
            generation,
            block_range: bounded_range,
            block_ids,
        }
    }

    pub fn apply_payload_window_result(
        &mut self,
        result: PayloadWindowLoadResult,
    ) -> PayloadWindowApplyDecision {
        let expected_generation = self.payload_window_generation;
        let result_generation = result.request.generation;
        let is_current = result_generation == expected_generation;
        if is_current {
            self.payload_window.block_range = result.request.block_range.clone();
        }
        for payload in result.records {
            // Results from an older viewport are still valid cache data. Apply
            // them only while that request still owns the loading marker, so a
            // late database response can never overwrite a local edit or a newer
            // request for the same block.
            if !self
                .payload_window
                .finish_loading(payload.block_id, result_generation)
            {
                continue;
            }
            let mut payload = normalize_payload_record_for_kind(payload);
            self.sync_table_runtime_from_loaded_record(&mut payload);
            self.payload_window.insert_loaded(payload);
        }
        for block_id in result.missing_block_ids {
            if self
                .payload_window
                .finish_loading(block_id, result_generation)
            {
                self.payload_window
                    .mark_failed(block_id, "payload missing from store");
            }
        }
        if !is_current {
            return PayloadWindowApplyDecision::DiscardedStaleGeneration {
                expected: expected_generation,
                actual: result_generation,
            };
        }
        PayloadWindowApplyDecision::Applied
    }

    pub fn payload_window_generation(&self) -> u64 {
        self.payload_window_generation
    }

    pub fn apply_payload_window_load_error(
        &mut self,
        request: PayloadWindowLoadRequest,
        message: impl Into<String>,
    ) -> PayloadWindowApplyDecision {
        let expected_generation = self.payload_window_generation;
        let request_generation = request.generation;
        let message = message.into();
        for block_id in request.block_ids {
            if self
                .payload_window
                .finish_loading(block_id, request_generation)
            {
                self.payload_window.mark_failed(block_id, message.clone());
            }
        }
        if request_generation != expected_generation {
            return PayloadWindowApplyDecision::DiscardedStaleGeneration {
                expected: expected_generation,
                actual: request_generation,
            };
        }
        PayloadWindowApplyDecision::Applied
    }

    fn bounded_payload_window_range(&self, block_range: Range<usize>) -> Range<usize> {
        block_range
            .start
            .min(self.visible_index.total_visible_count())
            ..block_range
                .end
                .min(self.visible_index.total_visible_count())
    }

    fn payload_window_block_ids(&self, block_range: &Range<usize>) -> Vec<BlockId> {
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
        for visible_index in block_range.clone() {
            if let Some(block_id) = self.visible_index.id_at_visible_index(visible_index) {
                push_unique(&mut block_ids, block_id);
            }
        }
        block_ids
    }
}
