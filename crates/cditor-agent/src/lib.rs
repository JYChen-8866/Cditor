#[cfg(test)]
mod tests;
pub mod model;
pub mod policy;
pub mod protocol;
pub mod runtime;
pub mod tools;

pub type BlockId = uuid::Uuid;
pub type DocumentId = uuid::Uuid;
pub type AgentSessionId = uuid::Uuid;
pub type TurnId = uuid::Uuid;
pub type ToolCallId = uuid::Uuid;
pub type TransactionId = uuid::Uuid;
pub type AgentSnapshotId = uuid::Uuid;
pub type JsonValue = serde_json::Value;
pub type VersionVector = Vec<(BlockId, u64)>;

pub use model::{ModelInfo, ModelProvider, StreamEvent, tool_parser};
pub use policy::{
    capability::{AgentCapability, AgentCapabilityPolicy},
    confirmation::ConfirmationPolicy,
    redaction::RedactionPolicy,
};
pub use protocol::{
    checkpoint::AgentCheckpoint,
    context::{
        AgentContextSnapshot, AgentSelectionDescriptor, AgentSurface, AgentTextAnchor,
        AgentVisibleWindow,
    },
    error::{AgentErrorCode, AgentToolError, BudgetDimension},
    event::{AgentEvent, QuestionOption, ToolProgress, ToolResultSummary},
    messages::{
        AgentEditorContext, AgentMessage, AgentReference, AgentToolCallEntry, SessionEntry,
        SessionEntryStep, agent_messages_to_entries, entries_to_agent_messages,
    },
};
pub use runtime::{
    budget::AgentBudget,
    compaction::{CompactionResult, Compactor},
    doom_loop::{DoomLoopDetector, DoomLoopStatus},
    engine::{
        AgentChannels, AgentConfirmResult, AgentFrontendResult, AgentQuestionAnswer,
        AgentRuntimeTurn, AgentService, AggregatedToolCall, ChatMessage, ToolCallMsg,
    },
    r#loop::AgentRuntime,
    msg_builder::{build_system_prompt, build_user_message_content},
    session::{AgentSession, AgentSessionConfig, AgentSessionState},
    turn::AgentTurn,
};
pub use tools::{
    concrete::blocks::register_native_tools,
    effects::ToolEffects,
    mutation::{
        AgentContent, AgentMutationCommitResult, AgentMutationIntent, AgentMutationPreview,
        BlockPreview, BlockTextDiff, InsertPosition, MutationWarning, PersistenceDisposition,
        PreparedAgentMutation, PreparedMutationId, StructureMovePreview, VersionedBlockRef,
        VersionedInsertAnchor, VersionedTextRange,
    },
    read::{
        AgentBlockNode, AgentBlockSummary, AgentReadEnvelope, BlockKind, ChildListEntry,
        DocumentStat, MarkdownDialect, MarkdownLossiness, ReadCursor,
    },
    registry::{ToolHandler, ToolRegistry},
};
pub use runtime::confirm::{
    ConfirmAnswer, ConfirmRequest, ConfirmSession, QuestionAnswer, QuestionRequest, await_confirm,
};
pub use runtime::persistence::{FsPersistenceStore, PersistenceError, PersistenceStore, save_runtime_atomic};
pub use runtime::adapter::{AgentReadPort, AgentWritePort, AgentTransactionChannel, build_context_snapshot};
pub use runtime::api::{ChatRequest, ChatResponse, ConfirmRequest as ApiConfirmRequest, SseEvent, SseOption, route_chat};
