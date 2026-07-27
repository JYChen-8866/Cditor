use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compactor {
    pub compact_at_tokens: usize,
    pub compaction_count: usize,
}

impl Compactor {
    pub fn new(compact_at_tokens: usize) -> Self {
        Self {
            compact_at_tokens,
            compaction_count: 0,
        }
    }

    pub fn should_compact(&self, current_tokens: usize) -> bool {
        current_tokens >= self.compact_at_tokens
    }
}
