pub use cditor_editor_protocol::command::{
    AiApplyCommandMode, BlockInput, BlockTransform, CURRENT_COMMAND_SCHEMA_VERSION, CaretDirection,
    CommandArgs, CommandCatalog, CommandCatalogRegistrationError, CommandCheckState,
    CommandDefinition, CommandError, CommandErrorCode, CommandId, CommandInvocation,
    CommandMutability, CommandOutcome, CommandOutcomeStatus, CommandQueryState, CommandSource,
    CommandUnavailableReason, CommandVisibility, EditorCommand as CditorCommand, TableAxis,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandState {
    pub enabled: bool,
    pub active: bool,
    pub mixed: bool,
    pub visible: bool,
    pub reason: Option<CommandUnavailableReason>,
}

impl CommandState {
    pub const DISABLED: Self = Self {
        enabled: false,
        active: false,
        mixed: false,
        visible: true,
        reason: Some(CommandUnavailableReason::RuntimeNotReady),
    };

    pub fn from_query(query: CommandQueryState) -> Self {
        Self {
            enabled: query.enabled,
            active: query.check == CommandCheckState::Checked,
            mixed: query.check == CommandCheckState::Mixed,
            visible: query.visibility == CommandVisibility::Visible,
            reason: query.reason,
        }
    }
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
