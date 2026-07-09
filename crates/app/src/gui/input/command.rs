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
    InsertSpaceOrMarkdownShortcut,
    DeleteBackward,
    DeleteForward,
    MoveCaretLeft { extend_selection: bool },
    MoveCaretRight { extend_selection: bool },
    MoveCaretUp { extend_selection: bool },
    MoveCaretDown { extend_selection: bool },
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleInlineCode,
    InsertChar(char),
}

impl GuiInputCommand {
    pub fn should_stop_propagation(self) -> bool {
        !matches!(self, Self::Ignore)
    }
}
