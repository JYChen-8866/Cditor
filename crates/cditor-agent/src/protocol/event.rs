use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolProgress {
    pub tool_call_id: u64,
    pub tool: String,
    pub phase: String,
    pub message: String,
    pub blocks_processed: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultSummary {
    pub tool_call_id: u64,
    pub tool: String,
    pub success: bool,
    pub blocks_read: Option<usize>,
    pub blocks_written: Option<usize>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentEvent {
    Thinking {
        turn_id: u64,
        text: String,
    },
    Content {
        turn_id: u64,
        text: String,
    },
    ToolCall {
        tool_call_id: u64,
        tool: String,
        params: serde_json::Value,
    },
    ToolProgress(ToolProgress),
    ToolResult(ToolResultSummary),
    Confirmation {
        tool_call_id: u64,
        summary: String,
    },
    Snapshot {
        document_id: u64,
        structure_version: u64,
    },
    Done {
        turn_id: u64,
    },
    Error {
        turn_id: u64,
        message: String,
    },
}
