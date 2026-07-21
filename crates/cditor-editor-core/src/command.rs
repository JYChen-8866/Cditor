use std::{fmt, str::FromStr};

use cditor_core::{
    edit::TransactionId,
    ids::BlockId,
    rich_text::{
        BlockAttrs, BlockPayload, InlineColorTarget, InlineMark, RichBlockKind, TableRange,
    },
};
use serde::{Deserialize, Serialize};

mod catalog;

pub use catalog::{
    CommandCatalog, CommandCatalogRegistrationError, CommandDefinition, CommandMutability,
};

pub const CURRENT_COMMAND_SCHEMA_VERSION: u16 = 1;

/// Stable, namespaced command identifier used by every command producer.
///
/// IDs are lowercase ASCII segments separated by dots. The format is kept
/// deliberately independent from Rust enum names so SDK and persisted
/// automation payloads do not change when implementation types move.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(String);

impl CommandId {
    pub fn new(value: impl Into<String>) -> Result<Self, CommandIdError> {
        let value = value.into();
        validate_command_id(&value)?;
        Ok(Self(value))
    }

    pub fn builtin(value: &'static str) -> Self {
        debug_assert!(validate_command_id(value).is_ok());
        Self(value.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CommandId {
    type Err = CommandIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandIdError {
    Empty,
    TooLong,
    MissingNamespace,
    EmptySegment,
    InvalidSegmentStart,
    InvalidCharacter,
}

impl fmt::Display for CommandIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "command id is empty",
            Self::TooLong => "command id exceeds 128 bytes",
            Self::MissingNamespace => "command id must contain a namespace separator",
            Self::EmptySegment => "command id contains an empty segment",
            Self::InvalidSegmentStart => "command id segments must start with a lowercase letter",
            Self::InvalidCharacter => {
                "command id contains characters outside lowercase ASCII, digits, '_' and '-'"
            }
        })
    }
}

impl std::error::Error for CommandIdError {}

fn validate_command_id(value: &str) -> Result<(), CommandIdError> {
    if value.is_empty() {
        return Err(CommandIdError::Empty);
    }
    if value.len() > 128 {
        return Err(CommandIdError::TooLong);
    }
    if !value.contains('.') {
        return Err(CommandIdError::MissingNamespace);
    }
    for segment in value.split('.') {
        let mut chars = segment.chars();
        let Some(first) = chars.next() else {
            return Err(CommandIdError::EmptySegment);
        };
        if !first.is_ascii_lowercase() {
            return Err(CommandIdError::InvalidSegmentStart);
        }
        if !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
        {
            return Err(CommandIdError::InvalidCharacter);
        }
    }
    Ok(())
}

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
    MoveCaret {
        direction: CaretDirection,
        extend_selection: bool,
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
            Self::MoveCaret { .. } => CommandArgumentKind::MoveCaret,
            Self::InlineMark(_) => CommandArgumentKind::InlineMark,
            Self::InlineColor { .. } => CommandArgumentKind::InlineColor,
            Self::BlockColor { .. } => CommandArgumentKind::BlockColor,
            Self::CodeLanguage { .. } => CommandArgumentKind::CodeLanguage,
            Self::AiApply { .. } => CommandArgumentKind::AiApply,
            Self::BlockKind(_) => CommandArgumentKind::BlockKind,
            Self::InsertBlock { .. } => CommandArgumentKind::InsertBlock,
            Self::SlashBlock { .. } => CommandArgumentKind::SlashBlock,
            Self::BlockTarget { .. } => CommandArgumentKind::BlockTarget,
            Self::InsertTable { .. } => CommandArgumentKind::InsertTable,
            Self::TableAxisTarget { .. } => CommandArgumentKind::TableAxisTarget,
            Self::TableInsertAxis { .. } => CommandArgumentKind::TableInsertAxis,
            Self::TableRangeTarget { .. } => CommandArgumentKind::TableRangeTarget,
            Self::TableRangeColor { .. } => CommandArgumentKind::TableRangeColor,
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
    MoveCaret,
    InlineMark,
    InlineColor,
    BlockColor,
    CodeLanguage,
    AiApply,
    BlockKind,
    InsertBlock,
    SlashBlock,
    BlockTarget,
    InsertTable,
    TableAxisTarget,
    TableInsertAxis,
    TableRangeTarget,
    TableRangeColor,
    Extension,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandInvocation {
    pub schema_version: u16,
    pub id: CommandId,
    pub args: CommandArgs,
    pub source: CommandSource,
    pub request_id: Option<u64>,
}

impl CommandInvocation {
    pub fn new(id: CommandId, args: CommandArgs, source: CommandSource) -> Self {
        Self {
            schema_version: CURRENT_COMMAND_SCHEMA_VERSION,
            id,
            args,
            source,
            request_id: None,
        }
    }

    pub fn with_request_id(mut self, request_id: u64) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn validate_schema(&self) -> Result<(), CommandError> {
        if self.schema_version == CURRENT_COMMAND_SCHEMA_VERSION {
            Ok(())
        } else {
            Err(CommandError::new(
                self.id.clone(),
                CommandErrorCode::UnsupportedSchema,
                format!(
                    "unsupported command schema {}; expected {}",
                    self.schema_version, CURRENT_COMMAND_SCHEMA_VERSION
                ),
            ))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCheckState {
    NotCheckable,
    Unchecked,
    Checked,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandVisibility {
    Visible,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandUnavailableReason {
    RuntimeNotReady,
    Readonly,
    InvalidSelection,
    MissingTarget,
    UnsupportedTarget,
    CompositionConflict,
    PermissionDenied,
    Busy,
    UnknownCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandQueryState {
    pub enabled: bool,
    pub check: CommandCheckState,
    pub visibility: CommandVisibility,
    pub reason: Option<CommandUnavailableReason>,
    pub detail: Option<String>,
}

impl CommandQueryState {
    pub const ENABLED: Self = Self {
        enabled: true,
        check: CommandCheckState::NotCheckable,
        visibility: CommandVisibility::Visible,
        reason: None,
        detail: None,
    };

    pub fn disabled(reason: CommandUnavailableReason) -> Self {
        Self {
            enabled: false,
            check: CommandCheckState::NotCheckable,
            visibility: CommandVisibility::Visible,
            reason: Some(reason),
            detail: None,
        }
    }

    pub fn with_check(mut self, check: CommandCheckState) -> Self {
        self.check = check;
        self
    }

    pub fn hidden(mut self) -> Self {
        self.visibility = CommandVisibility::Hidden;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CommandOutcomeStatus {
    Applied,
    #[default]
    NoOp,
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandOutcome {
    pub status: CommandOutcomeStatus,
    pub transaction_ids: Vec<TransactionId>,
    pub affected_blocks: Vec<BlockId>,
    pub selection_changed: bool,
    pub request_repaint: bool,
}

impl CommandOutcome {
    pub fn applied(transaction_ids: Vec<TransactionId>, affected_blocks: Vec<BlockId>) -> Self {
        Self {
            status: CommandOutcomeStatus::Applied,
            transaction_ids,
            affected_blocks,
            selection_changed: false,
            request_repaint: true,
        }
    }

    pub const fn no_op() -> Self {
        Self {
            status: CommandOutcomeStatus::NoOp,
            transaction_ids: Vec::new(),
            affected_blocks: Vec::new(),
            selection_changed: false,
            request_repaint: false,
        }
    }

    pub fn changed(&self) -> bool {
        matches!(self.status, CommandOutcomeStatus::Applied)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandErrorCode {
    UnknownCommand,
    UnsupportedSchema,
    InvalidArguments,
    Disabled,
    Readonly,
    PermissionDenied,
    StalePrecondition,
    CompositionConflict,
    ApplyFailed,
    RollbackFailed,
    Cancelled,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandError {
    pub command_id: CommandId,
    pub code: CommandErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl CommandError {
    pub fn new(command_id: CommandId, code: CommandErrorCode, message: impl Into<String>) -> Self {
        Self {
            command_id,
            code,
            message: message.into(),
            retryable: false,
        }
    }

    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.command_id, self.message)
    }
}

impl std::error::Error for CommandError {}

pub mod builtin {
    pub const EDIT_UNDO: &str = "edit.undo";
    pub const EDIT_REDO: &str = "edit.redo";
    pub const EDIT_SELECT_ALL: &str = "edit.select_all";
    pub const EDIT_COPY: &str = "edit.copy";
    pub const EDIT_CUT: &str = "edit.cut";
    pub const EDIT_PASTE: &str = "edit.paste";
    pub const EDIT_DELETE_SELECTION: &str = "edit.delete_selection";
    pub const TEXT_INSERT: &str = "text.insert";
    pub const TEXT_REPLACE: &str = "text.replace";
    pub const TEXT_DELETE_BACKWARD: &str = "text.delete_backward";
    pub const TEXT_DELETE_FORWARD: &str = "text.delete_forward";
    pub const TEXT_MOVE_CARET: &str = "text.move_caret";
    pub const TEXT_INSERT_SOFT_BREAK: &str = "text.insert_soft_break";
    pub const FORMAT_TOGGLE_MARK: &str = "format.toggle_mark";
    pub const FORMAT_TOGGLE_BOLD: &str = "format.toggle_bold";
    pub const FORMAT_TOGGLE_ITALIC: &str = "format.toggle_italic";
    pub const FORMAT_TOGGLE_UNDERLINE: &str = "format.toggle_underline";
    pub const FORMAT_TOGGLE_STRIKE: &str = "format.toggle_strike";
    pub const FORMAT_TOGGLE_INLINE_CODE: &str = "format.toggle_inline_code";
    pub const FORMAT_SET_COLOR: &str = "format.set_color";
    pub const BLOCK_INSERT: &str = "block.insert";
    pub const BLOCK_DELETE_SELECTED: &str = "block.delete_selected";
    pub const BLOCK_DUPLICATE_SELECTED: &str = "block.duplicate_selected";
    pub const BLOCK_INSERT_TABLE: &str = "block.insert_table";
    pub const BLOCK_INSERT_IMAGE: &str = "block.insert_image";
    pub const BLOCK_INSERT_WHITEBOARD: &str = "block.insert_whiteboard";
    pub const BLOCK_INSERT_MERMAID: &str = "block.insert_mermaid";
    pub const BLOCK_INSERT_AFTER: &str = "block.insert_after";
    pub const BLOCK_INSERT_AFTER_FOCUSED: &str = "block.insert_after_focused";
    pub const BLOCK_TRANSFORM: &str = "block.transform";
    pub const BLOCK_SET_COLOR: &str = "block.set_color";
    pub const BLOCK_APPLY_SLASH: &str = "block.apply_slash";
    pub const BLOCK_DELETE: &str = "block.delete";
    pub const BLOCK_DUPLICATE: &str = "block.duplicate";
    pub const BLOCK_COPY_TEXT: &str = "block.copy_text";
    pub const BLOCK_ENTER: &str = "block.enter";
    pub const BLOCK_INDENT: &str = "block.indent";
    pub const BLOCK_OUTDENT: &str = "block.outdent";
    pub const BLOCK_FOLD: &str = "block.fold";
    pub const BLOCK_UNFOLD: &str = "block.unfold";
    pub const HEADING_FOLD: &str = "heading.fold";
    pub const HEADING_UNFOLD: &str = "heading.unfold";
    pub const BLOCK_TOGGLE_TODO: &str = "block.toggle_todo";
    pub const CODE_SET_LANGUAGE: &str = "code.set_language";
    pub const TABLE_INSERT: &str = "table.insert";
    pub const TABLE_INSERT_AXIS: &str = "table.insert_axis";
    pub const TABLE_DELETE_AXIS: &str = "table.delete_axis";
    pub const TABLE_DUPLICATE_AXIS: &str = "table.duplicate_axis";
    pub const TABLE_TOGGLE_HEADER: &str = "table.toggle_header";
    pub const TABLE_CLEAR_RANGE: &str = "table.clear_range";
    pub const TABLE_SET_RANGE_COLOR: &str = "table.set_range_color";
    pub const TABLE_MERGE_CELLS: &str = "table.merge_cells";
    pub const TABLE_SPLIT_CELL: &str = "table.split_cell";
    pub const COLLECTION_INSERT_RECORD: &str = "collection.insert_record";
    pub const COLLECTION_DELETE_RECORD: &str = "collection.delete_record";
    pub const COLLECTION_ADD_PROPERTY: &str = "collection.add_property";
    pub const COLLECTION_UPDATE_PROPERTY: &str = "collection.update_property";
    pub const COLLECTION_DELETE_PROPERTY: &str = "collection.delete_property";
    pub const COLLECTION_UPDATE_VIEW: &str = "collection.update_view";
    pub const COMMENT_CREATE_THREAD: &str = "comment.create_thread";
    pub const COMMENT_RESOLVE_THREAD: &str = "comment.resolve_thread";
    pub const ASSET_INSERT: &str = "asset.insert";
    pub const ASSET_REPLACE: &str = "asset.replace";
    pub const ASSET_UPDATE: &str = "asset.update";
    pub const AI_APPLY: &str = "ai.apply";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ids_require_stable_namespaces() {
        assert_eq!(
            CommandId::new("format.toggle_mark")
                .expect("valid id")
                .as_str(),
            "format.toggle_mark"
        );
        assert_eq!(
            CommandId::new("undo"),
            Err(CommandIdError::MissingNamespace)
        );
        assert_eq!(
            CommandId::new("Edit.undo"),
            Err(CommandIdError::InvalidSegmentStart)
        );
        assert_eq!(
            CommandId::new("edit..undo"),
            Err(CommandIdError::EmptySegment)
        );
        assert_eq!(
            CommandId::new("edit.undo!"),
            Err(CommandIdError::InvalidCharacter)
        );
    }

    #[test]
    fn invocation_schema_roundtrips_without_losing_typed_arguments() {
        let invocation = CommandInvocation::new(
            CommandId::builtin(builtin::FORMAT_TOGGLE_MARK),
            CommandArgs::InlineMark(InlineMark::Bold),
            CommandSource::Toolbar,
        )
        .with_request_id(42);
        let encoded = serde_json::to_value(&invocation).expect("encode command");
        let decoded: CommandInvocation = serde_json::from_value(encoded).expect("decode command");

        assert_eq!(decoded, invocation);
        assert_eq!(decoded.args.kind(), CommandArgumentKind::InlineMark);
        assert_eq!(decoded.validate_schema(), Ok(()));
    }

    #[test]
    fn unknown_command_schema_is_rejected_before_dispatch() {
        let mut invocation = CommandInvocation::new(
            CommandId::builtin(builtin::EDIT_UNDO),
            CommandArgs::None,
            CommandSource::Sdk,
        );
        invocation.schema_version += 1;

        let error = invocation.validate_schema().expect_err("schema must fail");
        assert_eq!(error.code, CommandErrorCode::UnsupportedSchema);
        assert_eq!(error.command_id.as_str(), builtin::EDIT_UNDO);
    }

    #[test]
    fn query_state_distinguishes_checked_mixed_hidden_and_reason() {
        let checked = CommandQueryState::ENABLED.with_check(CommandCheckState::Checked);
        let mixed = CommandQueryState::ENABLED.with_check(CommandCheckState::Mixed);
        let disabled = CommandQueryState::disabled(CommandUnavailableReason::Readonly);
        let hidden = CommandQueryState::ENABLED.hidden();

        assert_eq!(checked.check, CommandCheckState::Checked);
        assert_eq!(mixed.check, CommandCheckState::Mixed);
        assert_eq!(disabled.reason, Some(CommandUnavailableReason::Readonly));
        assert_eq!(hidden.visibility, CommandVisibility::Hidden);
    }

    #[test]
    fn command_outcome_preserves_transactions_and_affected_blocks() {
        let outcome = CommandOutcome::applied(vec![11, 12], vec![7, 8]);
        let encoded = serde_json::to_string(&outcome).expect("encode outcome");
        let decoded: CommandOutcome = serde_json::from_str(&encoded).expect("decode outcome");

        assert!(decoded.changed());
        assert_eq!(decoded.transaction_ids, vec![11, 12]);
        assert_eq!(decoded.affected_blocks, vec![7, 8]);
        assert!(!CommandOutcome::no_op().changed());
    }
}
