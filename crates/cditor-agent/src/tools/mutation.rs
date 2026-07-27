use cditor_core::ids::BlockId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentContent {
    Markdown(String),
    Structured(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertPosition {
    Before(BlockId),
    After(BlockId),
    FirstChildOf(BlockId),
    LastChildOf(BlockId),
    AtIndex { parent_id: BlockId, index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedBlockRef {
    pub block_id: BlockId,
    pub content_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedInsertAnchor {
    pub position: InsertPosition,
    pub anchor_block_version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedTextRange {
    pub block_id: BlockId,
    pub content_version: u64,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockTextDiff {
    pub block_id: BlockId,
    pub removed_text: String,
    pub inserted_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockPreview {
    pub block_id: Option<BlockId>,
    pub indented: bool,
    pub kind_hint: Option<String>,
    pub summary: String,
    pub markdown_preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureMovePreview {
    pub moved_block_ids: Vec<BlockId>,
    pub source_parent_id: BlockId,
    pub target_position: InsertPosition,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationWarning {
    pub code: String,
    pub message: String,
    pub block_id: Option<BlockId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMutationIntent {
    pub content: AgentContent,
    pub target: InsertPosition,
    pub replace_block_id: Option<BlockId>,
    pub text_range: Option<VersionedTextRange>,
    pub precondition_block_ids: Vec<VersionedBlockRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PreparedMutationId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedAgentMutation {
    pub id: PreparedMutationId,
    pub intent: AgentMutationIntent,
    pub block_previews: Vec<BlockPreview>,
    pub move_previews: Vec<StructureMovePreview>,
    pub warnings: Vec<MutationWarning>,
    pub content_digest: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl PreparedAgentMutation {
    pub fn compute_digest(intent: &AgentMutationIntent) -> String {
        let mut hasher = Sha256::new();
        let json = serde_json::to_string(intent).unwrap_or_default();
        hasher.update(json.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMutationPreview {
    pub prepared: PreparedAgentMutation,
    pub affected_block_ids: Vec<BlockId>,
    pub will_create_blocks: usize,
    pub will_modify_blocks: usize,
    pub will_delete_blocks: usize,
    pub structure_version_after: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMutationCommitResult {
    pub committed: bool,
    pub transaction_id: u64,
    pub created_block_ids: Vec<BlockId>,
    pub modified_block_ids: Vec<BlockId>,
    pub deleted_block_ids: Vec<BlockId>,
    pub structure_version_after: u64,
}
