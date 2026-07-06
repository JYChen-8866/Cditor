use std::collections::BTreeMap;
use std::ops::Range;

use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use crate::core::document::BlockIndexRecord;
use crate::core::edit::{
    DocumentSelection, EditOperation, EditTransaction, EditTransactionKind, TextAffinity,
    TextPosition,
};
use crate::core::ids::{BlockId, DocumentId};
use crate::core::rich_text::{
    BlockAttrs, BlockPayload, CalloutVariant, EmbedPayload, FilePayload, ImagePayload, InlineMark,
    InlineSpan, RichBlockKind, TableCellPayload, TablePayload, TableRowPayload, TextAlign,
    WhiteboardPayload,
};
use crate::editor::scroll::virtual_scroll::ScrollAnchor;

pub type PgDocumentId = Uuid;
pub type PgBlockId = Uuid;

const DOCUMENT_ID_NAMESPACE: u128 = 0x1000_0000_0000_0000_0000_0000_0000_0000;
const BLOCK_ID_NAMESPACE: u128 = 0x2000_0000_0000_0000_0000_0000_0000_0000;
const LOW_64_BITS: u128 = u64::MAX as u128;
const NAMESPACE_MASK: u128 = !LOW_64_BITS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRow {
    pub id: PgDocumentId,
    pub workspace_id: Uuid,
    pub title: String,
    pub structure_version: i64,
    pub content_version: i64,
    pub layout_version: i64,
    pub schema_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRow {
    pub id: PgBlockId,
    pub document_id: PgDocumentId,
    pub parent_id: Option<PgBlockId>,
    pub prev_id: Option<PgBlockId>,
    pub next_id: Option<PgBlockId>,
    pub sort_key: String,
    pub depth: i32,
    pub kind: String,
    pub flags: i32,
    pub content_version: i64,
    pub structure_version: i64,
    pub attrs_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPayloadRow {
    pub block_id: PgBlockId,
    pub document_id: PgDocumentId,
    pub payload_format: String,
    pub payload_json: Option<serde_json::Value>,
    pub plain_text: String,
    pub content_hash: Option<String>,
    pub content_version: i64,
    pub byte_len: i64,
    pub inline_run_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbBlockAttrs {
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub text_align: DbTextAlign,
    pub indent: u16,
    pub folded: bool,
    pub locked: bool,
    pub custom: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbTextAlign {
    Start,
    Center,
    End,
}

impl From<&BlockAttrs> for DbBlockAttrs {
    fn from(attrs: &BlockAttrs) -> Self {
        Self {
            color: attrs.color.clone(),
            background_color: attrs.background_color.clone(),
            text_align: DbTextAlign::from(attrs.text_align),
            indent: attrs.indent,
            folded: attrs.folded,
            locked: attrs.locked,
            custom: attrs.custom.clone(),
        }
    }
}

impl From<DbBlockAttrs> for BlockAttrs {
    fn from(attrs: DbBlockAttrs) -> Self {
        Self {
            color: attrs.color,
            background_color: attrs.background_color,
            text_align: TextAlign::from(attrs.text_align),
            indent: attrs.indent,
            folded: attrs.folded,
            locked: attrs.locked,
            custom: attrs.custom,
        }
    }
}

impl From<TextAlign> for DbTextAlign {
    fn from(align: TextAlign) -> Self {
        match align {
            TextAlign::Start => Self::Start,
            TextAlign::Center => Self::Center,
            TextAlign::End => Self::End,
        }
    }
}

impl From<DbTextAlign> for TextAlign {
    fn from(align: DbTextAlign) -> Self {
        match align {
            DbTextAlign::Start => Self::Start,
            DbTextAlign::Center => Self::Center,
            DbTextAlign::End => Self::End,
        }
    }
}

pub fn encode_block_attrs(attrs: &BlockAttrs) -> serde_json::Result<serde_json::Value> {
    serde_json::to_value(DbBlockAttrs::from(attrs))
}

pub fn decode_block_attrs(value: serde_json::Value) -> serde_json::Result<BlockAttrs> {
    serde_json::from_value::<DbBlockAttrs>(value).map(BlockAttrs::from)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DbBlockPayload {
    RichText {
        spans: Vec<DbInlineSpan>,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    Table {
        rows: Vec<DbTableRow>,
        header_rows: usize,
        header_cols: usize,
    },
    Image {
        source: String,
        alt: String,
        caption: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_width_ratio_milli: Option<u16>,
    },
    File {
        name: String,
        source: String,
        size_bytes: Option<u64>,
    },
    Whiteboard {
        scene_json: String,
    },
    Embed {
        url: String,
        title: String,
    },
    Html {
        html: String,
        sanitized: bool,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbInlineSpan {
    pub text: String,
    pub marks: Vec<DbInlineMark>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DbInlineMark {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Link { href: String },
    Color(String),
    Background(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTableRow {
    pub cells: Vec<DbTableCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTableCell {
    pub spans: Vec<DbInlineSpan>,
}

impl From<&BlockPayload> for DbBlockPayload {
    fn from(payload: &BlockPayload) -> Self {
        match payload {
            BlockPayload::RichText { spans } => Self::RichText {
                spans: spans.iter().map(DbInlineSpan::from).collect(),
            },
            BlockPayload::Code { language, text } => Self::Code {
                language: language.clone(),
                text: text.clone(),
            },
            BlockPayload::Table(table) => Self::Table {
                rows: table.rows.iter().map(DbTableRow::from).collect(),
                header_rows: table.header_rows,
                header_cols: table.header_cols,
            },
            BlockPayload::Image(image) => Self::Image {
                source: image.source.clone(),
                alt: image.alt.clone(),
                caption: image.caption.clone(),
                display_width_ratio_milli: image.display_width_ratio_milli,
            },
            BlockPayload::File(file) => Self::File {
                name: file.name.clone(),
                source: file.source.clone(),
                size_bytes: file.size_bytes,
            },
            BlockPayload::Whiteboard(whiteboard) => Self::Whiteboard {
                scene_json: whiteboard.scene_json.clone(),
            },
            BlockPayload::Embed(embed) => Self::Embed {
                url: embed.url.clone(),
                title: embed.title.clone(),
            },
            BlockPayload::Html { html, sanitized } => Self::Html {
                html: html.clone(),
                sanitized: *sanitized,
            },
            BlockPayload::Empty => Self::Empty,
        }
    }
}

impl From<DbBlockPayload> for BlockPayload {
    fn from(payload: DbBlockPayload) -> Self {
        match payload {
            DbBlockPayload::RichText { spans } => Self::RichText {
                spans: spans.into_iter().map(InlineSpan::from).collect(),
            },
            DbBlockPayload::Code { language, text } => Self::Code { language, text },
            DbBlockPayload::Table {
                rows,
                header_rows,
                header_cols,
            } => Self::Table(TablePayload {
                rows: rows.into_iter().map(TableRowPayload::from).collect(),
                header_rows,
                header_cols,
            }),
            DbBlockPayload::Image {
                source,
                alt,
                caption,
                display_width_ratio_milli,
            } => Self::Image(ImagePayload {
                source,
                alt,
                caption,
                display_width_ratio_milli,
            }),
            DbBlockPayload::File {
                name,
                source,
                size_bytes,
            } => Self::File(FilePayload {
                name,
                source,
                size_bytes,
            }),
            DbBlockPayload::Whiteboard { scene_json } => {
                Self::Whiteboard(WhiteboardPayload { scene_json })
            }
            DbBlockPayload::Embed { url, title } => Self::Embed(EmbedPayload { url, title }),
            DbBlockPayload::Html { html, sanitized } => Self::Html { html, sanitized },
            DbBlockPayload::Empty => Self::Empty,
        }
    }
}

impl From<&InlineSpan> for DbInlineSpan {
    fn from(span: &InlineSpan) -> Self {
        Self {
            text: span.text.clone(),
            marks: span.marks.iter().map(DbInlineMark::from).collect(),
        }
    }
}

impl From<DbInlineSpan> for InlineSpan {
    fn from(span: DbInlineSpan) -> Self {
        Self {
            text: span.text,
            marks: span.marks.into_iter().map(InlineMark::from).collect(),
        }
    }
}

impl From<&InlineMark> for DbInlineMark {
    fn from(mark: &InlineMark) -> Self {
        match mark {
            InlineMark::Bold => Self::Bold,
            InlineMark::Italic => Self::Italic,
            InlineMark::Underline => Self::Underline,
            InlineMark::Strike => Self::Strike,
            InlineMark::Code => Self::Code,
            InlineMark::Link { href } => Self::Link { href: href.clone() },
            InlineMark::Color(color) => Self::Color(color.clone()),
            InlineMark::Background(color) => Self::Background(color.clone()),
        }
    }
}

impl From<DbInlineMark> for InlineMark {
    fn from(mark: DbInlineMark) -> Self {
        match mark {
            DbInlineMark::Bold => Self::Bold,
            DbInlineMark::Italic => Self::Italic,
            DbInlineMark::Underline => Self::Underline,
            DbInlineMark::Strike => Self::Strike,
            DbInlineMark::Code => Self::Code,
            DbInlineMark::Link { href } => Self::Link { href },
            DbInlineMark::Color(color) => Self::Color(color),
            DbInlineMark::Background(color) => Self::Background(color),
        }
    }
}

impl From<&TableRowPayload> for DbTableRow {
    fn from(row: &TableRowPayload) -> Self {
        Self {
            cells: row.cells.iter().map(DbTableCell::from).collect(),
        }
    }
}

impl From<DbTableRow> for TableRowPayload {
    fn from(row: DbTableRow) -> Self {
        Self {
            cells: row.cells.into_iter().map(TableCellPayload::from).collect(),
        }
    }
}

impl From<&TableCellPayload> for DbTableCell {
    fn from(cell: &TableCellPayload) -> Self {
        Self {
            spans: cell.spans.iter().map(DbInlineSpan::from).collect(),
        }
    }
}

impl From<DbTableCell> for TableCellPayload {
    fn from(cell: DbTableCell) -> Self {
        Self {
            spans: cell.spans.into_iter().map(InlineSpan::from).collect(),
        }
    }
}

pub fn encode_block_payload(payload: &BlockPayload) -> serde_json::Result<serde_json::Value> {
    serde_json::to_value(DbBlockPayload::from(payload))
}

pub fn decode_block_payload(value: serde_json::Value) -> serde_json::Result<BlockPayload> {
    serde_json::from_value::<DbBlockPayload>(value).map(BlockPayload::from)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbEditTransaction {
    pub id: u64,
    pub kind: DbEditTransactionKind,
    pub timestamp: u64,
    pub ops: Vec<DbEditOperation>,
    pub inverse_ops: Vec<DbEditOperation>,
    pub affected_blocks: Vec<BlockId>,
    pub before_selection: Option<DbDocumentSelection>,
    pub after_selection: Option<DbDocumentSelection>,
    pub before_anchor: Option<DbScrollAnchor>,
    pub after_anchor: Option<DbScrollAnchor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbEditTransactionKind {
    Typing,
    CompositionCommit,
    Paste,
    DragDrop,
    Format,
    ExplicitCommand,
    BlockStructureChange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DbEditOperation {
    InsertText {
        block_id: BlockId,
        offset: usize,
        text: String,
    },
    DeleteText {
        block_id: BlockId,
        start: usize,
        end: usize,
    },
    SplitBlock {
        block_id: BlockId,
        offset: usize,
        new_block_id: BlockId,
    },
    MergeBlocks {
        previous: BlockId,
        current: BlockId,
    },
    InsertBlock {
        index: usize,
        block: DbBlockIndexRecord,
    },
    DeleteBlock {
        block_id: BlockId,
    },
    MoveBlock {
        block_id: BlockId,
        target_index: usize,
    },
    MoveBlockToParent {
        block_id: BlockId,
        parent_id: Option<BlockId>,
        sibling_index: usize,
    },
    SetBlockKind {
        block_id: BlockId,
        kind: u16,
    },
    InsertBlocks {
        index: usize,
        blocks: Vec<DbBlockIndexRecord>,
    },
    DeleteBlockRange {
        start: usize,
        end: usize,
    },
    MoveBlockRange {
        start: usize,
        end: usize,
        target_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DbBlockIndexRecord {
    pub id: BlockId,
    pub parent_id: Option<BlockId>,
    pub depth: u16,
    pub kind_tag: u16,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbDocumentSelection {
    pub anchor: DbTextPosition,
    pub focus: DbTextPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTextPosition {
    pub block_id: BlockId,
    pub offset: usize,
    pub affinity: DbTextAffinity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbTextAffinity {
    Upstream,
    Downstream,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DbScrollAnchor {
    pub block_id: BlockId,
    pub offset_in_block: f64,
    pub viewport_y: f64,
}

impl From<&EditTransaction> for DbEditTransaction {
    fn from(tx: &EditTransaction) -> Self {
        Self {
            id: tx.id,
            kind: DbEditTransactionKind::from(tx.kind),
            timestamp: tx.timestamp,
            ops: tx.ops.iter().map(DbEditOperation::from).collect(),
            inverse_ops: tx.inverse_ops.iter().map(DbEditOperation::from).collect(),
            affected_blocks: tx.affected_blocks.clone(),
            before_selection: tx.before_selection.map(DbDocumentSelection::from),
            after_selection: tx.after_selection.map(DbDocumentSelection::from),
            before_anchor: tx.before_anchor.map(DbScrollAnchor::from),
            after_anchor: tx.after_anchor.map(DbScrollAnchor::from),
        }
    }
}

impl From<EditTransactionKind> for DbEditTransactionKind {
    fn from(kind: EditTransactionKind) -> Self {
        match kind {
            EditTransactionKind::Typing => Self::Typing,
            EditTransactionKind::CompositionCommit => Self::CompositionCommit,
            EditTransactionKind::Paste => Self::Paste,
            EditTransactionKind::DragDrop => Self::DragDrop,
            EditTransactionKind::Format => Self::Format,
            EditTransactionKind::ExplicitCommand => Self::ExplicitCommand,
            EditTransactionKind::BlockStructureChange => Self::BlockStructureChange,
        }
    }
}

impl From<&EditOperation> for DbEditOperation {
    fn from(op: &EditOperation) -> Self {
        match op {
            EditOperation::InsertText {
                block_id,
                offset,
                text,
            } => Self::InsertText {
                block_id: *block_id,
                offset: *offset,
                text: text.clone(),
            },
            EditOperation::DeleteText { block_id, range } => Self::DeleteText {
                block_id: *block_id,
                start: range.start,
                end: range.end,
            },
            EditOperation::SplitBlock {
                block_id,
                offset,
                new_block_id,
            } => Self::SplitBlock {
                block_id: *block_id,
                offset: *offset,
                new_block_id: *new_block_id,
            },
            EditOperation::MergeBlocks { previous, current } => Self::MergeBlocks {
                previous: *previous,
                current: *current,
            },
            EditOperation::InsertBlock { index, block } => Self::InsertBlock {
                index: *index,
                block: DbBlockIndexRecord::from(*block),
            },
            EditOperation::DeleteBlock { block_id } => Self::DeleteBlock {
                block_id: *block_id,
            },
            EditOperation::MoveBlock {
                block_id,
                target_index,
            } => Self::MoveBlock {
                block_id: *block_id,
                target_index: *target_index,
            },
            EditOperation::MoveBlockToParent {
                block_id,
                parent_id,
                sibling_index,
            } => Self::MoveBlockToParent {
                block_id: *block_id,
                parent_id: *parent_id,
                sibling_index: *sibling_index,
            },
            EditOperation::SetBlockKind { block_id, kind } => Self::SetBlockKind {
                block_id: *block_id,
                kind: *kind,
            },
            EditOperation::InsertBlocks { index, blocks } => Self::InsertBlocks {
                index: *index,
                blocks: blocks
                    .iter()
                    .copied()
                    .map(DbBlockIndexRecord::from)
                    .collect(),
            },
            EditOperation::DeleteBlockRange { range } => Self::DeleteBlockRange {
                start: range.start,
                end: range.end,
            },
            EditOperation::MoveBlockRange {
                range,
                target_index,
            } => Self::MoveBlockRange {
                start: range.start,
                end: range.end,
                target_index: *target_index,
            },
        }
    }
}

impl From<BlockIndexRecord> for DbBlockIndexRecord {
    fn from(record: BlockIndexRecord) -> Self {
        Self {
            id: record.id,
            parent_id: record.parent_id,
            depth: record.depth,
            kind_tag: record.kind_tag,
            flags: record.flags,
        }
    }
}

impl From<DocumentSelection> for DbDocumentSelection {
    fn from(selection: DocumentSelection) -> Self {
        Self {
            anchor: DbTextPosition::from(selection.anchor),
            focus: DbTextPosition::from(selection.focus),
        }
    }
}

impl From<TextPosition> for DbTextPosition {
    fn from(position: TextPosition) -> Self {
        Self {
            block_id: position.block_id,
            offset: position.offset,
            affinity: DbTextAffinity::from(position.affinity),
        }
    }
}

impl From<TextAffinity> for DbTextAffinity {
    fn from(affinity: TextAffinity) -> Self {
        match affinity {
            TextAffinity::Upstream => Self::Upstream,
            TextAffinity::Downstream => Self::Downstream,
        }
    }
}

impl From<ScrollAnchor> for DbScrollAnchor {
    fn from(anchor: ScrollAnchor) -> Self {
        Self {
            block_id: anchor.block_id,
            offset_in_block: anchor.offset_in_block,
            viewport_y: anchor.viewport_y,
        }
    }
}

pub fn encode_edit_transaction(tx: &EditTransaction) -> serde_json::Result<serde_json::Value> {
    serde_json::to_value(DbEditTransaction::from(tx))
}

pub fn decode_edit_transaction(value: serde_json::Value) -> serde_json::Result<EditTransaction> {
    serde_json::from_value::<DbEditTransaction>(value).map(EditTransaction::from)
}

impl From<DbEditTransaction> for EditTransaction {
    fn from(tx: DbEditTransaction) -> Self {
        Self {
            id: tx.id,
            ops: tx.ops.into_iter().map(EditOperation::from).collect(),
            inverse_ops: tx
                .inverse_ops
                .into_iter()
                .map(EditOperation::from)
                .collect(),
            affected_blocks: tx.affected_blocks,
            before_selection: tx.before_selection.map(DocumentSelection::from),
            after_selection: tx.after_selection.map(DocumentSelection::from),
            before_anchor: tx.before_anchor.map(ScrollAnchor::from),
            after_anchor: tx.after_anchor.map(ScrollAnchor::from),
            timestamp: tx.timestamp,
            kind: EditTransactionKind::from(tx.kind),
        }
    }
}

impl From<DbEditTransactionKind> for EditTransactionKind {
    fn from(kind: DbEditTransactionKind) -> Self {
        match kind {
            DbEditTransactionKind::Typing => Self::Typing,
            DbEditTransactionKind::CompositionCommit => Self::CompositionCommit,
            DbEditTransactionKind::Paste => Self::Paste,
            DbEditTransactionKind::DragDrop => Self::DragDrop,
            DbEditTransactionKind::Format => Self::Format,
            DbEditTransactionKind::ExplicitCommand => Self::ExplicitCommand,
            DbEditTransactionKind::BlockStructureChange => Self::BlockStructureChange,
        }
    }
}

impl From<DbEditOperation> for EditOperation {
    fn from(op: DbEditOperation) -> Self {
        match op {
            DbEditOperation::InsertText {
                block_id,
                offset,
                text,
            } => Self::InsertText {
                block_id,
                offset,
                text,
            },
            DbEditOperation::DeleteText {
                block_id,
                start,
                end,
            } => Self::DeleteText {
                block_id,
                range: Range { start, end },
            },
            DbEditOperation::SplitBlock {
                block_id,
                offset,
                new_block_id,
            } => Self::SplitBlock {
                block_id,
                offset,
                new_block_id,
            },
            DbEditOperation::MergeBlocks { previous, current } => {
                Self::MergeBlocks { previous, current }
            }
            DbEditOperation::InsertBlock { index, block } => Self::InsertBlock {
                index,
                block: BlockIndexRecord::from(block),
            },
            DbEditOperation::DeleteBlock { block_id } => Self::DeleteBlock { block_id },
            DbEditOperation::MoveBlock {
                block_id,
                target_index,
            } => Self::MoveBlock {
                block_id,
                target_index,
            },
            DbEditOperation::MoveBlockToParent {
                block_id,
                parent_id,
                sibling_index,
            } => Self::MoveBlockToParent {
                block_id,
                parent_id,
                sibling_index,
            },
            DbEditOperation::SetBlockKind { block_id, kind } => {
                Self::SetBlockKind { block_id, kind }
            }
            DbEditOperation::InsertBlocks { index, blocks } => Self::InsertBlocks {
                index,
                blocks: blocks.into_iter().map(BlockIndexRecord::from).collect(),
            },
            DbEditOperation::DeleteBlockRange { start, end } => Self::DeleteBlockRange {
                range: Range { start, end },
            },
            DbEditOperation::MoveBlockRange {
                start,
                end,
                target_index,
            } => Self::MoveBlockRange {
                range: Range { start, end },
                target_index,
            },
        }
    }
}

impl From<DbBlockIndexRecord> for BlockIndexRecord {
    fn from(record: DbBlockIndexRecord) -> Self {
        BlockIndexRecord::new(
            record.id,
            record.parent_id,
            record.depth,
            record.kind_tag,
            record.flags,
        )
    }
}

impl From<DbDocumentSelection> for DocumentSelection {
    fn from(selection: DbDocumentSelection) -> Self {
        Self {
            anchor: TextPosition::from(selection.anchor),
            focus: TextPosition::from(selection.focus),
        }
    }
}

impl From<DbTextPosition> for TextPosition {
    fn from(position: DbTextPosition) -> Self {
        Self {
            block_id: position.block_id,
            offset: position.offset,
            affinity: TextAffinity::from(position.affinity),
        }
    }
}

impl From<DbTextAffinity> for TextAffinity {
    fn from(affinity: DbTextAffinity) -> Self {
        match affinity {
            DbTextAffinity::Upstream => Self::Upstream,
            DbTextAffinity::Downstream => Self::Downstream,
        }
    }
}

impl From<DbScrollAnchor> for ScrollAnchor {
    fn from(anchor: DbScrollAnchor) -> Self {
        Self {
            block_id: anchor.block_id,
            offset_in_block: anchor.offset_in_block,
            viewport_y: anchor.viewport_y,
        }
    }
}

pub fn pg_document_id_from_runtime(id: DocumentId) -> PgDocumentId {
    Uuid::from_u128(DOCUMENT_ID_NAMESPACE | id as u128)
}

pub fn pg_block_id_from_runtime(id: BlockId) -> PgBlockId {
    Uuid::from_u128(BLOCK_ID_NAMESPACE | id as u128)
}

pub fn runtime_document_id_from_pg(id: PgDocumentId) -> Option<DocumentId> {
    let raw = id.as_u128();
    ((raw & NAMESPACE_MASK) == DOCUMENT_ID_NAMESPACE).then_some((raw & LOW_64_BITS) as u64)
}

pub fn runtime_block_id_from_pg(id: PgBlockId) -> Option<BlockId> {
    let raw = id.as_u128();
    ((raw & NAMESPACE_MASK) == BLOCK_ID_NAMESPACE).then_some((raw & LOW_64_BITS) as u64)
}

pub fn rich_block_kind_to_db(kind: &RichBlockKind) -> String {
    match kind {
        RichBlockKind::Paragraph => "paragraph".to_owned(),
        RichBlockKind::Heading { level } => format!("heading:{level}"),
        RichBlockKind::Quote => "quote".to_owned(),
        RichBlockKind::Callout { variant } => {
            format!("callout:{}", callout_variant_to_db(*variant))
        }
        RichBlockKind::Todo { checked } => format!("todo:{checked}"),
        RichBlockKind::BulletedList => "bulleted_list".to_owned(),
        RichBlockKind::NumberedList => "numbered_list".to_owned(),
        RichBlockKind::Toggle => "toggle".to_owned(),
        RichBlockKind::Code { language } => match language {
            Some(language) => format!("code:{language}"),
            None => "code".to_owned(),
        },
        RichBlockKind::Math => "math".to_owned(),
        RichBlockKind::Mermaid => "mermaid".to_owned(),
        RichBlockKind::Html => "html".to_owned(),
        RichBlockKind::Table => "table".to_owned(),
        RichBlockKind::Image => "image".to_owned(),
        RichBlockKind::File => "file".to_owned(),
        RichBlockKind::Attachment => "attachment".to_owned(),
        RichBlockKind::Whiteboard => "whiteboard".to_owned(),
        RichBlockKind::MindMap => "mind_map".to_owned(),
        RichBlockKind::Embed => "embed".to_owned(),
        RichBlockKind::Divider => "divider".to_owned(),
        RichBlockKind::Separator => "separator".to_owned(),
        RichBlockKind::FootnoteDefinition => "footnote_definition".to_owned(),
        RichBlockKind::Comment => "comment".to_owned(),
        RichBlockKind::RawMarkdown => "raw_markdown".to_owned(),
        RichBlockKind::Database => "database".to_owned(),
        RichBlockKind::Custom(name) => format!("custom:{name}"),
    }
}

pub fn rich_block_kind_from_db(value: &str) -> RichBlockKind {
    if let Some(level) = value.strip_prefix("heading:") {
        return RichBlockKind::Heading {
            level: level.parse::<u8>().unwrap_or(1).clamp(1, 6),
        };
    }
    if let Some(variant) = value.strip_prefix("callout:") {
        return RichBlockKind::Callout {
            variant: callout_variant_from_db(variant),
        };
    }
    if let Some(checked) = value.strip_prefix("todo:") {
        return RichBlockKind::Todo {
            checked: checked == "true",
        };
    }
    if let Some(language) = value.strip_prefix("code:") {
        return RichBlockKind::Code {
            language: Some(language.to_owned()),
        };
    }
    if let Some(name) = value.strip_prefix("custom:") {
        return RichBlockKind::Custom(name.to_owned());
    }

    match value {
        "paragraph" => RichBlockKind::Paragraph,
        "quote" => RichBlockKind::Quote,
        "bulleted_list" => RichBlockKind::BulletedList,
        "numbered_list" => RichBlockKind::NumberedList,
        "toggle" => RichBlockKind::Toggle,
        "code" => RichBlockKind::Code { language: None },
        "math" => RichBlockKind::Math,
        "mermaid" => RichBlockKind::Mermaid,
        "html" => RichBlockKind::Html,
        "table" => RichBlockKind::Table,
        "image" => RichBlockKind::Image,
        "file" => RichBlockKind::File,
        "attachment" => RichBlockKind::Attachment,
        "whiteboard" => RichBlockKind::Whiteboard,
        "mind_map" => RichBlockKind::MindMap,
        "embed" => RichBlockKind::Embed,
        "divider" => RichBlockKind::Divider,
        "separator" => RichBlockKind::Separator,
        "footnote_definition" => RichBlockKind::FootnoteDefinition,
        "comment" => RichBlockKind::Comment,
        "raw_markdown" => RichBlockKind::RawMarkdown,
        "database" => RichBlockKind::Database,
        _ => RichBlockKind::Paragraph,
    }
}

fn callout_variant_to_db(variant: CalloutVariant) -> &'static str {
    match variant {
        CalloutVariant::Note => "note",
        CalloutVariant::Tip => "tip",
        CalloutVariant::Important => "important",
        CalloutVariant::Warning => "warning",
        CalloutVariant::Caution => "caution",
        CalloutVariant::Info => "info",
        CalloutVariant::Success => "success",
        CalloutVariant::Danger => "danger",
    }
}

fn callout_variant_from_db(value: &str) -> CalloutVariant {
    match value {
        "tip" => CalloutVariant::Tip,
        "important" => CalloutVariant::Important,
        "warning" => CalloutVariant::Warning,
        "caution" => CalloutVariant::Caution,
        "info" => CalloutVariant::Info,
        "success" => CalloutVariant::Success,
        "danger" => CalloutVariant::Danger,
        _ => CalloutVariant::Note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_ids_map_to_stable_postgres_uuid_namespace() {
        let document = pg_document_id_from_runtime(42);
        let block = pg_block_id_from_runtime(42);

        assert_ne!(document, block);
        assert_eq!(runtime_document_id_from_pg(document), Some(42));
        assert_eq!(runtime_block_id_from_pg(block), Some(42));
        assert_eq!(runtime_block_id_from_pg(document), None);
    }

    #[test]
    fn block_attrs_round_trip_through_json() {
        let mut attrs = BlockAttrs {
            color: Some("#ff0000".to_owned()),
            background_color: Some("#00ff00".to_owned()),
            text_align: TextAlign::Center,
            indent: 3,
            folded: true,
            locked: true,
            custom: BTreeMap::new(),
        };
        attrs.custom.insert("key".to_owned(), "value".to_owned());

        let encoded = encode_block_attrs(&attrs).unwrap();
        let decoded = decode_block_attrs(encoded).unwrap();

        assert_eq!(decoded, attrs);
    }

    #[test]
    fn block_payload_round_trips_through_json() {
        let payloads = vec![
            BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "hello".to_owned(),
                    marks: vec![
                        InlineMark::Bold,
                        InlineMark::Link {
                            href: "https://example.com".to_owned(),
                        },
                    ],
                }],
            },
            BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {}".to_owned(),
            },
            BlockPayload::Table(TablePayload {
                rows: vec![TableRowPayload {
                    cells: vec![TableCellPayload {
                        spans: vec![InlineSpan::plain("cell")],
                    }],
                }],
                header_rows: 1,
                header_cols: 0,
            }),
            BlockPayload::Image(ImagePayload {
                source: "a.png".to_owned(),
                alt: "alt".to_owned(),
                caption: "caption".to_owned(),
                display_width_ratio_milli: Some(760),
            }),
            BlockPayload::Empty,
        ];

        for payload in payloads {
            let encoded = encode_block_payload(&payload).unwrap();
            let decoded = decode_block_payload(encoded).unwrap();
            assert_eq!(decoded, payload);
        }
    }

    #[test]
    fn edit_transaction_encodes_to_json() {
        let tx = EditTransaction::new(
            7,
            EditTransactionKind::Typing,
            123,
            vec![EditOperation::InsertText {
                block_id: 1,
                offset: 0,
                text: "A".to_owned(),
            }],
            vec![EditOperation::DeleteText {
                block_id: 1,
                range: 0..1,
            }],
        )
        .with_selection(
            Some(DocumentSelection::caret(TextPosition::downstream(1, 0))),
            Some(DocumentSelection::caret(TextPosition::downstream(1, 1))),
        );

        let encoded = encode_edit_transaction(&tx).unwrap();

        assert_eq!(encoded["id"], 7);
        assert_eq!(encoded["kind"], "typing");
        assert_eq!(encoded["ops"][0]["type"], "insert_text");
        assert_eq!(encoded["inverse_ops"][0]["type"], "delete_text");
        assert_eq!(encoded["after_selection"]["focus"]["offset"], 1);
    }

    #[test]
    fn move_block_to_parent_transaction_encodes_to_json() {
        let tx = EditTransaction::new(
            8,
            EditTransactionKind::BlockStructureChange,
            124,
            vec![EditOperation::MoveBlockToParent {
                block_id: 10,
                parent_id: Some(3),
                sibling_index: 2,
            }],
            vec![EditOperation::MoveBlockToParent {
                block_id: 10,
                parent_id: None,
                sibling_index: 4,
            }],
        );

        let encoded = encode_edit_transaction(&tx).unwrap();

        assert_eq!(encoded["kind"], "block_structure_change");
        assert_eq!(encoded["ops"][0]["type"], "move_block_to_parent");
        assert_eq!(encoded["ops"][0]["block_id"], 10);
        assert_eq!(encoded["ops"][0]["parent_id"], 3);
        assert_eq!(encoded["ops"][0]["sibling_index"], 2);
        assert_eq!(
            encoded["inverse_ops"][0]["parent_id"],
            serde_json::Value::Null
        );
        assert_eq!(encoded["inverse_ops"][0]["sibling_index"], 4);
    }

    #[test]
    fn rich_block_kind_round_trips_through_db_string() {
        let kinds = [
            RichBlockKind::Paragraph,
            RichBlockKind::Heading { level: 3 },
            RichBlockKind::Callout {
                variant: CalloutVariant::Warning,
            },
            RichBlockKind::Todo { checked: true },
            RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            RichBlockKind::Database,
            RichBlockKind::Custom("chart".to_owned()),
        ];

        for kind in kinds {
            let encoded = rich_block_kind_to_db(&kind);
            let decoded = rich_block_kind_from_db(&encoded);
            assert_eq!(decoded, kind);
        }
    }
}
