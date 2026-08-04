use super::*;

impl DocumentRuntime {
    pub fn plan_emergency_payload_load(
        &mut self,
        block_ids: &[BlockId],
    ) -> Result<Option<PayloadWindowLoadRequest>, String> {
        let mut missing = Vec::new();
        for block_id in block_ids {
            if self.document.index.index_of(*block_id).is_none() {
                continue;
            }
            if !self.document.payload_window.payloads.contains_key(block_id)
                && !missing.contains(block_id)
            {
                missing.push(*block_id);
            }
        }
        if missing.is_empty() {
            return Ok(None);
        }

        self.layout.payload_window_generation =
            self.layout.payload_window_generation.saturating_add(1);
        let generation = self.layout.payload_window_generation;
        for block_id in &missing {
            self.document.payload_window.mark_loading_with_priority(
                *block_id,
                generation,
                PayloadLoadPriority::Emergency,
            );
        }
        Ok(Some(PayloadWindowLoadRequest {
            generation,
            block_range: self.document.payload_window.block_range.clone(),
            block_ids: missing,
        }))
    }

    pub fn activate_payload_window_if_resident(&mut self, block_range: Range<usize>) -> bool {
        let bounded_range = self.bounded_payload_window_range(block_range);
        if self.document.payload_window.block_range == bounded_range {
            return false;
        }
        let block_ids = self.payload_window_block_ids(&bounded_range);
        let all_resident = block_ids
            .iter()
            .all(|block_id| self.document.payload_window.payloads.contains_key(block_id));
        if !all_resident {
            return false;
        }
        self.document.payload_window.block_range = bounded_range;
        for block_id in block_ids {
            self.document.payload_window.touch(block_id);
        }
        true
    }

    pub fn plan_payload_window_load_if_needed(
        &mut self,
        block_range: Range<usize>,
    ) -> Option<PayloadWindowLoadRequest> {
        let bounded_range = self.bounded_payload_window_range(block_range);
        let block_ids = self.payload_window_block_ids(&bounded_range);
        let missing_block_ids = block_ids
            .iter()
            .copied()
            .filter(|block_id| {
                !self.document.payload_window.payloads.contains_key(block_id)
                    && self
                        .document
                        .payload_window
                        .loading_priority(*block_id)
                        .is_none_or(|priority| priority < PayloadLoadPriority::Visible)
                    && self.document.payload_window.can_retry(*block_id)
            })
            .collect::<Vec<_>>();

        // Planning owns only I/O intent. The presented/active range is changed
        // by the projection window commit after its visible core is resident.
        if missing_block_ids.is_empty() {
            for block_id in block_ids {
                self.document.payload_window.touch(block_id);
            }
            return None;
        }

        self.layout.payload_window_generation =
            self.layout.payload_window_generation.saturating_add(1);
        let generation = self.layout.payload_window_generation;
        for &block_id in &block_ids {
            if self
                .document
                .payload_window
                .payloads
                .contains_key(&block_id)
            {
                self.document.payload_window.touch(block_id);
            }
        }
        for block_id in &missing_block_ids {
            self.document
                .payload_window
                .mark_loading(*block_id, generation);
        }

        Some(PayloadWindowLoadRequest {
            generation,
            block_range: bounded_range,
            block_ids: missing_block_ids,
        })
    }

    pub fn plan_payload_prefetch_load_if_needed(
        &mut self,
        block_range: Range<usize>,
    ) -> Option<PayloadWindowLoadRequest> {
        let bounded_range = self.bounded_payload_window_range(block_range);
        let missing_block_ids = self
            .payload_window_block_ids(&bounded_range)
            .into_iter()
            .filter(|block_id| {
                !self.document.payload_window.payloads.contains_key(block_id)
                    && !self.document.payload_window.loading.contains(block_id)
                    && self.document.payload_window.can_retry(*block_id)
            })
            .collect::<Vec<_>>();
        if missing_block_ids.is_empty() {
            return None;
        }

        self.layout.payload_window_generation =
            self.layout.payload_window_generation.saturating_add(1);
        let generation = self.layout.payload_window_generation;
        for block_id in &missing_block_ids {
            self.document.payload_window.mark_loading_with_priority(
                *block_id,
                generation,
                PayloadLoadPriority::Prefetch,
            );
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
        self.layout.payload_window_generation =
            self.layout.payload_window_generation.saturating_add(1);
        let generation = self.layout.payload_window_generation;
        let bounded_range = self.bounded_payload_window_range(block_range);
        let block_ids = self.payload_window_block_ids(&bounded_range);

        for block_id in &block_ids {
            if self.document.payload_window.payloads.contains_key(block_id) {
                self.document.payload_window.touch(*block_id);
            } else {
                self.document
                    .payload_window
                    .mark_loading(*block_id, generation);
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
        self.apply_payload_result(result, PayloadMissingPolicy::PublishFailure)
    }

    pub fn apply_payload_prefetch_result(
        &mut self,
        result: PayloadWindowLoadResult,
    ) -> PayloadWindowApplyDecision {
        self.apply_payload_result(result, PayloadMissingPolicy::ReleaseOwnership)
    }

    fn apply_payload_result(
        &mut self,
        result: PayloadWindowLoadResult,
        missing_policy: PayloadMissingPolicy,
    ) -> PayloadWindowApplyDecision {
        let expected_generation = self.layout.payload_window_generation;
        let result_generation = result.request.generation;
        let is_current = result_generation == expected_generation;
        for payload in result.records {
            let block_id = payload.block_id();
            // Results from an older viewport are still valid cache data. Apply
            // them only while that request still owns the loading marker, so a
            // late database response can never overwrite a local edit or a newer
            // request for the same block.
            if !self
                .document
                .payload_window
                .finish_loading(block_id, result_generation)
            {
                continue;
            }
            self.document.payload_window.insert_loaded_prepared(payload);
        }
        for block_id in result.missing_block_ids {
            if self
                .document
                .payload_window
                .finish_loading(block_id, result_generation)
                && missing_policy == PayloadMissingPolicy::PublishFailure
            {
                self.document
                    .payload_window
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
        self.layout.payload_window_generation
    }

    pub fn cancel_payload_window_load(&mut self, generation: u64) -> usize {
        self.document
            .payload_window
            .cancel_loading_generation(generation)
    }

    pub fn apply_payload_window_load_error(
        &mut self,
        request: PayloadWindowLoadRequest,
        message: impl Into<String>,
    ) -> PayloadWindowApplyDecision {
        let expected_generation = self.layout.payload_window_generation;
        let request_generation = request.generation;
        let message = message.into();
        for block_id in request.block_ids {
            if self
                .document
                .payload_window
                .finish_loading(block_id, request_generation)
            {
                self.document
                    .payload_window
                    .mark_failed(block_id, message.clone());
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

    pub fn apply_payload_prefetch_load_error(
        &mut self,
        request: PayloadWindowLoadRequest,
    ) -> PayloadWindowApplyDecision {
        let expected_generation = self.layout.payload_window_generation;
        let request_generation = request.generation;
        for block_id in request.block_ids {
            self.document
                .payload_window
                .finish_loading(block_id, request_generation);
        }
        if request_generation != expected_generation {
            return PayloadWindowApplyDecision::DiscardedStaleGeneration {
                expected: expected_generation,
                actual: request_generation,
            };
        }
        PayloadWindowApplyDecision::Applied
    }

    /// Clears terminal payload failures for the requested visible range so an
    /// explicit user retry can start a fresh bounded retry cycle.
    pub fn retry_failed_payload_window(&mut self, block_range: Range<usize>) -> usize {
        let bounded_range = self.bounded_payload_window_range(block_range);
        let mut reset_count = 0;
        for block_id in self.payload_window_block_ids(&bounded_range) {
            if self.document.payload_window.clear_failure(block_id) {
                reset_count += 1;
            }
        }
        reset_count
    }

    fn bounded_payload_window_range(&self, block_range: Range<usize>) -> Range<usize> {
        block_range
            .start
            .min(self.document.visible_index.total_visible_count())
            ..block_range
                .end
                .min(self.document.visible_index.total_visible_count())
    }

    fn payload_window_block_ids(&self, block_range: &Range<usize>) -> Vec<BlockId> {
        // Interaction pins are at most three ids. Keep their legacy ordering,
        // then append the visible sequence without repeatedly searching the
        // growing result vector. VisibleDocumentIndex guarantees unique ids.
        let mut block_ids = Vec::with_capacity(block_range.len().saturating_add(3));
        if let Some(block_id) = self.focused_block_id() {
            push_unique(&mut block_ids, block_id);
        }
        if !self.selection.selected_block_ids.is_empty() {
            if let Some(first) = self.selection.selected_block_ids.iter().min().copied() {
                push_unique(&mut block_ids, first);
            }
            if let Some(last) = self.selection.selected_block_ids.iter().max().copied() {
                push_unique(&mut block_ids, last);
            }
        }
        let interaction_pin_count = block_ids.len();
        for visible_index in block_range.clone() {
            if let Some(block_id) = self
                .document
                .visible_index
                .id_at_visible_index(visible_index)
                && !block_ids[..interaction_pin_count].contains(&block_id)
            {
                block_ids.push(block_id);
            }
        }
        block_ids
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PayloadMissingPolicy {
    PublishFailure,
    ReleaseOwnership,
}
