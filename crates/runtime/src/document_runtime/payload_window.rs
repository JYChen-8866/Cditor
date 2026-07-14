use super::*;

impl DocumentRuntime {
    pub fn plan_payload_window_load_if_needed(
        &mut self,
        block_range: Range<usize>,
    ) -> Option<PayloadWindowLoadRequest> {
        let bounded_range = self.bounded_payload_window_range(block_range);
        let block_ids = self.payload_window_block_ids(&bounded_range);
        let range_changed = self.payload_window.block_range != bounded_range;
        let has_unrequested_missing = block_ids.iter().any(|block_id| {
            !self.payload_window.payloads.contains_key(block_id)
                && !self.payload_window.loading.contains(block_id)
                && self.payload_window.can_retry(*block_id)
        });

        if !range_changed && !has_unrequested_missing {
            return None;
        }

        self.payload_window_generation = self.payload_window_generation.saturating_add(1);
        let generation = self.payload_window_generation;
        self.payload_window.block_range = bounded_range.clone();

        let missing_block_ids = block_ids
            .into_iter()
            .filter(|block_id| {
                !self.payload_window.payloads.contains_key(block_id)
                    && !self.payload_window.loading.contains(block_id)
                    && self.payload_window.can_retry(*block_id)
            })
            .collect::<Vec<_>>();
        if missing_block_ids.is_empty() {
            return None;
        }
        for block_id in &missing_block_ids {
            self.payload_window.mark_loading(*block_id);
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

    pub fn apply_payload_window_load_error(
        &mut self,
        request: PayloadWindowLoadRequest,
        message: impl Into<String>,
    ) -> PayloadWindowApplyDecision {
        if request.generation != self.payload_window_generation {
            return PayloadWindowApplyDecision::DiscardedStaleGeneration {
                expected: self.payload_window_generation,
                actual: request.generation,
            };
        }
        let message = message.into();
        for block_id in request.block_ids {
            self.payload_window.mark_failed(block_id, message.clone());
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
