use crate::{AgentSnapshotId, BlockId, DocumentId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSurface {
    Title,
    Body,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTextAnchor {
    pub block_id: BlockId,
    pub surface: AgentSurface,
    pub utf16_offset: u32,
    pub content_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentSelectionDescriptor {
    None,
    Caret {
        anchor: AgentTextAnchor,
    },
    Range {
        start: AgentTextAnchor,
        end: AgentTextAnchor,
    },
    BlockSelection {
        block_ids: Vec<BlockId>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentVisibleWindow {
    pub first_block_id: Option<BlockId>,
    pub last_block_id: Option<BlockId>,
    pub visible_block_ids: Vec<BlockId>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContextSnapshot {
    pub snapshot_id: AgentSnapshotId,
    pub document_id: DocumentId,
    pub document_revision: u64,
    pub structure_version: u64,
    pub title: Option<String>,
    pub focused: Option<AgentTextAnchor>,
    pub selection: AgentSelectionDescriptor,
    pub selected_block_ids: Vec<BlockId>,
    pub visible_window: AgentVisibleWindow,
    pub captured_at_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_snap() -> AgentContextSnapshot {
        AgentContextSnapshot {
            snapshot_id: uuid::Uuid::new_v4(),
            document_id: uuid::Uuid::new_v4(),
            document_revision: 1,
            structure_version: 3,
            title: Some("Doc".into()),
            focused: None,
            selection: AgentSelectionDescriptor::None,
            selected_block_ids: vec![],
            visible_window: AgentVisibleWindow {
                first_block_id: None,
                last_block_id: None,
                visible_block_ids: vec![uuid::Uuid::new_v4()],
                truncated: false,
            },
            captured_at_ms: 1000,
        }
    }
    #[test]
    fn snapshot_roundtrip_json() {
        let s = make_snap();
        let j = serde_json::to_string(&s).unwrap();
        let b: AgentContextSnapshot = serde_json::from_str(&j).unwrap();
        assert_eq!(b.document_revision, 1);
    }
    #[test]
    fn selection_none_serde() {
        let d = AgentSelectionDescriptor::None;
        let j = serde_json::to_string(&d).unwrap();
        assert!(j.contains("none"));
    }
}
