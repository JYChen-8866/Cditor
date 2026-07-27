use serde::{Deserialize, Serialize};

use crate::protocol::context::AgentContextSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTurn {
    pub turn_id: u64,
    pub user_message: String,
    pub context_snapshot: Option<AgentContextSnapshot>,
    pub tool_calls: Vec<TurnToolCall>,
    pub response_text: Option<String>,
    pub token_count: usize,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnToolCall {
    pub call_id: u64,
    pub tool: String,
    pub params: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
}
