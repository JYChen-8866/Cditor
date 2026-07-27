use super::read::{AgentBlockNode, MarkdownDialect};
use crate::policy::capability::AgentCapability;
use crate::{BlockId, TransactionId};
use serde::{Deserialize, Serialize};

pub type PreparedMutationId = uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentMutationIntent {
    ReplaceBlock {
        target: VersionedBlockRef,
        content: AgentContent,
    },
    InsertBlocks {
        anchor: VersionedInsertAnchor,
        content: AgentContent,
    },
    DeleteBlocks {
        targets: Vec<VersionedBlockRef>,
    },
    MoveBlocks {
        targets: Vec<VersionedBlockRef>,
        destination: VersionedInsertAnchor,
    },
    ReplaceSelection {
        target: VersionedTextRange,
        content: AgentContent,
    },
    SetBlockAttrs {
        target: VersionedBlockRef,
        patch_json: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum AgentContent {
    Markdown {
        source: String,
        dialect: MarkdownDialect,
    },
    Structured(AgentBlockNode),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VersionedBlockRef {
    pub block_id: BlockId,
    pub content_version: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertPosition {
    Before,
    After,
    FirstChild,
    LastChild,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VersionedInsertAnchor {
    pub reference_block_id: BlockId,
    pub position: InsertPosition,
    pub expected_structure_version: u64,
    pub expected_reference_content_version: Option<u64>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VersionedTextRange {
    pub block_id: BlockId,
    pub start_offset: u32,
    pub end_offset: u32,
    pub content_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedAgentMutation {
    pub id: PreparedMutationId,
    pub session_id: crate::AgentSessionId,
    pub tool_call_id: crate::ToolCallId,
    pub base_document_revision: u64,
    pub base_structure_version: u64,
    pub capability: AgentCapability,
    pub digest: [u8; 32],
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMutationPreview {
    pub summary: String,
    pub affected_blocks: Vec<BlockId>,
    pub inserted_blocks: Vec<BlockPreview>,
    pub deleted_blocks: Vec<BlockPreview>,
    pub textual_diffs: Vec<BlockTextDiff>,
    pub structural_moves: Vec<StructureMovePreview>,
    pub warnings: Vec<MutationWarning>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPreview {
    pub block_id: BlockId,
    pub kind: String,
    pub plain_text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTextDiff {
    pub block_id: BlockId,
    pub before: String,
    pub after: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureMovePreview {
    pub block_id: BlockId,
    pub from_parent: Option<BlockId>,
    pub to_position: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceDisposition {
    Queued,
    Saved,
    Flushed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMutationCommitResult {
    pub transaction_id: TransactionId,
    pub document_revision: u64,
    pub structure_version: u64,
    pub affected_blocks: Vec<BlockId>,
    pub created_block_ids: Vec<BlockId>,
    pub persistence: PersistenceDisposition,
    pub undo_available: bool,
}
#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn mutation_intent_serde() {
        let i = AgentMutationIntent::ReplaceBlock {
            target: VersionedBlockRef {
                block_id: uuid::Uuid::new_v4(),
                content_version: 1,
            },
            content: AgentContent::Markdown {
                source: "# x".into(),
                dialect: MarkdownDialect::CommonMark,
            },
        };
        let j = serde_json::to_string(&i).unwrap();
        assert!(j.contains("replace_block"));
    }
    #[test]
    fn commit_result_serde() {
        let r = AgentMutationCommitResult {
            transaction_id: uuid::Uuid::new_v4(),
            document_revision: 42,
            structure_version: 7,
            affected_blocks: vec![],
            created_block_ids: vec![],
            persistence: PersistenceDisposition::Queued,
            undo_available: true,
        };
        let j = serde_json::to_string(&r).unwrap();
        let b: AgentMutationCommitResult = serde_json::from_str(&j).unwrap();
        assert!(b.undo_available);
    }
}

impl AgentMutationIntent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ReplaceBlock { .. } => "replace_block",
            Self::InsertBlocks { .. } => "insert_blocks",
            Self::DeleteBlocks { .. } => "delete_blocks",
            Self::MoveBlocks { .. } => "move_blocks",
            Self::ReplaceSelection { .. } => "replace_selection",
            Self::SetBlockAttrs { .. } => "set_block_attrs",
        }
    }
}
