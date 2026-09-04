use std::ops::Range;

use cditor_core::rich_text::{
    BlockAttrs, BlockPayload, InlineColorTarget, InlineMark, RichBlockKind,
};

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct BlockInput {
    pub kind: RichBlockKind,
    pub attrs: BlockAttrs,
    pub payload: BlockPayload,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandEnvelope {
    pub command: EditorCommand,
    pub source: CommandSource,
    pub expected_revision: Option<u64>,
    pub request_id: Option<u64>,
}

impl CommandEnvelope {
    pub const fn new(command: EditorCommand, source: CommandSource) -> Self {
        Self {
            command,
            source,
            expected_revision: None,
            request_id: None,
        }
    }

    pub const fn expecting_revision(mut self, revision: u64) -> Self {
        self.expected_revision = Some(revision);
        self
    }

    pub const fn with_request_id(mut self, request_id: u64) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn invocation(&self) -> CommandInvocation {
        let mut invocation = self.command.invocation(self.source);
        invocation.request_id = self.request_id;
        invocation
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockTransform {
    Kind(RichBlockKind),
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum EditorCommand {
    Undo,
    Redo,
    SelectAll,
    CopySelection,
    CopySelectionAsMarkdown,
    CutSelection,
    PasteClipboard,
    #[doc(hidden)]
    ApplyClipboardData {
        text: String,
        metadata_json: Option<String>,
    },
    #[doc(hidden)]
    ApplyMarkdownImport {
        text: String,
    },
    #[doc(hidden)]
    InsertImageAsset {
        payload: cditor_core::rich_text::ImagePayload,
        asset: Option<cditor_core::edit::AssetSnapshot>,
        after_block_id: Option<cditor_core::ids::BlockId>,
    },
    #[doc(hidden)]
    InsertVideoAsset {
        payload: cditor_core::rich_text::VideoPayload,
        asset: Option<cditor_core::edit::AssetSnapshot>,
        after_block_id: Option<cditor_core::ids::BlockId>,
    },
    #[doc(hidden)]
    SetVideoSource {
        block_id: cditor_core::ids::BlockId,
        source: String,
        title: String,
        media_type: Option<String>,
        asset: Option<cditor_core::edit::AssetSnapshot>,
    },
    #[doc(hidden)]
    SetPageCover {
        source: Option<String>,
        position_y_milli: u16,
    },
    #[doc(hidden)]
    SetPageIconEmoji {
        emoji: Option<String>,
    },
    #[doc(hidden)]
    SetPageIconAsset {
        source: Option<String>,
    },
    DeleteSelection,
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrike,
    ToggleInlineCode,
    SetInlineColor {
        target: InlineColorTarget,
        color: Option<String>,
    },
    /// Sets, re-targets, or clears (`href: None`) the inline link on the
    /// focused selection; `text: Some(_)` also replaces the selected label.
    SetInlineLink {
        href: Option<String>,
        text: Option<String>,
    },
    SetBlockColor {
        block_id: cditor_core::ids::BlockId,
        target: InlineColorTarget,
        color: Option<String>,
    },
    #[doc(hidden)]
    InsertParagraphAfterBlock {
        block_id: cditor_core::ids::BlockId,
    },
    #[doc(hidden)]
    DeleteBlock {
        block_id: cditor_core::ids::BlockId,
    },
    #[doc(hidden)]
    ToggleTodo {
        block_id: cditor_core::ids::BlockId,
    },
    #[doc(hidden)]
    SetCodeLanguage {
        block_id: cditor_core::ids::BlockId,
        language: Option<String>,
    },
    #[doc(hidden)]
    CopyBlockText {
        block_id: cditor_core::ids::BlockId,
    },
    #[doc(hidden)]
    CopyBlockLink {
        block_id: cditor_core::ids::BlockId,
    },
    #[doc(hidden)]
    CopyBlockMarkdown {
        block_id: cditor_core::ids::BlockId,
    },
    #[doc(hidden)]
    MoveBlockBefore {
        block_id: cditor_core::ids::BlockId,
        before_block_id: Option<cditor_core::ids::BlockId>,
    },
    #[doc(hidden)]
    MoveBlockToParent {
        block_id: cditor_core::ids::BlockId,
        parent_id: Option<cditor_core::ids::BlockId>,
        sibling_index: usize,
    },
    #[doc(hidden)]
    ApplyAiPreview {
        mode: AiApplyCommandMode,
    },
    InsertBlock(BlockInput),
    TransformBlock(BlockTransform),
    DeleteSelectedBlocks,
    DuplicateSelectedBlocks,
    InsertTable {
        rows: usize,
        columns: usize,
    },
    InsertImage,
    InsertWhiteboard,
    InsertMermaid,
    FoldHeading,
    UnfoldHeading,
    InsertParagraphAfterFocused,
    #[doc(hidden)]
    EnsureTrailingParagraph,
    InsertSoftLineBreak,
    HandleEnter,
    IndentBlock,
    OutdentBlock,
    DeleteBackward,
    DeleteForward,
    #[doc(hidden)]
    SetDocumentSelection {
        selection: cditor_core::edit::DocumentSelection,
    },
    #[doc(hidden)]
    SetBlockSelectionRange {
        anchor_block_id: cditor_core::ids::BlockId,
        focus_block_id: cditor_core::ids::BlockId,
    },
    #[doc(hidden)]
    FocusBlock {
        block_id: cditor_core::ids::BlockId,
    },
    #[doc(hidden)]
    FocusTableCell {
        block_id: cditor_core::ids::BlockId,
        row: usize,
        col: usize,
        offset: Option<usize>,
        affinity: cditor_core::edit::TextAffinity,
    },
    #[doc(hidden)]
    BlurTableCell,
    #[doc(hidden)]
    SetTableCellSelection {
        block_id: cditor_core::ids::BlockId,
        row: usize,
        col: usize,
        anchor_offset: usize,
        focus_offset: usize,
        focus_affinity: cditor_core::edit::TextAffinity,
    },
    #[doc(hidden)]
    NavigateTableCell {
        direction: TableCellNavigationDirection,
        extend_selection: bool,
    },
    #[doc(hidden)]
    SetTextSurfaceSelection {
        surface_id: cditor_core::ids::SurfaceId,
        anchor_offset: usize,
        focus_offset: usize,
        focus_affinity: cditor_core::edit::TextAffinity,
    },
    MoveCaret {
        direction: CaretDirection,
        extend_selection: bool,
    },
    #[doc(hidden)]
    ApplySlashBlock {
        block_id: cditor_core::ids::BlockId,
        trigger_range: Range<usize>,
        kind: RichBlockKind,
    },
    #[doc(hidden)]
    TableToggleHeader {
        block_id: cditor_core::ids::BlockId,
        axis: TableAxis,
    },
    #[doc(hidden)]
    TableInsertAxis {
        block_id: cditor_core::ids::BlockId,
        axis: TableAxis,
        index: usize,
        count: usize,
    },
    #[doc(hidden)]
    TableDeleteAxis {
        block_id: cditor_core::ids::BlockId,
        axis: TableAxis,
        index: usize,
    },
    #[doc(hidden)]
    TableDuplicateAxis {
        block_id: cditor_core::ids::BlockId,
        axis: TableAxis,
        index: usize,
    },
    #[doc(hidden)]
    TableClearRange {
        block_id: cditor_core::ids::BlockId,
        range: cditor_core::rich_text::TableRange,
    },
    #[doc(hidden)]
    TableSetRangeBackground {
        block_id: cditor_core::ids::BlockId,
        range: cditor_core::rich_text::TableRange,
        color: Option<String>,
    },
    TableMergeCells {
        block_id: cditor_core::ids::BlockId,
        range: cditor_core::rich_text::TableRange,
    },
    TableSplitCell {
        block_id: cditor_core::ids::BlockId,
        row: usize,
        col: usize,
    },
    TableSetRangeAlign {
        block_id: cditor_core::ids::BlockId,
        range: cditor_core::rich_text::TableRange,
        align: cditor_core::rich_text::TableCellAlign,
    },
    #[doc(hidden)]
    SetMediaWidthRatio {
        block_id: cditor_core::ids::BlockId,
        ratio_milli: u16,
    },
    #[doc(hidden)]
    UpdateWhiteboardScene {
        block_id: cditor_core::ids::BlockId,
        scene_json: String,
    },
    #[doc(hidden)]
    TableResizeAxis {
        block_id: cditor_core::ids::BlockId,
        axis: TableAxis,
        index: usize,
        size_px: u16,
    },
    #[doc(hidden)]
    TableMoveAxis {
        block_id: cditor_core::ids::BlockId,
        axis: TableAxis,
        from_index: usize,
        to_index: usize,
    },
}

impl EditorCommand {
    pub const fn stable_id(&self) -> &'static str {
        match self {
            Self::Undo => "edit.undo",
            Self::Redo => "edit.redo",
            Self::SelectAll => "edit.select_all",
            Self::CopySelection => "edit.copy",
            Self::CopySelectionAsMarkdown => builtin::EDIT_COPY_AS_MARKDOWN,
            Self::CutSelection => "edit.cut",
            Self::PasteClipboard => "edit.paste",
            Self::ApplyClipboardData { .. } => builtin::EDIT_APPLY_CLIPBOARD_DATA,
            Self::ApplyMarkdownImport { .. } => builtin::EDIT_APPLY_MARKDOWN_IMPORT,
            Self::InsertImageAsset { .. } => builtin::ASSET_INSERT_IMAGE_PAYLOAD,
            Self::InsertVideoAsset { .. } => builtin::ASSET_INSERT_VIDEO_PAYLOAD,
            Self::SetVideoSource { .. } => builtin::VIDEO_SET_SOURCE,
            Self::SetPageCover { .. } => builtin::DOCUMENT_SET_COVER,
            Self::SetPageIconEmoji { .. } => builtin::DOCUMENT_SET_ICON,
            Self::SetPageIconAsset { .. } => builtin::DOCUMENT_SET_ICON,
            Self::DeleteSelection => "edit.delete_selection",
            Self::ToggleBold => "format.toggle_bold",
            Self::ToggleItalic => "format.toggle_italic",
            Self::ToggleUnderline => "format.toggle_underline",
            Self::ToggleStrike => "format.toggle_strike",
            Self::ToggleInlineCode => "format.toggle_inline_code",
            Self::SetInlineColor { .. } => builtin::FORMAT_SET_COLOR,
            Self::SetInlineLink { .. } => "format.set_link",
            Self::SetBlockColor { .. } => builtin::BLOCK_SET_COLOR,
            Self::InsertParagraphAfterBlock { .. } => builtin::BLOCK_INSERT_AFTER,
            Self::DeleteBlock { .. } => builtin::BLOCK_DELETE,
            Self::ToggleTodo { .. } => builtin::BLOCK_TOGGLE_TODO,
            Self::SetCodeLanguage { .. } => builtin::CODE_SET_LANGUAGE,
            Self::CopyBlockText { .. } => builtin::BLOCK_COPY_TEXT,
            Self::CopyBlockLink { .. } => builtin::BLOCK_COPY_LINK,
            Self::CopyBlockMarkdown { .. } => builtin::BLOCK_COPY_MARKDOWN,
            Self::MoveBlockBefore { .. } => builtin::BLOCK_MOVE_BEFORE,
            Self::MoveBlockToParent { .. } => builtin::BLOCK_MOVE_TO_PARENT,
            Self::ApplyAiPreview { .. } => builtin::AI_APPLY,
            Self::InsertBlock(_) => "block.insert",
            Self::TransformBlock(_) => "block.transform",
            Self::DeleteSelectedBlocks => "block.delete_selected",
            Self::DuplicateSelectedBlocks => "block.duplicate_selected",
            Self::InsertTable { .. } => "block.insert_table",
            Self::InsertImage => "block.insert_image",
            Self::InsertWhiteboard => "block.insert_whiteboard",
            Self::InsertMermaid => "block.insert_mermaid",
            Self::FoldHeading => "heading.fold",
            Self::UnfoldHeading => "heading.unfold",
            Self::InsertParagraphAfterFocused => builtin::BLOCK_INSERT_AFTER_FOCUSED,
            Self::EnsureTrailingParagraph => builtin::BLOCK_ENSURE_TRAILING_PARAGRAPH,
            Self::InsertSoftLineBreak => builtin::TEXT_INSERT_SOFT_BREAK,
            Self::HandleEnter => builtin::BLOCK_ENTER,
            Self::IndentBlock => builtin::BLOCK_INDENT,
            Self::OutdentBlock => builtin::BLOCK_OUTDENT,
            Self::DeleteBackward => builtin::TEXT_DELETE_BACKWARD,
            Self::DeleteForward => builtin::TEXT_DELETE_FORWARD,
            Self::SetDocumentSelection { .. } => builtin::SELECTION_SET_DOCUMENT,
            Self::SetBlockSelectionRange { .. } => builtin::SELECTION_SET_BLOCK_RANGE,
            Self::FocusBlock { .. } => builtin::SELECTION_FOCUS_BLOCK,
            Self::FocusTableCell { .. } => builtin::SELECTION_FOCUS_TABLE_CELL,
            Self::BlurTableCell => builtin::SELECTION_BLUR_TABLE_CELL,
            Self::SetTableCellSelection { .. } => builtin::SELECTION_SET_TABLE_CELL,
            Self::NavigateTableCell { .. } => builtin::SELECTION_NAVIGATE_TABLE_CELL,
            Self::SetTextSurfaceSelection { .. } => builtin::SELECTION_SET_TEXT_SURFACE,
            Self::MoveCaret { .. } => builtin::TEXT_MOVE_CARET,
            Self::ApplySlashBlock { .. } => builtin::BLOCK_APPLY_SLASH,
            Self::TableToggleHeader { .. } => builtin::TABLE_TOGGLE_HEADER,
            Self::TableInsertAxis { .. } => builtin::TABLE_INSERT_AXIS,
            Self::TableDeleteAxis { .. } => builtin::TABLE_DELETE_AXIS,
            Self::TableDuplicateAxis { .. } => builtin::TABLE_DUPLICATE_AXIS,
            Self::TableClearRange { .. } => builtin::TABLE_CLEAR_RANGE,
            Self::TableSetRangeBackground { .. } => builtin::TABLE_SET_RANGE_COLOR,
            Self::TableMergeCells { .. } => builtin::TABLE_MERGE_CELLS,
            Self::TableSplitCell { .. } => builtin::TABLE_SPLIT_CELL,
            Self::TableSetRangeAlign { .. } => builtin::TABLE_SET_ALIGN,
            Self::SetMediaWidthRatio { .. } => builtin::MEDIA_SET_WIDTH_RATIO,
            Self::UpdateWhiteboardScene { .. } => builtin::WHITEBOARD_UPDATE_SCENE,
            Self::TableResizeAxis { .. } => builtin::TABLE_RESIZE_AXIS,
            Self::TableMoveAxis { .. } => builtin::TABLE_MOVE_AXIS,
        }
    }

    pub fn invocation(&self, source: CommandSource) -> CommandInvocation {
        CommandInvocation::new(
            CommandId::builtin(self.stable_id()),
            self.arguments(),
            source,
        )
    }

    pub fn arguments(&self) -> CommandArgs {
        match self {
            Self::ApplyClipboardData {
                text,
                metadata_json,
            } => CommandArgs::ClipboardData {
                text: text.clone(),
                metadata_json: metadata_json.clone(),
            },
            Self::ApplyMarkdownImport { text } => CommandArgs::Markdown { text: text.clone() },
            Self::InsertImageAsset {
                payload,
                asset,
                after_block_id,
            } => CommandArgs::ImageAsset {
                payload: payload.clone(),
                asset: asset.clone(),
                after_block_id: *after_block_id,
            },
            Self::InsertVideoAsset {
                payload,
                asset,
                after_block_id,
            } => CommandArgs::VideoAsset {
                payload: payload.clone(),
                asset: asset.clone(),
                after_block_id: *after_block_id,
            },
            Self::SetVideoSource {
                block_id,
                source,
                title,
                media_type,
                asset,
            } => CommandArgs::VideoSource {
                block_id: *block_id,
                source: source.clone(),
                title: title.clone(),
                media_type: media_type.clone(),
                asset: asset.clone(),
            },
            Self::SetPageCover {
                source,
                position_y_milli,
            } => CommandArgs::Extension {
                schema: "cditor.page_cover.v1".to_owned(),
                payload: serde_json::json!({
                    "source": source,
                    "position_y_milli": position_y_milli,
                }),
            },
            Self::SetPageIconEmoji { emoji } => CommandArgs::Extension {
                schema: "cditor.page_icon.v1".to_owned(),
                payload: serde_json::json!({ "emoji": emoji }),
            },
            Self::SetPageIconAsset { source } => CommandArgs::Extension {
                schema: "cditor.page_icon_asset.v1".to_owned(),
                payload: serde_json::json!({ "source": source }),
            },
            Self::ToggleBold => CommandArgs::InlineMark(InlineMark::Bold),
            Self::ToggleItalic => CommandArgs::InlineMark(InlineMark::Italic),
            Self::ToggleUnderline => CommandArgs::InlineMark(InlineMark::Underline),
            Self::ToggleStrike => CommandArgs::InlineMark(InlineMark::Strike),
            Self::ToggleInlineCode => CommandArgs::InlineMark(InlineMark::Code),
            Self::SetInlineColor { target, color } => CommandArgs::InlineColor {
                target: *target,
                color: color.clone(),
            },
            Self::SetInlineLink { href, text } => CommandArgs::InlineLink {
                href: href.clone(),
                text: text.clone(),
            },
            Self::SetBlockColor {
                block_id,
                target,
                color,
            } => CommandArgs::BlockColor {
                block_id: *block_id,
                target: *target,
                color: color.clone(),
            },
            Self::InsertParagraphAfterBlock { block_id }
            | Self::DeleteBlock { block_id }
            | Self::ToggleTodo { block_id }
            | Self::CopyBlockText { block_id }
            | Self::CopyBlockLink { block_id }
            | Self::CopyBlockMarkdown { block_id }
            | Self::FocusBlock { block_id } => CommandArgs::BlockTarget {
                block_id: *block_id,
            },
            Self::MoveBlockBefore {
                block_id,
                before_block_id,
            } => CommandArgs::BlockMoveBefore {
                block_id: *block_id,
                before_block_id: *before_block_id,
            },
            Self::MoveBlockToParent {
                block_id,
                parent_id,
                sibling_index,
            } => CommandArgs::BlockMoveToParent {
                block_id: *block_id,
                parent_id: *parent_id,
                sibling_index: *sibling_index,
            },
            Self::SetCodeLanguage { block_id, language } => CommandArgs::CodeLanguage {
                block_id: *block_id,
                language: language.clone(),
            },
            Self::ApplyAiPreview { mode } => CommandArgs::AiApply { mode: *mode },
            Self::InsertBlock(input) => CommandArgs::InsertBlock {
                kind: input.kind.clone(),
                attrs: input.attrs.clone(),
                payload: input.payload.clone(),
            },
            Self::TransformBlock(BlockTransform::Kind(kind)) => {
                CommandArgs::BlockKind(kind.clone())
            }
            Self::InsertTable { rows, columns } => CommandArgs::InsertTable {
                rows: *rows,
                columns: *columns,
            },
            Self::InsertImage => CommandArgs::BlockKind(RichBlockKind::Image),
            Self::InsertWhiteboard => CommandArgs::BlockKind(RichBlockKind::Whiteboard),
            Self::InsertMermaid => CommandArgs::BlockKind(RichBlockKind::Mermaid),
            Self::MoveCaret {
                direction,
                extend_selection,
            } => CommandArgs::MoveCaret {
                direction: *direction,
                extend_selection: *extend_selection,
            },
            Self::SetDocumentSelection { selection } => CommandArgs::DocumentSelection(*selection),
            Self::SetBlockSelectionRange {
                anchor_block_id,
                focus_block_id,
            } => CommandArgs::BlockSelectionRange {
                anchor_block_id: *anchor_block_id,
                focus_block_id: *focus_block_id,
            },
            Self::FocusTableCell {
                block_id,
                row,
                col,
                offset,
                affinity,
            } => CommandArgs::TableCellFocus {
                block_id: *block_id,
                row: *row,
                col: *col,
                offset: *offset,
                affinity: *affinity,
            },
            Self::SetTextSurfaceSelection {
                surface_id,
                anchor_offset,
                focus_offset,
                focus_affinity,
            } => CommandArgs::TextSurfaceSelection {
                surface_id: *surface_id,
                anchor_offset: *anchor_offset,
                focus_offset: *focus_offset,
                focus_affinity: *focus_affinity,
            },
            Self::SetTableCellSelection {
                block_id,
                row,
                col,
                anchor_offset,
                focus_offset,
                focus_affinity,
            } => CommandArgs::TableCellSelection {
                block_id: *block_id,
                row: *row,
                col: *col,
                anchor_offset: *anchor_offset,
                focus_offset: *focus_offset,
                focus_affinity: *focus_affinity,
            },
            Self::NavigateTableCell {
                direction,
                extend_selection,
            } => CommandArgs::TableCellNavigation {
                direction: *direction,
                extend_selection: *extend_selection,
            },
            Self::ApplySlashBlock {
                block_id,
                trigger_range,
                kind,
            } => CommandArgs::SlashBlock {
                block_id: *block_id,
                start: trigger_range.start,
                end: trigger_range.end,
                kind: kind.clone(),
            },
            Self::TableToggleHeader { block_id, axis }
            | Self::TableDeleteAxis {
                block_id,
                axis,
                index: _,
            }
            | Self::TableDuplicateAxis {
                block_id,
                axis,
                index: _,
            } => CommandArgs::TableAxisTarget {
                block_id: *block_id,
                axis: *axis,
                index: match self {
                    Self::TableDeleteAxis { index, .. }
                    | Self::TableDuplicateAxis { index, .. } => *index,
                    _ => 0,
                },
            },
            Self::TableInsertAxis {
                block_id,
                axis,
                index,
                count,
            } => CommandArgs::TableInsertAxis {
                block_id: *block_id,
                axis: *axis,
                index: *index,
                count: *count,
            },
            Self::TableClearRange { block_id, range } => CommandArgs::TableRangeTarget {
                block_id: *block_id,
                range: *range,
            },
            Self::TableSetRangeBackground {
                block_id,
                range,
                color,
            } => CommandArgs::TableRangeColor {
                block_id: *block_id,
                range: *range,
                color: color.clone(),
            },
            Self::TableMergeCells { block_id, range } => CommandArgs::TableRangeTarget {
                block_id: *block_id,
                range: *range,
            },
            Self::TableSplitCell { block_id, row, col } => CommandArgs::TableRangeTarget {
                block_id: *block_id,
                range: cditor_core::rich_text::TableRange::normalized(*row, *col, *row, *col),
            },
            Self::TableSetRangeAlign {
                block_id,
                range,
                align,
            } => CommandArgs::TableRangeAlign {
                block_id: *block_id,
                range: *range,
                align: *align,
            },
            Self::SetMediaWidthRatio {
                block_id,
                ratio_milli,
            } => CommandArgs::MediaWidthRatio {
                block_id: *block_id,
                ratio_milli: *ratio_milli,
            },
            Self::UpdateWhiteboardScene {
                block_id,
                scene_json,
            } => CommandArgs::WhiteboardScene {
                block_id: *block_id,
                scene_json: scene_json.clone(),
            },
            Self::TableResizeAxis {
                block_id,
                axis,
                index,
                size_px,
            } => CommandArgs::TableAxisResize {
                block_id: *block_id,
                axis: *axis,
                index: *index,
                size_px: *size_px,
            },
            Self::TableMoveAxis {
                block_id,
                axis,
                from_index,
                to_index,
            } => CommandArgs::TableAxisMove {
                block_id: *block_id,
                axis: *axis,
                from_index: *from_index,
                to_index: *to_index,
            },
            _ => CommandArgs::None,
        }
    }
}
