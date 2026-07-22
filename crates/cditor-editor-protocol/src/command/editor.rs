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
    CutSelection,
    PasteClipboard,
    #[doc(hidden)]
    ApplyClipboardData {
        text: String,
        metadata_json: Option<String>,
    },
    #[doc(hidden)]
    InsertImageAsset {
        payload: cditor_core::rich_text::ImagePayload,
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
    #[doc(hidden)]
    SetMediaWidthRatio {
        block_id: cditor_core::ids::BlockId,
        ratio_milli: u16,
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
            Self::CutSelection => "edit.cut",
            Self::PasteClipboard => "edit.paste",
            Self::ApplyClipboardData { .. } => builtin::EDIT_APPLY_CLIPBOARD_DATA,
            Self::InsertImageAsset { .. } => builtin::ASSET_INSERT_IMAGE_PAYLOAD,
            Self::DeleteSelection => "edit.delete_selection",
            Self::ToggleBold => "format.toggle_bold",
            Self::ToggleItalic => "format.toggle_italic",
            Self::ToggleUnderline => "format.toggle_underline",
            Self::ToggleStrike => "format.toggle_strike",
            Self::ToggleInlineCode => "format.toggle_inline_code",
            Self::SetInlineColor { .. } => builtin::FORMAT_SET_COLOR,
            Self::SetBlockColor { .. } => builtin::BLOCK_SET_COLOR,
            Self::InsertParagraphAfterBlock { .. } => builtin::BLOCK_INSERT_AFTER,
            Self::DeleteBlock { .. } => builtin::BLOCK_DELETE,
            Self::ToggleTodo { .. } => builtin::BLOCK_TOGGLE_TODO,
            Self::SetCodeLanguage { .. } => builtin::CODE_SET_LANGUAGE,
            Self::CopyBlockText { .. } => builtin::BLOCK_COPY_TEXT,
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
            Self::FocusBlock { .. } => builtin::SELECTION_FOCUS_BLOCK,
            Self::FocusTableCell { .. } => builtin::SELECTION_FOCUS_TABLE_CELL,
            Self::BlurTableCell => builtin::SELECTION_BLUR_TABLE_CELL,
            Self::MoveCaret { .. } => builtin::TEXT_MOVE_CARET,
            Self::ApplySlashBlock { .. } => builtin::BLOCK_APPLY_SLASH,
            Self::TableToggleHeader { .. } => builtin::TABLE_TOGGLE_HEADER,
            Self::TableInsertAxis { .. } => builtin::TABLE_INSERT_AXIS,
            Self::TableDeleteAxis { .. } => builtin::TABLE_DELETE_AXIS,
            Self::TableDuplicateAxis { .. } => builtin::TABLE_DUPLICATE_AXIS,
            Self::TableClearRange { .. } => builtin::TABLE_CLEAR_RANGE,
            Self::TableSetRangeBackground { .. } => builtin::TABLE_SET_RANGE_COLOR,
            Self::SetMediaWidthRatio { .. } => builtin::MEDIA_SET_WIDTH_RATIO,
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
            Self::InsertImageAsset { payload } => CommandArgs::ImageAsset {
                payload: payload.clone(),
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
            } => CommandArgs::TableInsertAxis {
                block_id: *block_id,
                axis: *axis,
                index: *index,
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
            Self::SetMediaWidthRatio {
                block_id,
                ratio_milli,
            } => CommandArgs::MediaWidthRatio {
                block_id: *block_id,
                ratio_milli: *ratio_milli,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_commands_share_stable_ids_and_typed_arguments() {
        let command = EditorCommand::SetBlockColor {
            block_id: 7,
            target: InlineColorTarget::Background,
            color: Some("#ffffff".to_owned()),
        };
        assert_eq!(command.stable_id(), builtin::BLOCK_SET_COLOR);
        assert!(matches!(
            command.arguments(),
            CommandArgs::BlockColor {
                block_id: 7,
                target: InlineColorTarget::Background,
                color: Some(_),
            }
        ));
    }

    #[test]
    fn editor_command_invocation_keeps_source_and_catalog_contract() {
        let command = EditorCommand::MoveCaret {
            direction: CaretDirection::NextWord,
            extend_selection: true,
        };
        let invocation = command.invocation(CommandSource::Keyboard);
        assert_eq!(invocation.id.as_str(), builtin::TEXT_MOVE_CARET);
        assert_eq!(invocation.source, CommandSource::Keyboard);
        assert_eq!(
            CommandCatalog::builtin().validate_invocation(&invocation),
            Ok(())
        );
    }

    #[test]
    fn envelope_keeps_optimistic_revision_and_request_identity() {
        let envelope = CommandEnvelope::new(EditorCommand::Undo, CommandSource::Sdk)
            .expecting_revision(9)
            .with_request_id(12);
        assert_eq!(envelope.expected_revision, Some(9));
        assert_eq!(envelope.invocation().request_id, Some(12));
    }

    #[test]
    fn drag_commit_commands_keep_typed_catalog_arguments() {
        let commands = [
            EditorCommand::MoveBlockBefore {
                block_id: 2,
                before_block_id: Some(4),
            },
            EditorCommand::MoveBlockToParent {
                block_id: 3,
                parent_id: Some(2),
                sibling_index: 0,
            },
            EditorCommand::ApplyClipboardData {
                text: "plain".to_owned(),
                metadata_json: None,
            },
            EditorCommand::InsertImageAsset {
                payload: cditor_core::rich_text::ImagePayload::default(),
            },
            EditorCommand::SetMediaWidthRatio {
                block_id: 7,
                ratio_milli: 750,
            },
            EditorCommand::TableResizeAxis {
                block_id: 8,
                axis: TableAxis::Column,
                index: 2,
                size_px: 180,
            },
            EditorCommand::TableMoveAxis {
                block_id: 8,
                axis: TableAxis::Row,
                from_index: 1,
                to_index: 3,
            },
        ];
        let catalog = CommandCatalog::builtin();
        for command in commands {
            assert_eq!(
                catalog.validate_invocation(&command.invocation(CommandSource::Toolbar)),
                Ok(())
            );
        }
    }

    #[test]
    fn down_placer_command_has_a_stable_catalog_contract() {
        let command = EditorCommand::EnsureTrailingParagraph;
        let invocation = command.invocation(CommandSource::Toolbar);

        assert_eq!(
            invocation.id.as_str(),
            builtin::BLOCK_ENSURE_TRAILING_PARAGRAPH
        );
        assert_eq!(invocation.args, CommandArgs::None);
        assert_eq!(
            CommandCatalog::builtin().validate_invocation(&invocation),
            Ok(())
        );
    }

    #[test]
    fn document_selection_command_preserves_direction_and_affinity() {
        let selection = cditor_core::edit::DocumentSelection {
            anchor: cditor_core::edit::TextPosition {
                block_id: 2,
                offset: 4,
                affinity: cditor_core::edit::TextAffinity::Upstream,
            },
            focus: cditor_core::edit::TextPosition::downstream(1, 1),
        };
        let command = EditorCommand::SetDocumentSelection { selection };
        let invocation = command.invocation(CommandSource::Sdk);

        assert_eq!(invocation.id.as_str(), builtin::SELECTION_SET_DOCUMENT);
        assert_eq!(invocation.args, CommandArgs::DocumentSelection(selection));
        assert_eq!(
            CommandCatalog::builtin().validate_invocation(&invocation),
            Ok(())
        );
    }

    #[test]
    fn block_focus_command_uses_a_read_only_typed_target() {
        let command = EditorCommand::FocusBlock { block_id: 9 };
        let invocation = command.invocation(CommandSource::Toolbar);

        assert_eq!(invocation.id.as_str(), builtin::SELECTION_FOCUS_BLOCK);
        assert_eq!(invocation.args, CommandArgs::BlockTarget { block_id: 9 });
        let definition = CommandCatalog::builtin()
            .definition(&invocation.id)
            .cloned()
            .expect("focus command must be registered");
        assert_eq!(definition.mutability, CommandMutability::ReadOnly);
    }

    #[test]
    fn table_cell_focus_command_preserves_geometry_adapter_output() {
        let command = EditorCommand::FocusTableCell {
            block_id: 8,
            row: 2,
            col: 3,
            offset: Some(5),
            affinity: cditor_core::edit::TextAffinity::Upstream,
        };
        let invocation = command.invocation(CommandSource::Toolbar);

        assert_eq!(invocation.id.as_str(), builtin::SELECTION_FOCUS_TABLE_CELL);
        assert!(matches!(
            invocation.args,
            CommandArgs::TableCellFocus {
                block_id: 8,
                row: 2,
                col: 3,
                offset: Some(5),
                affinity: cditor_core::edit::TextAffinity::Upstream,
            }
        ));
        assert_eq!(
            CommandCatalog::builtin().validate_invocation(&invocation),
            Ok(())
        );
    }
}
