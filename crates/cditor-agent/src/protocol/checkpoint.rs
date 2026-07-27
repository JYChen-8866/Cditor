use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCheckpoint {
    pub turn_count: u64,
    pub cursor_token_count: u64,
    pub tool_call_count: u64,
    pub mutation_count: u64,
    pub committed: bool,
}
