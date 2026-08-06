use cditor_core::{
    edit::DocumentSelection,
    ids::BlockId,
    rich_text::{
        BlockAttrs, BlockPayload, ImagePayload, InlineColorTarget, InlineMark, RichBlockKind,
        TableCellAlign, TableRange,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    Keyboard,
    Toolbar,
    SlashMenu,
    ContextMenu,
    Sdk,
    Automation,
    Ai,
    Plugin,
    Import,
    Ime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaretDirection {
    PreviousVisual,
    NextVisual,
    PreviousWord,
    NextWord,
    PreviousLine,
    NextLine,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableCellNavigationDirection {
    Left,
    Right,
    Up,
    Down,
    TabForward,
    TabBackward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableAxis {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiApplyCommandMode {
    Replace,
    InsertAfter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CommandArgs {
    None,
    Text(String),
    ReplaceText {
        block_id: BlockId,
        start: usize,
        end: usize,
        text: String,
    },
    ClipboardData {
        text: String,
        metadata_json: Option<String>,
    },
    Markdown {
        text: String,
    },
    ImageAsset {
        payload: ImagePayload,
        asset: Option<cditor_core::edit::AssetSnapshot>,
        after_block_id: Option<BlockId>,
    },
    MoveCaret {
        direction: CaretDirection,
        extend_selection: bool,
    },
    DocumentSelection(DocumentSelection),
    BlockSelectionRange {
        anchor_block_id: BlockId,
        focus_block_id: BlockId,
    },
    TableCellFocus {
        block_id: BlockId,
        row: usize,
        col: usize,
        offset: Option<usize>,
        affinity: cditor_core::edit::TextAffinity,
    },
    TableCellSelection {
        block_id: BlockId,
        row: usize,
        col: usize,
        anchor_offset: usize,
        focus_offset: usize,
        focus_affinity: cditor_core::edit::TextAffinity,
    },
    TableCellNavigation {
        direction: TableCellNavigationDirection,
        extend_selection: bool,
    },
    TextSurfaceSelection {
        surface_id: cditor_core::ids::SurfaceId,
        anchor_offset: usize,
        focus_offset: usize,
        focus_affinity: cditor_core::edit::TextAffinity,
    },
    InlineMark(InlineMark),
    InlineColor {
        target: InlineColorTarget,
        color: Option<String>,
    },
    BlockColor {
        block_id: BlockId,
        target: InlineColorTarget,
        color: Option<String>,
    },
    CodeLanguage {
        block_id: BlockId,
        language: Option<String>,
    },
    AiApply {
        mode: AiApplyCommandMode,
    },
    BlockKind(RichBlockKind),
    InsertBlock {
        kind: RichBlockKind,
        attrs: BlockAttrs,
        payload: BlockPayload,
    },
    SlashBlock {
        block_id: BlockId,
        start: usize,
        end: usize,
        kind: RichBlockKind,
    },
    BlockTarget {
        block_id: BlockId,
    },
    BlockMoveBefore {
        block_id: BlockId,
        before_block_id: Option<BlockId>,
    },
    BlockMoveToParent {
        block_id: BlockId,
        parent_id: Option<BlockId>,
        sibling_index: usize,
    },
    InsertTable {
        rows: usize,
        columns: usize,
    },
    TableAxisTarget {
        block_id: BlockId,
        axis: TableAxis,
        index: usize,
    },
    TableInsertAxis {
        block_id: BlockId,
        axis: TableAxis,
        index: usize,
        count: usize,
    },
    MediaWidthRatio {
        block_id: BlockId,
        ratio_milli: u16,
    },
    WhiteboardScene {
        block_id: BlockId,
        scene_json: String,
    },
    TableAxisResize {
        block_id: BlockId,
        axis: TableAxis,
        index: usize,
        size_px: u16,
    },
    TableAxisMove {
        block_id: BlockId,
        axis: TableAxis,
        from_index: usize,
        to_index: usize,
    },
    TableRangeTarget {
        block_id: BlockId,
        range: TableRange,
    },
    TableRangeColor {
        block_id: BlockId,
        range: TableRange,
        color: Option<String>,
    },
    TableRangeAlign {
        block_id: BlockId,
        range: TableRange,
        align: TableCellAlign,
    },
    Extension {
        schema: String,
        payload: serde_json::Value,
    },
}

impl CommandArgs {
    pub const fn kind(&self) -> CommandArgumentKind {
        match self {
            Self::None => CommandArgumentKind::None,
            Self::Text(_) => CommandArgumentKind::Text,
            Self::ReplaceText { .. } => CommandArgumentKind::ReplaceText,
            Self::ClipboardData { .. } => CommandArgumentKind::ClipboardData,
            Self::Markdown { .. } => CommandArgumentKind::Markdown,
            Self::ImageAsset { .. } => CommandArgumentKind::ImageAsset,
            Self::MoveCaret { .. } => CommandArgumentKind::MoveCaret,
            Self::DocumentSelection(_) => CommandArgumentKind::DocumentSelection,
            Self::BlockSelectionRange { .. } => CommandArgumentKind::BlockSelectionRange,
            Self::TableCellFocus { .. } => CommandArgumentKind::TableCellFocus,
            Self::TableCellSelection { .. } => CommandArgumentKind::TableCellSelection,
            Self::TableCellNavigation { .. } => CommandArgumentKind::TableCellNavigation,
            Self::TextSurfaceSelection { .. } => CommandArgumentKind::TextSurfaceSelection,
            Self::InlineMark(_) => CommandArgumentKind::InlineMark,
            Self::InlineColor { .. } => CommandArgumentKind::InlineColor,
            Self::BlockColor { .. } => CommandArgumentKind::BlockColor,
            Self::CodeLanguage { .. } => CommandArgumentKind::CodeLanguage,
            Self::AiApply { .. } => CommandArgumentKind::AiApply,
            Self::BlockKind(_) => CommandArgumentKind::BlockKind,
            Self::InsertBlock { .. } => CommandArgumentKind::InsertBlock,
            Self::SlashBlock { .. } => CommandArgumentKind::SlashBlock,
            Self::BlockTarget { .. } => CommandArgumentKind::BlockTarget,
            Self::BlockMoveBefore { .. } => CommandArgumentKind::BlockMoveBefore,
            Self::BlockMoveToParent { .. } => CommandArgumentKind::BlockMoveToParent,
            Self::InsertTable { .. } => CommandArgumentKind::InsertTable,
            Self::TableAxisTarget { .. } => CommandArgumentKind::TableAxisTarget,
            Self::TableInsertAxis { .. } => CommandArgumentKind::TableInsertAxis,
            Self::MediaWidthRatio { .. } => CommandArgumentKind::MediaWidthRatio,
            Self::WhiteboardScene { .. } => CommandArgumentKind::WhiteboardScene,
            Self::TableAxisResize { .. } => CommandArgumentKind::TableAxisResize,
            Self::TableAxisMove { .. } => CommandArgumentKind::TableAxisMove,
            Self::TableRangeTarget { .. } => CommandArgumentKind::TableRangeTarget,
            Self::TableRangeColor { .. } => CommandArgumentKind::TableRangeColor,
            Self::TableRangeAlign { .. } => CommandArgumentKind::TableRangeAlign,
            Self::Extension { .. } => CommandArgumentKind::Extension,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandArgumentKind {
    None,
    Text,
    ReplaceText,
    ClipboardData,
    Markdown,
    ImageAsset,
    MoveCaret,
    DocumentSelection,
    BlockSelectionRange,
    TableCellFocus,
    TableCellSelection,
    TableCellNavigation,
    TextSurfaceSelection,
    InlineMark,
    InlineColor,
    BlockColor,
    CodeLanguage,
    AiApply,
    BlockKind,
    InsertBlock,
    SlashBlock,
    BlockTarget,
    BlockMoveBefore,
    BlockMoveToParent,
    InsertTable,
    TableAxisTarget,
    TableInsertAxis,
    MediaWidthRatio,
    WhiteboardScene,
    TableAxisResize,
    TableAxisMove,
    TableRangeTarget,
    TableRangeColor,
    TableRangeAlign,
    Extension,
}
