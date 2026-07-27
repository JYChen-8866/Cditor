use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoomLoopDetector {
    pub warn_threshold: usize,
    pub stop_threshold: usize,
    signatures: HashMap<String, usize>,
}

impl DoomLoopDetector {
    pub fn new(warn_threshold: usize) -> Self {
        Self {
            warn_threshold,
            stop_threshold: warn_threshold.saturating_add(2),
            signatures: HashMap::new(),
        }
    }

    pub fn record(&mut self, tool_name: &str, params_key: &str) -> DoomLoopAction {
        let signature = format!("{tool_name}:{params_key}");
        let count = self.signatures.entry(signature).or_insert(0);
        *count += 1;
        let count = *count;

        if count >= self.stop_threshold {
            DoomLoopAction::Stop
        } else if count >= self.warn_threshold {
            DoomLoopAction::Warn
        } else {
            DoomLoopAction::None
        }
    }

    pub fn reset(&mut self) {
        self.signatures.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoomLoopAction {
    None,
    Warn,
    Stop,
}
