use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::version::StructureVersion;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentSurface {
    Block(BlockId),
    TableCell {
        block_id: BlockId,
        row: usize,
        col: usize,
    },
    Document,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTextAnchor {
    pub block_id: BlockId,
    pub content_version: u64,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSelectionDescriptor {
    pub focused_block_id: Option<BlockId>,
    pub selected_block_ids: Vec<BlockId>,
    pub text_anchor: Option<AgentTextAnchor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentVisibleWindow {
    pub visible_block_ids: Vec<BlockId>,
    pub max_blocks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContextSnapshot {
    pub document_id: DocumentId,
    pub document_title: Option<String>,
    pub structure_version: StructureVersion,
    pub surface: AgentSurface,
    pub selection: AgentSelectionDescriptor,
    pub visible_window: AgentVisibleWindow,
}
