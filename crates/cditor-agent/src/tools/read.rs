use cditor_core::ids::BlockId;
use cditor_core::version::StructureVersion;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownDialect {
    Standard,
    Kramdown,
    SiYuan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownLossiness {
    pub lost_inline_styles: usize,
    pub lost_embeds: usize,
    pub lost_custom_blocks: usize,
    pub lost_tables: usize,
    pub notes: Vec<String>,
}

impl MarkdownLossiness {
    pub fn is_empty(&self) -> bool {
        self.lost_inline_styles == 0
            && self.lost_embeds == 0
            && self.lost_custom_blocks == 0
            && self.lost_tables == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    Paragraph,
    Heading { level: u8 },
    Quote,
    BulletedList,
    NumberedList,
    Todo { checked: bool },
    Toggle,
    Callout { icon: Option<String> },
    Code { language: Option<String> },
    Table,
    TableRow,
    TableCell,
    Divider,
    Image,
    Video,
    Audio,
    File,
    Bookmark,
    Embed,
    Collection,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBlockNode {
    pub id: BlockId,
    pub kind: BlockKind,
    pub content_version: u64,
    pub markdown: Option<String>,
    pub summary: Option<String>,
    pub children: Vec<AgentBlockNode>,
    pub has_more_children: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentStat {
    pub total_blocks: usize,
    pub max_depth: u16,
    pub heading_count: usize,
    pub table_count: usize,
    pub media_count: usize,
    pub total_text_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadCursor {
    pub after_block_id: Option<BlockId>,
    pub remaining: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentReadEnvelope<T: Clone + std::fmt::Debug + PartialEq + Eq> {
    pub result: T,
    pub cursor: Option<ReadCursor>,
    pub lossiness: Option<MarkdownLossiness>,
    pub structure_version: StructureVersion,
}
