use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::sync::Arc;

use cditor_core::ids::BlockId;
use cditor_core::rich_text::BlockPayloadRecord;

use super::payload_cache::estimated_payload_record_bytes;
use super::payload_preparation::{PreparedPayloadRecord, prepare_payload_records};

mod cache_maintenance;

pub const MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS: u8 = 3;
/// Failed payloads retain their diagnostic string and retry counter. A broken
/// document can otherwise add one entry for every block visited during a
/// session, even after those blocks have left the active payload window.
pub const MAX_PAYLOAD_WINDOW_FAILURES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PayloadLoadPriority {
    Prefetch,
    Visible,
    Emergency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PayloadLoadOwner {
    generation: u64,
    priority: PayloadLoadPriority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadWindowLoadRequest {
    pub generation: u64,
    pub block_range: Range<usize>,
    pub block_ids: Vec<BlockId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PayloadWindowLoadResult {
    pub request: PayloadWindowLoadRequest,
    pub records: Vec<PreparedPayloadRecord>,
    pub missing_block_ids: Vec<BlockId>,
}

impl PayloadWindowLoadResult {
    pub fn prepare(
        request: PayloadWindowLoadRequest,
        records: Vec<BlockPayloadRecord>,
        missing_block_ids: Vec<BlockId>,
    ) -> Self {
        Self {
            request,
            records: prepare_payload_records(records),
            missing_block_ids,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadWindowApplyDecision {
    Applied,
    DiscardedStaleGeneration { expected: u64, actual: u64 },
}

#[derive(Debug, Clone, Default)]
pub struct PayloadWindow {
    pub block_range: Range<usize>,
    pub payloads: HashMap<BlockId, Arc<BlockPayloadRecord>>,
    pub loading: HashSet<BlockId>,
    loading_generations: HashMap<BlockId, PayloadLoadOwner>,
    pub failed: HashMap<BlockId, String>,
    pub failure_attempts: HashMap<BlockId, u8>,
    failure_order: VecDeque<BlockId>,
    persisted_versions: HashMap<BlockId, u64>,
    last_access: HashMap<BlockId, u64>,
    access_order: BTreeSet<(u64, BlockId)>,
    access_clock: u64,
    estimated_bytes_by_block: HashMap<BlockId, usize>,
    estimated_bytes_dirty: HashSet<BlockId>,
    estimated_bytes_dirty_queue: VecDeque<BlockId>,
    total_estimated_bytes: usize,
    maintenance_cursor: Option<(u64, BlockId)>,
    maintenance_cycle_active: bool,
    residency_revision: u64,
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
            failure_order: VecDeque::new(),
            persisted_versions: HashMap::new(),
            last_access: HashMap::new(),
            access_order: BTreeSet::new(),
            access_clock: 0,
            estimated_bytes_by_block: HashMap::new(),
            estimated_bytes_dirty: HashSet::new(),
            estimated_bytes_dirty_queue: VecDeque::new(),
            total_estimated_bytes: 0,
            maintenance_cursor: None,
            maintenance_cycle_active: false,
            residency_revision: 0,
        }
    }

    /// Inserts a local record while preserving the last known persisted version.
    /// New records and records whose version changed therefore remain dirty.
    pub fn insert(&mut self, payload: BlockPayloadRecord) {
        let estimated_bytes = estimated_payload_record_bytes(&payload);
        self.insert_shared(Arc::new(payload), estimated_bytes);
    }

    fn insert_shared(&mut self, payload: Arc<BlockPayloadRecord>, estimated_bytes: usize) {
        let block_id = payload.block_id;
        let was_resident = self.payloads.contains_key(&block_id);
        self.loading.remove(&block_id);
        self.loading_generations.remove(&block_id);
        self.clear_failure(block_id);
        self.replace_estimated_size(block_id, estimated_bytes);
        self.clear_estimated_size_dirty(block_id);
        self.payloads.insert(block_id, payload);
        if !was_resident {
            self.residency_revision = self.residency_revision.saturating_add(1);
        }
        self.touch(block_id);
    }

    /// Inserts a record whose current content version is known to be durable.
    pub fn insert_loaded(&mut self, payload: BlockPayloadRecord) {
        let block_id = payload.block_id;
        let content_version = payload.content_version;
        self.insert(payload);
        self.persisted_versions.insert(block_id, content_version);
    }

    /// Inserts a storage record whose expensive preparation already completed
    /// off the main thread.
    pub fn insert_loaded_prepared(&mut self, payload: PreparedPayloadRecord) {
        let block_id = payload.block_id();
        let content_version = payload.content_version();
        let (record, estimated_bytes) = payload.into_parts();
        self.insert_shared(record, estimated_bytes);
        self.persisted_versions.insert(block_id, content_version);
    }

    pub fn get(&self, block_id: BlockId) -> Option<&BlockPayloadRecord> {
        self.payloads.get(&block_id).map(Arc::as_ref)
    }

    pub fn get_shared(&self, block_id: BlockId) -> Option<&Arc<BlockPayloadRecord>> {
        self.payloads.get(&block_id)
    }

    pub fn get_mut(&mut self, block_id: BlockId) -> Option<&mut BlockPayloadRecord> {
        if !self.payloads.contains_key(&block_id) {
            return None;
        }
        self.mark_estimated_size_dirty(block_id);
        self.restart_cache_maintenance_cycle();
        self.payloads.get_mut(&block_id).map(Arc::make_mut)
    }

    pub fn remove(&mut self, block_id: BlockId) -> Option<Arc<BlockPayloadRecord>> {
        self.remove_internal(block_id)
    }

    fn remove_internal(&mut self, block_id: BlockId) -> Option<Arc<BlockPayloadRecord>> {
        self.loading.remove(&block_id);
        self.loading_generations.remove(&block_id);
        self.clear_failure(block_id);
        self.persisted_versions.remove(&block_id);
        if let Some(stamp) = self.last_access.remove(&block_id) {
            self.access_order.remove(&(stamp, block_id));
        }
        self.clear_estimated_size_dirty(block_id);
        if let Some(bytes) = self.estimated_bytes_by_block.remove(&block_id) {
            self.total_estimated_bytes = self.total_estimated_bytes.saturating_sub(bytes);
        }
        let removed = self.payloads.remove(&block_id);
        if removed.is_some() {
            self.residency_revision = self.residency_revision.saturating_add(1);
        }
        removed
    }

    pub fn touch(&mut self, block_id: BlockId) {
        if !self.payloads.contains_key(&block_id) {
            return;
        }
        self.access_clock = self.access_clock.saturating_add(1);
        let stamp = self.access_clock;
        if let Some(previous) = self.last_access.insert(block_id, stamp) {
            self.access_order.remove(&(previous, block_id));
        }
        self.access_order.insert((stamp, block_id));
    }

    pub fn mark_persisted_versions(&mut self, versions: &[(BlockId, u64)]) {
        let mut changed = false;
        for &(block_id, content_version) in versions {
            if self.payloads.contains_key(&block_id) {
                changed |= self.persisted_versions.insert(block_id, content_version)
                    != Some(content_version);
            }
        }
        if changed {
            self.restart_cache_maintenance_cycle();
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

    pub fn residency_revision(&self) -> u64 {
        self.residency_revision
    }

    pub fn mark_loading(&mut self, block_id: BlockId, generation: u64) {
        self.mark_loading_with_priority(block_id, generation, PayloadLoadPriority::Visible);
    }

    pub fn mark_loading_with_priority(
        &mut self,
        block_id: BlockId,
        generation: u64,
        priority: PayloadLoadPriority,
    ) {
        self.loading.insert(block_id);
        self.loading_generations.insert(
            block_id,
            PayloadLoadOwner {
                generation,
                priority,
            },
        );
    }

    pub fn loading_priority(&self, block_id: BlockId) -> Option<PayloadLoadPriority> {
        self.loading_generations
            .get(&block_id)
            .map(|owner| owner.priority)
    }

    pub fn finish_loading(&mut self, block_id: BlockId, generation: u64) -> bool {
        if self
            .loading_generations
            .get(&block_id)
            .map(|owner| owner.generation)
            != Some(generation)
        {
            return false;
        }
        self.loading.remove(&block_id);
        self.loading_generations.remove(&block_id);
        true
    }

    pub fn cancel_loading_generation(&mut self, generation: u64) -> usize {
        let block_ids = self
            .loading_generations
            .iter()
            .filter_map(|(&block_id, owner)| (owner.generation == generation).then_some(block_id))
            .collect::<Vec<_>>();
        for block_id in &block_ids {
            self.loading.remove(block_id);
            self.loading_generations.remove(block_id);
        }
        block_ids.len()
    }

    pub fn mark_failed(&mut self, block_id: BlockId, message: impl Into<String>) {
        self.loading.remove(&block_id);
        self.loading_generations.remove(&block_id);
        if !self.failed.contains_key(&block_id) {
            while self.failed.len() >= MAX_PAYLOAD_WINDOW_FAILURES {
                if !self.evict_oldest_failure(Some(block_id)) {
                    break;
                }
            }
            self.failure_order
                .retain(|candidate| *candidate != block_id);
            self.failure_order.push_back(block_id);
        }
        self.failed.insert(block_id, message.into());
        while self.failed.len() > MAX_PAYLOAD_WINDOW_FAILURES {
            if !self.evict_oldest_failure(Some(block_id)) {
                break;
            }
        }
        let attempts = self.failure_attempts.entry(block_id).or_default();
        *attempts = attempts.saturating_add(1);
    }

    pub fn can_retry(&self, block_id: BlockId) -> bool {
        self.failure_attempts.get(&block_id).copied().unwrap_or(0)
            < MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS
    }

    pub(crate) fn clear_failure(&mut self, block_id: BlockId) -> bool {
        let had_failure =
            self.failed.contains_key(&block_id) || self.failure_attempts.contains_key(&block_id);
        self.failed.remove(&block_id);
        self.failure_attempts.remove(&block_id);
        self.failure_order
            .retain(|candidate| *candidate != block_id);
        had_failure
    }

    fn evict_oldest_failure(&mut self, protected: Option<BlockId>) -> bool {
        while let Some(candidate) = self.failure_order.pop_front() {
            if Some(candidate) == protected {
                continue;
            }
            if self.failed.remove(&candidate).is_some() {
                self.failure_attempts.remove(&candidate);
                return true;
            }
        }
        let candidate = self
            .failed
            .keys()
            .copied()
            .find(|candidate| Some(*candidate) != protected);
        if let Some(candidate) = candidate {
            self.failed.remove(&candidate);
            self.failure_attempts.remove(&candidate);
            self.failure_order.retain(|queued| *queued != candidate);
            return true;
        }
        false
    }

    fn replace_estimated_size(&mut self, block_id: BlockId, bytes: usize) {
        if let Some(previous) = self.estimated_bytes_by_block.insert(block_id, bytes) {
            self.total_estimated_bytes = self.total_estimated_bytes.saturating_sub(previous);
        }
        self.total_estimated_bytes = self.total_estimated_bytes.saturating_add(bytes);
    }

    fn mark_estimated_size_dirty(&mut self, block_id: BlockId) {
        if self.estimated_bytes_dirty.insert(block_id) {
            self.estimated_bytes_dirty_queue.push_back(block_id);
        }
    }

    fn clear_estimated_size_dirty(&mut self, block_id: BlockId) {
        self.estimated_bytes_dirty.remove(&block_id);
        if self.estimated_bytes_dirty.is_empty() {
            self.estimated_bytes_dirty_queue.clear();
        }
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
    fn cancelling_a_generation_only_releases_its_loading_markers() {
        let mut window = PayloadWindow::new(0..3);
        window.mark_loading(1, 7);
        window.mark_loading(2, 7);
        window.mark_loading(3, 8);

        assert_eq!(window.cancel_loading_generation(7), 2);
        assert!(!window.loading.contains(&1));
        assert!(!window.loading.contains(&2));
        assert!(window.loading.contains(&3));
        assert!(!window.finish_loading(1, 7));
        assert!(window.finish_loading(3, 8));
    }

    #[test]
    fn higher_priority_owner_can_replace_prefetch_without_accepting_its_late_result() {
        let mut window = PayloadWindow::new(0..1);
        window.mark_loading_with_priority(1, 7, PayloadLoadPriority::Prefetch);
        window.mark_loading_with_priority(1, 8, PayloadLoadPriority::Visible);

        assert_eq!(
            window.loading_priority(1),
            Some(PayloadLoadPriority::Visible)
        );
        assert!(!window.finish_loading(1, 7));
        assert!(window.finish_loading(1, 8));
    }

    #[test]
    fn eviction_is_lru_and_skips_protected_records() {
        let mut window = PayloadWindow::new(0..0);
        window.insert_loaded(payload(1, 1, "one"));
        window.insert_loaded(payload(2, 1, "two"));
        window.insert_loaded(payload(3, 1, "three"));

        let evicted = window.evict_to_limits(2, usize::MAX, |block_id, _, _| block_id != 1);
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
                .evict_to_limits(0, usize::MAX, |_, _, _| true)
                .is_empty()
        );
    }

    #[test]
    fn residency_revision_changes_only_when_membership_changes() {
        let mut window = PayloadWindow::new(0..0);
        assert_eq!(window.residency_revision(), 0);

        window.insert_loaded(payload(1, 1, "one"));
        let inserted_revision = window.residency_revision();
        assert!(inserted_revision > 0);

        window.insert_loaded(payload(1, 2, "updated"));
        assert_eq!(window.residency_revision(), inserted_revision);
        assert!(window.remove(99).is_none());
        assert_eq!(window.residency_revision(), inserted_revision);

        assert!(window.remove(1).is_some());
        assert!(window.residency_revision() > inserted_revision);
    }

    #[test]
    fn prepared_storage_record_moves_its_arc_and_byte_estimate_into_residency() {
        let mut window = PayloadWindow::new(0..1);
        let prepared = PreparedPayloadRecord::prepare(payload(1, 7, &"x".repeat(8_192)));
        let expected_bytes = prepared.estimated_bytes();
        let shared = prepared.record().clone();

        window.insert_loaded_prepared(prepared);

        assert!(Arc::ptr_eq(window.get_shared(1).unwrap(), &shared));
        assert_eq!(window.total_estimated_bytes(), expected_bytes);
        assert!(!window.is_dirty(1));
    }

    #[test]
    fn batch_eviction_scans_an_old_protected_set_only_once() {
        let mut window = PayloadWindow::new(0..0);
        for block_id in 1..=200 {
            window.insert_loaded(payload(block_id, 1, "payload"));
        }
        let mut predicate_calls = 0;

        let evicted = window.evict_to_limits(100, usize::MAX, |block_id, _, _| {
            predicate_calls += 1;
            block_id > 100
        });

        assert_eq!(evicted.len(), 100);
        assert_eq!(predicate_calls, 200);
        assert!((1..=100).all(|block_id| window.get(block_id).is_some()));
    }

    #[test]
    fn failed_payload_diagnostics_are_bounded_and_keep_latest_failure() {
        let mut window = PayloadWindow::new(0..0);
        for block_id in 1..=(MAX_PAYLOAD_WINDOW_FAILURES as u64 + 1) {
            window.mark_failed(block_id, format!("failure-{block_id}"));
        }

        assert_eq!(window.failed.len(), MAX_PAYLOAD_WINDOW_FAILURES);
        assert!(!window.failed.contains_key(&1));
        assert_eq!(
            window.failed.get(&(MAX_PAYLOAD_WINDOW_FAILURES as u64 + 1)),
            Some(&format!(
                "failure-{}",
                MAX_PAYLOAD_WINDOW_FAILURES as u64 + 1
            ))
        );
        assert_eq!(
            window
                .failure_attempts
                .get(&(MAX_PAYLOAD_WINDOW_FAILURES as u64 + 1)),
            Some(&1)
        );
    }
}
