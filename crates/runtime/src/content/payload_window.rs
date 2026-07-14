use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::ops::Range;

use cditor_core::ids::BlockId;
use cditor_core::rich_text::BlockPayloadRecord;

use super::payload_cache::estimated_payload_record_bytes;

pub const MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadWindowLoadRequest {
    pub generation: u64,
    pub block_range: Range<usize>,
    pub block_ids: Vec<BlockId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PayloadWindowLoadResult {
    pub request: PayloadWindowLoadRequest,
    pub records: Vec<BlockPayloadRecord>,
    pub missing_block_ids: Vec<BlockId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadWindowApplyDecision {
    Applied,
    DiscardedStaleGeneration { expected: u64, actual: u64 },
}

#[derive(Debug, Clone, Default)]
pub struct PayloadWindow {
    pub block_range: Range<usize>,
    pub payloads: HashMap<BlockId, BlockPayloadRecord>,
    pub loading: HashSet<BlockId>,
    loading_generations: HashMap<BlockId, u64>,
    pub failed: HashMap<BlockId, String>,
    pub failure_attempts: HashMap<BlockId, u8>,
    persisted_versions: HashMap<BlockId, u64>,
    last_access: HashMap<BlockId, u64>,
    access_order: BinaryHeap<Reverse<(u64, BlockId)>>,
    access_clock: u64,
    estimated_bytes_by_block: HashMap<BlockId, usize>,
    total_estimated_bytes: usize,
}

impl PayloadWindow {
    pub fn new(block_range: Range<usize>) -> Self {
        Self {
            block_range,
            payloads: HashMap::new(),
            loading: HashSet::new(),
            loading_generations: HashMap::new(),
            failed: HashMap::new(),
            failure_attempts: HashMap::new(),
            persisted_versions: HashMap::new(),
            last_access: HashMap::new(),
            access_order: BinaryHeap::new(),
            access_clock: 0,
            estimated_bytes_by_block: HashMap::new(),
            total_estimated_bytes: 0,
        }
    }

    /// Inserts a local record while preserving the last known persisted version.
    /// New records and records whose version changed therefore remain dirty.
    pub fn insert(&mut self, payload: BlockPayloadRecord) {
        let block_id = payload.block_id;
        self.loading.remove(&payload.block_id);
        self.loading_generations.remove(&payload.block_id);
        self.failed.remove(&payload.block_id);
        self.failure_attempts.remove(&payload.block_id);
        self.replace_estimated_size(block_id, estimated_payload_record_bytes(&payload));
        self.payloads.insert(block_id, payload);
        self.touch(block_id);
    }

    /// Inserts a record whose current content version is known to be durable.
    pub fn insert_loaded(&mut self, payload: BlockPayloadRecord) {
        let block_id = payload.block_id;
        let content_version = payload.content_version;
        self.insert(payload);
        self.persisted_versions.insert(block_id, content_version);
    }

    pub fn get(&self, block_id: BlockId) -> Option<&BlockPayloadRecord> {
        self.payloads.get(&block_id)
    }

    pub fn remove(&mut self, block_id: BlockId) -> Option<BlockPayloadRecord> {
        self.remove_internal(block_id, true)
    }

    fn remove_internal(
        &mut self,
        block_id: BlockId,
        compact_access_order: bool,
    ) -> Option<BlockPayloadRecord> {
        self.loading.remove(&block_id);
        self.loading_generations.remove(&block_id);
        self.failed.remove(&block_id);
        self.failure_attempts.remove(&block_id);
        self.persisted_versions.remove(&block_id);
        self.last_access.remove(&block_id);
        if let Some(bytes) = self.estimated_bytes_by_block.remove(&block_id) {
            self.total_estimated_bytes = self.total_estimated_bytes.saturating_sub(bytes);
        }
        let removed = self.payloads.remove(&block_id);
        if compact_access_order {
            self.compact_access_order_if_needed();
        }
        removed
    }

    pub fn touch(&mut self, block_id: BlockId) {
        if !self.payloads.contains_key(&block_id) {
            return;
        }
        self.access_clock = self.access_clock.saturating_add(1);
        let stamp = self.access_clock;
        self.last_access.insert(block_id, stamp);
        self.access_order.push(Reverse((stamp, block_id)));
        self.compact_access_order_if_needed();
    }

    pub fn mark_persisted_versions(&mut self, versions: &[(BlockId, u64)]) {
        for &(block_id, content_version) in versions {
            if self.payloads.contains_key(&block_id) {
                self.persisted_versions.insert(block_id, content_version);
            }
        }
    }

    pub fn is_dirty(&self, block_id: BlockId) -> bool {
        let Some(payload) = self.payloads.get(&block_id) else {
            return false;
        };
        self.persisted_versions.get(&block_id).copied() != Some(payload.content_version)
    }

    pub fn total_estimated_bytes(&self) -> usize {
        self.total_estimated_bytes
    }

    /// Recalculates sizes at a cache-maintenance boundary. Edits may mutate a
    /// loaded record in place, so input handling stays allocation-free and the
    /// less frequent trim pass accounts for the new capacity.
    pub fn refresh_estimated_bytes(&mut self) {
        self.estimated_bytes_by_block.clear();
        self.total_estimated_bytes = 0;
        for (&block_id, payload) in &self.payloads {
            let bytes = estimated_payload_record_bytes(payload);
            self.estimated_bytes_by_block.insert(block_id, bytes);
            self.total_estimated_bytes = self.total_estimated_bytes.saturating_add(bytes);
        }
    }

    /// Evicts a batch in one LRU scan. Pinned or dirty candidates are deferred
    /// until the whole pass finishes, avoiding repeated scans when a large
    /// protected set is older than the clean records being released.
    pub fn evict_to_limits(
        &mut self,
        max_entries: usize,
        max_estimated_bytes: usize,
        mut can_evict: impl FnMut(BlockId, &BlockPayloadRecord) -> bool,
    ) -> Vec<BlockPayloadRecord> {
        let mut evicted = Vec::new();
        let mut protected = Vec::new();
        while self.payloads.len() > max_entries || self.total_estimated_bytes > max_estimated_bytes
        {
            let mut candidate = None;
            while let Some(Reverse((stamp, block_id))) = self.access_order.pop() {
                if self.last_access.get(&block_id).copied() != Some(stamp) {
                    continue;
                }
                let Some(payload) = self.payloads.get(&block_id) else {
                    self.last_access.remove(&block_id);
                    continue;
                };
                if !can_evict(block_id, payload) {
                    protected.push(Reverse((stamp, block_id)));
                    continue;
                }
                candidate = Some(block_id);
                break;
            }
            let Some(block_id) = candidate else {
                break;
            };
            if let Some(payload) = self.remove_internal(block_id, false) {
                evicted.push(payload);
            }
        }
        self.access_order.extend(protected);
        self.compact_access_order_if_needed();
        evicted
    }

    pub fn mark_loading(&mut self, block_id: BlockId, generation: u64) {
        self.loading.insert(block_id);
        self.loading_generations.insert(block_id, generation);
    }

    pub fn finish_loading(&mut self, block_id: BlockId, generation: u64) -> bool {
        if self.loading_generations.get(&block_id).copied() != Some(generation) {
            return false;
        }
        self.loading.remove(&block_id);
        self.loading_generations.remove(&block_id);
        true
    }

    pub fn mark_failed(&mut self, block_id: BlockId, message: impl Into<String>) {
        self.loading.remove(&block_id);
        self.loading_generations.remove(&block_id);
        self.failed.insert(block_id, message.into());
        let attempts = self.failure_attempts.entry(block_id).or_default();
        *attempts = attempts.saturating_add(1);
    }

    pub fn can_retry(&self, block_id: BlockId) -> bool {
        self.failure_attempts.get(&block_id).copied().unwrap_or(0)
            < MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS
    }

    fn replace_estimated_size(&mut self, block_id: BlockId, bytes: usize) {
        if let Some(previous) = self.estimated_bytes_by_block.insert(block_id, bytes) {
            self.total_estimated_bytes = self.total_estimated_bytes.saturating_sub(previous);
        }
        self.total_estimated_bytes = self.total_estimated_bytes.saturating_add(bytes);
    }

    fn compact_access_order_if_needed(&mut self) {
        let max_len = self.last_access.len().saturating_mul(4).saturating_add(64);
        if self.access_order.len() <= max_len {
            return;
        }
        self.access_order = self
            .last_access
            .iter()
            .map(|(&block_id, &stamp)| Reverse((stamp, block_id)))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::RichBlockKind;

    fn payload(block_id: BlockId, version: u64, text: &str) -> BlockPayloadRecord {
        let mut payload = BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, text);
        payload.content_version = version;
        payload
    }

    #[test]
    fn loaded_and_saved_versions_distinguish_clean_from_dirty_records() {
        let mut window = PayloadWindow::new(0..1);
        window.insert_loaded(payload(1, 1, "one"));
        assert!(!window.is_dirty(1));

        window.insert(payload(1, 2, "two"));
        assert!(window.is_dirty(1));
        window.mark_persisted_versions(&[(1, 1)]);
        assert!(
            window.is_dirty(1),
            "an older save cannot clean a newer edit"
        );
        window.mark_persisted_versions(&[(1, 2)]);
        assert!(!window.is_dirty(1));
    }

    #[test]
    fn eviction_is_lru_and_skips_protected_records() {
        let mut window = PayloadWindow::new(0..0);
        window.insert_loaded(payload(1, 1, "one"));
        window.insert_loaded(payload(2, 1, "two"));
        window.insert_loaded(payload(3, 1, "three"));

        let evicted = window.evict_to_limits(2, usize::MAX, |block_id, _| block_id != 1);
        assert_eq!(evicted[0].block_id, 2);
        assert!(window.get(1).is_some());
        assert!(window.get(2).is_none());
    }

    #[test]
    fn removal_cleans_size_and_version_metadata() {
        let mut window = PayloadWindow::new(0..0);
        window.insert_loaded(payload(1, 1, &"x".repeat(1_024)));
        assert!(window.total_estimated_bytes() > 0);

        assert!(window.remove(1).is_some());
        assert_eq!(window.total_estimated_bytes(), 0);
        assert!(!window.is_dirty(1));
        assert!(
            window
                .evict_to_limits(0, usize::MAX, |_, _| true)
                .is_empty()
        );
    }

    #[test]
    fn batch_eviction_scans_an_old_protected_set_only_once() {
        let mut window = PayloadWindow::new(0..0);
        for block_id in 1..=200 {
            window.insert_loaded(payload(block_id, 1, "payload"));
        }
        let mut predicate_calls = 0;

        let evicted = window.evict_to_limits(100, usize::MAX, |block_id, _| {
            predicate_calls += 1;
            block_id > 100
        });

        assert_eq!(evicted.len(), 100);
        assert_eq!(predicate_calls, 200);
        assert!((1..=100).all(|block_id| window.get(block_id).is_some()));
    }
}
