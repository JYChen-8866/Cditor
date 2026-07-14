use cditor_core::{edit::TransactionId, rich_text::RichBlockKind};

use super::document::BlockInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockTransform {
    Kind(RichBlockKind),
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CditorCommand {
    Undo,
    Redo,
    SelectAll,
    DeleteSelection,
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrike,
    ToggleInlineCode,
    InsertBlock(BlockInput),
    TransformBlock(BlockTransform),
    DeleteSelectedBlocks,
    DuplicateSelectedBlocks,
    InsertTable { rows: usize, columns: usize },
    InsertImage,
    InsertWhiteboard,
    InsertMermaid,
    FoldHeading,
    UnfoldHeading,
}

impl CditorCommand {
    pub const fn stable_id(&self) -> &'static str {
        match self {
            Self::Undo => "edit.undo",
            Self::Redo => "edit.redo",
            Self::SelectAll => "edit.select_all",
            Self::DeleteSelection => "edit.delete_selection",
            Self::ToggleBold => "format.toggle_bold",
            Self::ToggleItalic => "format.toggle_italic",
            Self::ToggleUnderline => "format.toggle_underline",
            Self::ToggleStrike => "format.toggle_strike",
            Self::ToggleInlineCode => "format.toggle_inline_code",
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandState {
    pub enabled: bool,
    pub active: bool,
    pub visible: bool,
}

impl CommandState {
    pub const DISABLED: Self = Self {
        enabled: false,
        active: false,
        visible: true,
    };
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommandOutcome {
    pub changed: bool,
    pub transaction_id: Option<TransactionId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashItem {
    pub command_id: String,
    pub title: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarItem {
    pub command_id: String,
    pub label: String,
}
