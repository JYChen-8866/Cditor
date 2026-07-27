pub mod protocol;
pub mod runtime;
pub mod tools;
pub mod policy;

pub use protocol::{
    checkpoint::AgentCheckpoint,
    context::{
        AgentContextSnapshot, AgentSelectionDescriptor, AgentSurface,
        AgentTextAnchor, AgentVisibleWindow,
    },
    error::{AgentErrorCode, AgentToolError},
    event::{AgentEvent, ToolProgress, ToolResultSummary},
};
pub use runtime::{
    budget::AgentBudget,
    compaction::Compactor,
    doom_loop::DoomLoopDetector,
    session::{AgentSession, AgentSessionConfig, AgentSessionState},
    turn::AgentTurn,
};
pub use tools::{
    effects::ToolEffects,
    mutation::{
        AgentContent, AgentMutationCommitResult, AgentMutationIntent,
        AgentMutationPreview, BlockPreview, BlockTextDiff,
        InsertPosition, MutationWarning, PreparedAgentMutation,
        PreparedMutationId, StructureMovePreview, VersionedBlockRef,
        VersionedInsertAnchor, VersionedTextRange,
    },
    read::{
        AgentBlockNode, AgentReadEnvelope, BlockKind, DocumentStat,
        MarkdownDialect, MarkdownLossiness, ReadCursor,
    },
    registry::{ToolHandler, ToolRegistry},
};
pub use policy::{
    capability::AgentCapability,
    confirmation::ConfirmationPolicy,
    redaction::RedactionPolicy,
};

pub type BlockId = uuid::Uuid;
pub type DocumentId = uuid::Uuid;
pub type AgentSessionId = uuid::Uuid;
pub type TurnId = uuid::Uuid;
pub type ToolCallId = uuid::Uuid;
pub type TransactionId = uuid::Uuid;
pub type AgentSnapshotId = uuid::Uuid;
pub type JsonValue = serde_json::Value;
pub type VersionVector = Vec<(BlockId, u64)>;
