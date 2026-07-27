use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum AgentErrorCode {
    #[error("block not found: {0}")]
    BlockNotFound(u64),
    #[error("document not found: {0}")]
    DocumentNotFound(u64),
    #[error("document not loaded")]
    DocumentNotLoaded,
    #[error("invalid markdown: {0}")]
    InvalidMarkdown(String),
    #[error("structure constraint violation: {0}")]
    StructureConstraint(String),
    #[error("content version mismatch: block {block_id}, expected {expected}, actual {actual}")]
    ContentVersionMismatch {
        block_id: u64,
        expected: u64,
        actual: u64,
    },
    #[error("structure version mismatch: expected {expected}, actual {actual}")]
    StructureVersionMismatch { expected: u64, actual: u64 },
    #[error("mutation conflict: {reason}")]
    MutationConflict { reason: String },
    #[error("tool timeout")]
    Timeout,
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    #[error("budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("cancelled")]
    Cancelled,
    #[error("internal: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("tool '{tool}' failed: {error}")]
pub struct AgentToolError {
    pub tool: String,
    pub error: AgentErrorCode,
}
