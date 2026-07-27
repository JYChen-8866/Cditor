pub mod policy;
pub mod protocol;
pub mod runtime;
pub mod tools;

// Re-export core type aliases from cditor_core so consumers stay consistent.
pub use cditor_core::ids::BlockId;
pub use cditor_core::ids::DocumentId;
pub use cditor_core::version::StructureVersion;

pub use policy::{
    capability::AgentCapability, confirmation::ConfirmationPolicy, redaction::RedactionPolicy,
};
pub use protocol::{
    checkpoint::AgentCheckpoint,
    context::{
        AgentContextSnapshot, AgentSelectionDescriptor, AgentSurface, AgentTextAnchor,
        AgentVisibleWindow,
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
        AgentContent, AgentMutationCommitResult, AgentMutationIntent, AgentMutationPreview,
        BlockPreview, BlockTextDiff, InsertPosition, MutationWarning, PreparedAgentMutation,
        PreparedMutationId, StructureMovePreview, VersionedBlockRef, VersionedInsertAnchor,
        VersionedTextRange,
    },
    read::{
        AgentBlockNode, AgentReadEnvelope, BlockKind, DocumentStat, MarkdownDialect,
        MarkdownLossiness, ReadCursor,
    },
    registry::{ToolHandler, ToolRegistry},
};

pub type AgentSessionId = u64;
pub type TurnId = u64;
pub type ToolCallId = u64;
pub type TransactionId = u64;
pub type AgentSnapshotId = u64;
pub type JsonValue = serde_json::Value;
pub type VersionVector = Vec<(BlockId, u64)>;
