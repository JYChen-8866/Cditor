use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBudget {
    pub max_tokens: usize,
    pub used_tokens: usize,
}

impl AgentBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            used_tokens: 0,
        }
    }

    pub fn remaining(&self) -> usize {
        self.max_tokens.saturating_sub(self.used_tokens)
    }

    pub fn consume(&mut self, tokens: usize) -> bool {
        if self.used_tokens.saturating_add(tokens) > self.max_tokens {
            return false;
        }
        self.used_tokens += tokens;
        true
    }

    pub fn is_exhausted(&self) -> bool {
        self.used_tokens >= self.max_tokens
    }
}
