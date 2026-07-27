use crate::{BlockId, DocumentId, ToolCallId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadCursor {
    pub document_id: DocumentId,
    pub structure_version: u64,
    pub next_offset: usize,
    pub token: Option<[u8; 16]>,
}
impl ReadCursor {
    pub fn is_stale(&self, current: u64) -> bool {
        self.structure_version != current
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReadEnvelope<T> {
    pub request_id: ToolCallId,
    pub document_id: DocumentId,
    pub observed_revision: u64,
    pub observed_structure_version: u64,
    pub data: T,
    pub truncated: bool,
    pub continuation: Option<ReadCursor>,
    pub estimated_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Document,
    Heading,
    Paragraph,
    List,
    ListItem,
    CodeBlock,
    Table,
    TableRow,
    TableCell,
    Image,
    Callout,
    Blockquote,
    Divider,
    Bookmark,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkdownDialect {
    CommonMark,
    Cditor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownLossiness {
    pub warnings: Vec<String>,
    pub lossy_blocks: Vec<BlockId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentStat {
    pub document_id: DocumentId,
    pub title: Option<String>,
    pub total_blocks: usize,
    pub top_level_count: usize,
    pub kind_counts: Vec<(BlockKind, usize)>,
    pub structure_version: u64,
    pub fully_indexed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBlockSummary {
    pub block_id: BlockId,
    pub kind: BlockKind,
    pub plain_text: String,
    pub attrs_json: Option<String>,
    pub content_version: u64,
    pub has_children: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildListEntry {
    pub block_id: BlockId,
    pub kind: BlockKind,
    pub summary: String,
    pub has_children: bool,
    pub content_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBlockNode {
    pub block_id: BlockId,
    pub kind: BlockKind,
    pub inline_marks: Vec<String>,
    pub attrs_json: Option<String>,
    pub children: Vec<AgentBlockNode>,
}
#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn read_cursor_stale() {
        let c = ReadCursor {
            document_id: uuid::Uuid::new_v4(),
            structure_version: 5,
            next_offset: 0,
            token: None,
        };
        assert!(c.is_stale(6));
        assert!(!c.is_stale(5));
    }
    #[test]
    fn envelope_serde() {
        let e: AgentReadEnvelope<DocumentStat> = AgentReadEnvelope {
            request_id: uuid::Uuid::new_v4(),
            document_id: uuid::Uuid::new_v4(),
            observed_revision: 7,
            observed_structure_version: 3,
            data: DocumentStat {
                document_id: uuid::Uuid::new_v4(),
                title: None,
                total_blocks: 42,
                top_level_count: 5,
                kind_counts: vec![],
                structure_version: 3,
                fully_indexed: true,
            },
            truncated: false,
            continuation: None,
            estimated_tokens: 200,
        };
        let j = serde_json::to_string(&e).unwrap();
        let b: AgentReadEnvelope<DocumentStat> = serde_json::from_str(&j).unwrap();
        assert!(!b.truncated);
    }
}
