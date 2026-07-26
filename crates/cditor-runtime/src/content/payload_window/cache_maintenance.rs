use std::ops::Bound::{Excluded, Unbounded};
use std::sync::Arc;

use cditor_core::ids::BlockId;
use cditor_core::rich_text::BlockPayloadRecord;

use crate::content::payload_cache::{
    PayloadCacheMaintenanceBudget, PayloadCachePolicy, estimated_payload_record_bytes,
};

use super::PayloadWindow;

#[derive(Debug, Default)]
pub(crate) struct PayloadWindowMaintenanceSlice {
    pub(crate) evicted: Vec<Arc<BlockPayloadRecord>>,
    pub(crate) byte_estimates_refreshed: usize,
    pub(crate) lru_candidates_examined: usize,
    pub(crate) maintenance_pending: bool,
}

impl PayloadWindow {
    pub(crate) fn restart_cache_maintenance_cycle(&mut self) {
        self.finish_maintenance_cycle();
    }

    /// Advances byte accounting and LRU eviction by one bounded slice.
    ///
    /// The LRU cursor moves past protected entries without removing them. If
    /// every candidate is dirty or pinned, the cursor reaches the end and the
    /// result stops requesting follow-up work. A later external maintenance
    /// request starts a new cycle from the oldest resident entry.
    pub(crate) fn maintain_cache_slice(
        &mut self,
        policy: PayloadCachePolicy,
        budget: PayloadCacheMaintenanceBudget,
        mut can_evict: impl FnMut(BlockId, &BlockPayloadRecord, bool) -> bool,
    ) -> PayloadWindowMaintenanceSlice {
        let mut result = PayloadWindowMaintenanceSlice::default();
        result.byte_estimates_refreshed =
            self.refresh_dirty_byte_estimates(budget.max_byte_estimate_refreshes);

        if !self.estimated_bytes_dirty.is_empty() {
            result.maintenance_pending = budget.max_byte_estimate_refreshes > 0;
            return result;
        }
        if !self.is_over_cache_policy(policy) {
            self.finish_maintenance_cycle();
            return result;
        }
        if budget.max_lru_candidates == 0 || budget.max_evictions == 0 {
            self.finish_maintenance_cycle();
            return result;
        }

        if !self.maintenance_cycle_active {
            self.maintenance_cycle_active = true;
            self.maintenance_cursor = None;
        }

        while self.is_over_cache_policy(policy)
            && result.lru_candidates_examined < budget.max_lru_candidates
            && result.evicted.len() < budget.max_evictions
        {
            let Some(candidate) = self.next_maintenance_candidate() else {
                break;
            };
            self.maintenance_cursor = Some(candidate);
            result.lru_candidates_examined += 1;

            let (_, block_id) = candidate;
            let Some(payload) = self.payloads.get(&block_id) else {
                continue;
            };
            if !can_evict(block_id, payload.as_ref(), self.is_dirty(block_id)) {
                continue;
            }
            if let Some(payload) = self.remove_internal(block_id) {
                result.evicted.push(payload);
            }
        }

        let over_capacity = self.is_over_cache_policy(policy);
        result.maintenance_pending = over_capacity && self.next_maintenance_candidate().is_some();
        if !result.maintenance_pending {
            self.finish_maintenance_cycle();
        }
        result
    }

    /// Compatibility entry point for explicit synchronous callers and tests.
    /// Interactive GPUI code must call `maintain_cache_slice` through the
    /// runtime port with a bounded budget.
    pub fn evict_to_limits(
        &mut self,
        max_entries: usize,
        max_estimated_bytes: usize,
        mut can_evict: impl FnMut(BlockId, &BlockPayloadRecord, bool) -> bool,
    ) -> Vec<Arc<BlockPayloadRecord>> {
        self.maintain_cache_slice(
            PayloadCachePolicy {
                max_entries,
                max_estimated_bytes,
            },
            PayloadCacheMaintenanceBudget::unbounded(),
            &mut can_evict,
        )
        .evicted
    }

    fn refresh_dirty_byte_estimates(&mut self, max_candidates: usize) -> usize {
        let mut candidates_examined = 0;
        let mut refreshed = 0;
        while candidates_examined < max_candidates {
            let Some(block_id) = self.estimated_bytes_dirty_queue.pop_front() else {
                break;
            };
            candidates_examined += 1;
            if !self.estimated_bytes_dirty.remove(&block_id) {
                continue;
            }
            let Some(bytes) = self
                .payloads
                .get(&block_id)
                .map(|payload| estimated_payload_record_bytes(payload.as_ref()))
            else {
                continue;
            };
            self.replace_estimated_size(block_id, bytes);
            refreshed += 1;
        }
        if self.estimated_bytes_dirty.is_empty() {
            self.estimated_bytes_dirty_queue.clear();
        }
        refreshed
    }

    fn next_maintenance_candidate(&self) -> Option<(u64, BlockId)> {
        match self.maintenance_cursor {
            Some(cursor) => self
                .access_order
                .range((Excluded(cursor), Unbounded))
                .next()
                .copied(),
            None => self.access_order.first().copied(),
        }
    }

    fn is_over_cache_policy(&self, policy: PayloadCachePolicy) -> bool {
        self.payloads.len() > policy.max_entries
            || self.total_estimated_bytes > policy.max_estimated_bytes
    }

    fn finish_maintenance_cycle(&mut self) {
        self.maintenance_cursor = None;
        self.maintenance_cycle_active = false;
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayload, InlineSpan, RichBlockKind};

    use super::*;

    fn payload(block_id: BlockId, version: u64, text: &str) -> BlockPayloadRecord {
        let mut payload = BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, text);
        payload.content_version = version;
        payload
    }

    fn entry_policy(max_entries: usize) -> PayloadCachePolicy {
        PayloadCachePolicy {
            max_entries,
            max_estimated_bytes: usize::MAX,
        }
    }

    #[test]
    fn byte_accounting_refreshes_only_the_bounded_dirty_set() {
        let mut window = PayloadWindow::new(0..0);
        for block_id in 1..=10 {
            window.insert_loaded(payload(block_id, 1, "short"));
            let record = window.get_mut(block_id).unwrap();
            record.content_version = 2;
            record.payload = BlockPayload::RichText {
                spans: vec![InlineSpan::plain("x".repeat(8 * 1024))],
            };
        }
        let before = window.total_estimated_bytes();
        let budget = PayloadCacheMaintenanceBudget {
            max_byte_estimate_refreshes: 3,
            max_lru_candidates: 1,
            max_evictions: 1,
        };

        let first = window.maintain_cache_slice(entry_policy(usize::MAX), budget, |_, _, _| true);

        assert_eq!(first.byte_estimates_refreshed, 3);
        assert!(first.maintenance_pending);
        assert!(window.total_estimated_bytes() > before);

        let mut total_refreshed = first.byte_estimates_refreshed;
        let mut pending = first.maintenance_pending;
        while pending {
            let slice =
                window.maintain_cache_slice(entry_policy(usize::MAX), budget, |_, _, _| true);
            assert!(slice.byte_estimates_refreshed <= 3);
            total_refreshed += slice.byte_estimates_refreshed;
            pending = slice.maintenance_pending;
        }
        assert_eq!(total_refreshed, 10);
    }

    #[test]
    fn candidate_and_eviction_counts_are_bounded_per_slice() {
        let mut window = PayloadWindow::new(0..0);
        for block_id in 1..=30 {
            window.insert_loaded(payload(block_id, 1, "payload"));
        }
        let budget = PayloadCacheMaintenanceBudget {
            max_byte_estimate_refreshes: 4,
            max_lru_candidates: 5,
            max_evictions: 3,
        };

        let first = window.maintain_cache_slice(entry_policy(10), budget, |_, _, _| true);

        assert_eq!(first.evicted.len(), 3);
        assert_eq!(first.lru_candidates_examined, 3);
        assert!(first.maintenance_pending);

        let mut pending = first.maintenance_pending;
        while pending {
            let slice = window.maintain_cache_slice(entry_policy(10), budget, |_, _, _| true);
            assert!(slice.evicted.len() <= 3);
            assert!(slice.lru_candidates_examined <= 5);
            pending = slice.maintenance_pending;
        }
        assert_eq!(window.payloads.len(), 10);
    }

    #[test]
    fn an_all_protected_cache_reaches_a_terminal_slice() {
        let mut window = PayloadWindow::new(0..0);
        for block_id in 1..=23 {
            window.insert_loaded(payload(block_id, 1, "payload"));
        }
        let budget = PayloadCacheMaintenanceBudget {
            max_byte_estimate_refreshes: 4,
            max_lru_candidates: 4,
            max_evictions: 4,
        };
        let mut examined = 0;
        let mut slices = 0;

        loop {
            let slice = window.maintain_cache_slice(entry_policy(0), budget, |_, _, _| false);
            slices += 1;
            examined += slice.lru_candidates_examined;
            assert!(slice.lru_candidates_examined <= 4);
            assert!(slice.evicted.is_empty());
            if !slice.maintenance_pending {
                break;
            }
            assert!(slices < 10, "protected candidates must not spin forever");
        }

        assert_eq!(examined, 23);
        assert_eq!(window.payloads.len(), 23);

        let retry = window.maintain_cache_slice(
            entry_policy(0),
            PayloadCacheMaintenanceBudget::unbounded(),
            |_, _, _| true,
        );
        assert_eq!(retry.evicted.len(), 23);
        assert!(!retry.maintenance_pending);
    }

    #[test]
    fn dirty_payloads_also_end_the_scan_without_self_rescheduling() {
        let mut window = PayloadWindow::new(0..0);
        for block_id in 1..=12 {
            window.insert(payload(block_id, 1, "unsaved"));
        }
        let budget = PayloadCacheMaintenanceBudget {
            max_byte_estimate_refreshes: 4,
            max_lru_candidates: 5,
            max_evictions: 5,
        };
        let mut slices = 0;

        loop {
            let slice = window.maintain_cache_slice(entry_policy(0), budget, |_, _, dirty| !dirty);
            slices += 1;
            if !slice.maintenance_pending {
                break;
            }
            assert!(slices < 10, "dirty candidates must not spin forever");
        }

        assert_eq!(window.payloads.len(), 12);

        let versions = (1..=12).map(|block_id| (block_id, 1)).collect::<Vec<_>>();
        window.mark_persisted_versions(&versions);
        let retry = window.maintain_cache_slice(
            entry_policy(0),
            PayloadCacheMaintenanceBudget::unbounded(),
            |_, _, dirty| !dirty,
        );
        assert_eq!(retry.evicted.len(), 12);
        assert!(!retry.maintenance_pending);
    }
}
