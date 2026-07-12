use crate::ids::BlockId;
use crate::layout::StableBox;
use serde::{Deserialize, Serialize};

use super::{InlineSpan, RichBlockKind, TablePayload, plain_text_from_spans};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockPayloadRecord {
    pub block_id: BlockId,
    pub content_version: u64,
    pub kind: RichBlockKind,
    pub payload: BlockPayload,
}

impl BlockPayloadRecord {
    pub fn rich_text(block_id: BlockId, kind: RichBlockKind, text: impl Into<String>) -> Self {
        Self {
            block_id,
            content_version: 1,
            kind,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan::plain(text)],
            },
        }
    }

    pub fn plain_text(&self) -> String {
        self.payload.plain_text()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockPayload {
    RichText {
        spans: Vec<InlineSpan>,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    Table(TablePayload),
    Image(ImagePayload),
    File(FilePayload),
    Whiteboard(WhiteboardPayload),
    Embed(EmbedPayload),
    Html {
        html: String,
        sanitized: bool,
    },
    Empty,
}

impl BlockPayload {
    pub fn plain_text(&self) -> String {
        match self {
            Self::RichText { spans } => plain_text_from_spans(spans),
            Self::Code { text, .. } => text.clone(),
            Self::Table(table) => table.plain_text(),
            Self::Image(image) => [image.alt.as_str(), image.caption.as_str()].join(" "),
            Self::File(file) => file.name.clone(),
            Self::Whiteboard(_) => "whiteboard".to_owned(),
            Self::Embed(embed) => [embed.title.as_str(), embed.url.as_str()].join(" "),
            Self::Html { html, .. } => html.clone(),
            Self::Empty => String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockPayloadView {
    Loaded(BlockPayloadRecord),
    Placeholder { estimated_height: f64 },
    Loading { stable_box: StableBox },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImagePayload {
    pub source: String,
    pub alt: String,
    pub caption: String,
    /// User-selected display width as a fraction of the editor image max width.
    /// 1000 means full image column width; None uses the Notion-like default.
    pub display_width_ratio_milli: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FilePayload {
    pub name: String,
    pub source: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WhiteboardPayload {
    pub scene_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EmbedPayload {
    pub url: String,
    pub title: String,
}
