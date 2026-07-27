use crate::tools::mutation::PreparedMutationId;
use crate::{AgentErrorCode, AgentSnapshotId, JsonValue, ToolCallId, TurnId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgress {
    pub call_id: ToolCallId,
    pub message: String,
    pub fraction: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultSummary {
    pub call_id: ToolCallId,
    pub name: String,
    pub truncated: bool,
    pub estimated_tokens: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Turn anchor — emitted IMMEDIATELY on begin for frontend recovery.
    Turn {
        turn_id: TurnId,
        base_revision: i64,
    },
    /// User-readable error (non-fatal).
    Error {
        error: String,
    },
    /// Normal interruption (saveTurn "interrupted"), not a Failure.
    TurnInterrupted {
        turn_id: TurnId,
        reason: String,
    },
    TurnStarted {
        turn_id: TurnId,
        context: AgentSnapshotId,
    },
    ModelTextDelta {
        turn_id: TurnId,
        delta: String,
    },
    /// Extended-thinking delta (carried in thinking_content).
    ThinkingDelta {
        delta: String,
    },
    ThinkingDone,
    ToolCallStarted {
        call_id: ToolCallId,
        name: String,
        safe_args: JsonValue,
    },
    ToolCallProgress {
        call_id: ToolCallId,
        progress: ToolProgress,
    },
    ToolCallFinished {
        call_id: ToolCallId,
        summary: ToolResultSummary,
    },
    /// Tool confirm/cancel result relayed back from frontend.
    ConfirmResult {
        prepared_id: PreparedMutationId,
        approved: bool,
        always: bool,
    },
    /// Model asked a question (question tool).
    Question {
        question_id: String,
        title: String,
        options: Vec<QuestionOption>,
    },
    ConfirmationRequired {
        prepared_id: PreparedMutationId,
    },
    MutationCommitted {
        result: JsonValue,
    },
    MutationConflict {
        prepared_id: PreparedMutationId,
    },
    ContextCompacted {
        before_tokens: u32,
        after_tokens: u32,
    },
    Usage {
        prompt_tokens: u64,
        cached_tokens: u64,
        completion_tokens: u64,
    },
    Failed {
        code: AgentErrorCode,
        message: String,
        retryable: bool,
        /// When true, the entire session is compromised (SiYuan's criticalFailure).
        #[serde(default)]
        critical: bool,
    },
    TurnCompleted {
        turn_id: TurnId,
    },
}
