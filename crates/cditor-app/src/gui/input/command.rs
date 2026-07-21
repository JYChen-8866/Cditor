#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiInputCommand {
    Ignore,
    ToggleDebugOverlay,
    SelectAllFocusedText,
    CopySelection,
    CutSelection,
    PasteClipboard,
    UndoFocusedBlock,
    RedoFocusedBlock,
    InsertParagraphAfterFocused,
    InsertSoftLineBreak,
    HandleEnter,
    IndentBlock,
    OutdentBlock,
    DeleteBackward,
    DeleteForward,
    MoveCaretLeft { extend_selection: bool },
    MoveCaretRight { extend_selection: bool },
    MoveCaretToPreviousWord { extend_selection: bool },
    MoveCaretToNextWord { extend_selection: bool },
    MoveCaretToDocumentStart { extend_selection: bool },
    MoveCaretToDocumentEnd { extend_selection: bool },
    MoveCaretUp { extend_selection: bool },
    MoveCaretDown { extend_selection: bool },
    MoveCaretToLineStart { extend_selection: bool },
    MoveCaretToLineEnd { extend_selection: bool },
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleInlineCode,
}

impl GuiInputCommand {
    pub fn should_stop_propagation(self) -> bool {
        !matches!(self, Self::Ignore)
    }

    pub fn cditor_command(self) -> Option<crate::api::CditorCommand> {
        use crate::api::{CaretDirection, CditorCommand};

        Some(match self {
            Self::Ignore | Self::ToggleDebugOverlay => return None,
            Self::SelectAllFocusedText => CditorCommand::SelectAll,
            Self::CopySelection => CditorCommand::CopySelection,
            Self::CutSelection => CditorCommand::CutSelection,
            Self::PasteClipboard => CditorCommand::PasteClipboard,
            Self::UndoFocusedBlock => CditorCommand::Undo,
            Self::RedoFocusedBlock => CditorCommand::Redo,
            Self::InsertParagraphAfterFocused => CditorCommand::InsertParagraphAfterFocused,
            Self::InsertSoftLineBreak => CditorCommand::InsertSoftLineBreak,
            Self::HandleEnter => CditorCommand::HandleEnter,
            Self::IndentBlock => CditorCommand::IndentBlock,
            Self::OutdentBlock => CditorCommand::OutdentBlock,
            Self::DeleteBackward => CditorCommand::DeleteBackward,
            Self::DeleteForward => CditorCommand::DeleteForward,
            Self::MoveCaretLeft { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::PreviousVisual,
                extend_selection,
            },
            Self::MoveCaretRight { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::NextVisual,
                extend_selection,
            },
            Self::MoveCaretToPreviousWord { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::PreviousWord,
                extend_selection,
            },
            Self::MoveCaretToNextWord { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::NextWord,
                extend_selection,
            },
            Self::MoveCaretToDocumentStart { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::DocumentStart,
                extend_selection,
            },
            Self::MoveCaretToDocumentEnd { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::DocumentEnd,
                extend_selection,
            },
            Self::MoveCaretUp { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::PreviousLine,
                extend_selection,
            },
            Self::MoveCaretDown { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::NextLine,
                extend_selection,
            },
            Self::MoveCaretToLineStart { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::LineStart,
                extend_selection,
            },
            Self::MoveCaretToLineEnd { extend_selection } => CditorCommand::MoveCaret {
                direction: CaretDirection::LineEnd,
                extend_selection,
            },
            Self::ToggleBold => CditorCommand::ToggleBold,
            Self::ToggleItalic => CditorCommand::ToggleItalic,
            Self::ToggleUnderline => CditorCommand::ToggleUnderline,
            Self::ToggleInlineCode => CditorCommand::ToggleInlineCode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{CaretDirection, CditorCommand, CommandArgs, CommandSource};

    #[test]
    fn gui_input_maps_to_the_stable_command_protocol() {
        let command = GuiInputCommand::MoveCaretLeft {
            extend_selection: true,
        }
        .cditor_command()
        .expect("document command");
        assert_eq!(
            command,
            CditorCommand::MoveCaret {
                direction: CaretDirection::PreviousVisual,
                extend_selection: true,
            }
        );
        let invocation = command.invocation(CommandSource::Keyboard);
        assert_eq!(invocation.id.as_str(), "text.move_caret");
        assert_eq!(
            invocation.args,
            CommandArgs::MoveCaret {
                direction: CaretDirection::PreviousVisual,
                extend_selection: true,
            }
        );
    }

    #[test]
    fn word_navigation_maps_to_stable_caret_directions() {
        assert_eq!(
            GuiInputCommand::MoveCaretToPreviousWord {
                extend_selection: true,
            }
            .cditor_command(),
            Some(CditorCommand::MoveCaret {
                direction: CaretDirection::PreviousWord,
                extend_selection: true,
            })
        );
        assert_eq!(
            GuiInputCommand::MoveCaretToNextWord {
                extend_selection: false,
            }
            .cditor_command(),
            Some(CditorCommand::MoveCaret {
                direction: CaretDirection::NextWord,
                extend_selection: false,
            })
        );
    }

    #[test]
    fn transient_gui_only_actions_do_not_escape_as_document_commands() {
        assert_eq!(GuiInputCommand::Ignore.cditor_command(), None);
        assert_eq!(GuiInputCommand::ToggleDebugOverlay.cditor_command(), None);
    }
}
