use std::collections::{HashMap, HashSet};
use std::ops::Range;

use cditor_core::ids::BlockId;
use cditor_core::rich_text::BlockPayloadRecord;

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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PayloadWindow {
    pub block_range: Range<usize>,
    pub payloads: HashMap<BlockId, BlockPayloadRecord>,
    pub loading: HashSet<BlockId>,
    pub failed: HashMap<BlockId, String>,
}

impl PayloadWindow {
    pub fn new(block_range: Range<usize>) -> Self {
        Self {
            block_range,
            payloads: HashMap::new(),
            loading: HashSet::new(),
            failed: HashMap::new(),
        }
    }

    pub fn insert(&mut self, payload: BlockPayloadRecord) {
        self.loading.remove(&payload.block_id);
        self.failed.remove(&payload.block_id);
        self.payloads.insert(payload.block_id, payload);
    }

    pub fn get(&self, block_id: BlockId) -> Option<&BlockPayloadRecord> {
        self.payloads.get(&block_id)
    }

    pub fn mark_loading(&mut self, block_id: BlockId) {
        self.loading.insert(block_id);
    }

    pub fn mark_failed(&mut self, block_id: BlockId, message: impl Into<String>) {
        self.loading.remove(&block_id);
        self.failed.insert(block_id, message.into());
    }
}
