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
    InsertSoftLineBreak,
    HandleEnter,
    IndentBlock,
    OutdentBlock,
    DeleteBackward,
    DeleteForward,
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
            Self::InsertSoftLineBreak => builtin::TEXT_INSERT_SOFT_BREAK,
            Self::HandleEnter => builtin::BLOCK_ENTER,
            Self::IndentBlock => builtin::BLOCK_INDENT,
            Self::OutdentBlock => builtin::BLOCK_OUTDENT,
            Self::DeleteBackward => builtin::TEXT_DELETE_BACKWARD,
            Self::DeleteForward => builtin::TEXT_DELETE_FORWARD,
            Self::MoveCaret { .. } => builtin::TEXT_MOVE_CARET,
            Self::ApplySlashBlock { .. } => builtin::BLOCK_APPLY_SLASH,
            Self::TableToggleHeader { .. } => builtin::TABLE_TOGGLE_HEADER,
            Self::TableInsertAxis { .. } => builtin::TABLE_INSERT_AXIS,
            Self::TableDeleteAxis { .. } => builtin::TABLE_DELETE_AXIS,
            Self::TableDuplicateAxis { .. } => builtin::TABLE_DUPLICATE_AXIS,
            Self::TableClearRange { .. } => builtin::TABLE_CLEAR_RANGE,
            Self::TableSetRangeBackground { .. } => builtin::TABLE_SET_RANGE_COLOR,
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
            | Self::CopyBlockText { block_id } => CommandArgs::BlockTarget {
                block_id: *block_id,
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
}
