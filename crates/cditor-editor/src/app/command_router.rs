use gpui::{ClipboardItem, Context};
use std::sync::OnceLock;

use crate::app::cditor_v2_view::CditorV2View;
use crate::input::GuiInputCommand;
use cditor_api::{CditorError, command::CommandState, event::CditorEvent};
use cditor_core::edit::ChangeOrigin;
use cditor_editor_protocol::command::{
    CaretDirection, CditorCommand, CommandCatalog, CommandEnvelope, CommandOutcome,
    CommandQueryState, CommandSource, CommandUnavailableReason,
};
use cditor_session::project_command_query;
use cditor_session::{project_block_plain_text, project_command_dispatch, project_end_input_batch};

impl CditorV2View {
    pub(in crate::app) fn apply_input_command(
        &mut self,
        command: GuiInputCommand,
        cx: &mut Context<Self>,
    ) {
        if matches!(command, GuiInputCommand::ToggleDebugOverlay) {
            self.show_debug = !self.show_debug;
            return;
        }
        let Some(command) = command.cditor_command() else {
            return;
        };
        let _ = self.dispatch_command(command, CommandSource::Keyboard, cx);
    }

    pub fn sdk_execute_command(
        &mut self,
        command: CditorCommand,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, CditorError> {
        self.dispatch_command(command, CommandSource::Sdk, cx)
    }

    pub fn sdk_command_state(&self, command: &CditorCommand) -> CommandState {
        CommandState::from_query(self.query_command(command))
    }

    pub(in crate::app) fn dispatch_command(
        &mut self,
        command: CditorCommand,
        source: CommandSource,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, CditorError> {
        let invocation = command.invocation(source);
        command_catalog()
            .validate_invocation(&invocation)
            .map_err(|error| CditorError::InvalidInput(error.to_string()))?;
        let state = self.query_command(&command);
        if !state.enabled {
            return Err(error_for_disabled_command(&command, &state));
        }

        if matches!(command, CditorCommand::Undo | CditorCommand::Redo) {
            let redo = matches!(command, CditorCommand::Redo);
            return self
                .execute_history_action(source, redo, cx)
                .map(|changed| CommandOutcome::from_document_change(changed, None));
        }

        if let Some(runtime) = self.ready_runtime() {
            project_end_input_batch(runtime);
        }

        if let CditorCommand::CopyBlockText { block_id } = command {
            let text = self
                .ready_runtime_ref()
                .and_then(|runtime| project_block_plain_text(runtime, block_id))
                .ok_or(CditorError::BlockNotFound(block_id))?;
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            return Ok(CommandOutcome::applied_side_effect(false));
        }

        if runtime_dispatches(&command) {
            let mutates_document = command_mutates_document(&command);
            let dispatched = project_command_dispatch(
                self.ready_runtime().ok_or(CditorError::NotReady)?,
                CommandEnvelope::new(command.clone(), source),
            )
            .map_err(protocol_command_error)?;
            if dispatched.revision != dispatched.before_revision && mutates_document {
                self.mark_dirty_at_revision(
                    change_origin_for_source(source),
                    dispatched.revision,
                    cx,
                );
            }
            let outcome = dispatched.outcome;
            if outcome.selection_changed
                && let Some(selection) = self.sdk_selection()
            {
                cx.emit(CditorEvent::SelectionChanged { selection });
            }
            if outcome.request_repaint {
                cx.notify();
            }
            return Ok(outcome);
        }
        if let Some(gui_command) = gui_handler_for_command(&command) {
            let before_revision = self.ready_runtime_ref().map(|runtime| runtime.revision());
            let before_selection = self
                .ready_runtime_ref()
                .and_then(|runtime| runtime.document_selection_snapshot());
            self.execute_gui_input_command_handler(gui_command, cx);
            let after_revision = self.ready_runtime_ref().map(|runtime| runtime.revision());
            let after_selection = self
                .ready_runtime_ref()
                .and_then(|runtime| runtime.document_selection_snapshot());
            let changed = before_revision != after_revision;
            let selection_changed = before_selection != after_selection;
            let side_effect_only = matches!(command, CditorCommand::CopySelection);
            return Ok(if changed {
                CommandOutcome::from_document_change(true, None)
            } else if selection_changed || side_effect_only {
                CommandOutcome::applied_side_effect(selection_changed)
            } else {
                CommandOutcome::no_op()
            });
        }

        self.execute_sdk_command_handler(command, source, cx)
    }

    pub(in crate::app) fn query_command(&self, command: &CditorCommand) -> CommandQueryState {
        let Some(runtime) = self.ready_runtime_ref() else {
            return CommandQueryState::disabled(CommandUnavailableReason::RuntimeNotReady);
        };
        project_command_query(runtime, command, self.readonly)
    }
}

fn runtime_dispatches(command: &CditorCommand) -> bool {
    matches!(
        command,
        CditorCommand::SelectAll
            | CditorCommand::SetDocumentSelection { .. }
            | CditorCommand::SetBlockSelectionRange { .. }
            | CditorCommand::FocusBlock { .. }
            | CditorCommand::FocusTableCell { .. }
            | CditorCommand::BlurTableCell
            | CditorCommand::SetTableCellSelection { .. }
            | CditorCommand::SetTextSurfaceSelection { .. }
            | CditorCommand::DeleteSelection
            | CditorCommand::ApplyClipboardData { .. }
            | CditorCommand::InsertImageAsset { .. }
            | CditorCommand::ToggleBold
            | CditorCommand::ToggleItalic
            | CditorCommand::ToggleUnderline
            | CditorCommand::ToggleStrike
            | CditorCommand::ToggleInlineCode
            | CditorCommand::SetInlineColor { .. }
            | CditorCommand::SetBlockColor { .. }
            | CditorCommand::InsertParagraphAfterBlock { .. }
            | CditorCommand::MoveBlockBefore { .. }
            | CditorCommand::MoveBlockToParent { .. }
            | CditorCommand::InsertParagraphAfterFocused
            | CditorCommand::EnsureTrailingParagraph
            | CditorCommand::InsertSoftLineBreak
            | CditorCommand::HandleEnter
            | CditorCommand::IndentBlock
            | CditorCommand::OutdentBlock
            | CditorCommand::DeleteBackward
            | CditorCommand::DeleteForward
            | CditorCommand::DeleteBlock { .. }
            | CditorCommand::ToggleTodo { .. }
            | CditorCommand::SetCodeLanguage { .. }
            | CditorCommand::TransformBlock(_)
            | CditorCommand::DeleteSelectedBlocks
            | CditorCommand::ApplySlashBlock { .. }
            | CditorCommand::ApplyAiPreview { .. }
            | CditorCommand::FoldHeading
            | CditorCommand::UnfoldHeading
            | CditorCommand::TableToggleHeader { .. }
            | CditorCommand::TableInsertAxis { .. }
            | CditorCommand::TableDeleteAxis { .. }
            | CditorCommand::TableDuplicateAxis { .. }
            | CditorCommand::TableClearRange { .. }
            | CditorCommand::TableSetRangeBackground { .. }
            | CditorCommand::TableMergeCells { .. }
            | CditorCommand::TableSplitCell { .. }
            | CditorCommand::TableSetRangeAlign { .. }
            | CditorCommand::SetMediaWidthRatio { .. }
            | CditorCommand::UpdateWhiteboardScene { .. }
            | CditorCommand::TableResizeAxis { .. }
            | CditorCommand::TableMoveAxis { .. }
    )
}

fn protocol_command_error(error: cditor_editor_protocol::ProtocolError) -> CditorError {
    use cditor_editor_protocol::ProtocolErrorCode;
    match error.code {
        ProtocolErrorCode::InvalidArguments | ProtocolErrorCode::UnsupportedVersion => {
            CditorError::InvalidInput(error.message)
        }
        ProtocolErrorCode::Readonly => CditorError::Readonly,
        ProtocolErrorCode::Cancelled => CditorError::Cancelled,
        ProtocolErrorCode::Timeout => CditorError::Timeout,
        _ => CditorError::Internal(error.message),
    }
}

fn command_catalog() -> &'static CommandCatalog {
    static CATALOG: OnceLock<CommandCatalog> = OnceLock::new();
    CATALOG.get_or_init(CommandCatalog::builtin)
}

pub(in crate::app) fn change_origin_for_source(source: CommandSource) -> ChangeOrigin {
    match source {
        CommandSource::Keyboard
        | CommandSource::Toolbar
        | CommandSource::SlashMenu
        | CommandSource::ContextMenu => ChangeOrigin::User,
        CommandSource::Ime => ChangeOrigin::Ime,
        CommandSource::Sdk | CommandSource::Automation => ChangeOrigin::Host,
        CommandSource::Plugin => ChangeOrigin::Plugin,
        CommandSource::Ai => ChangeOrigin::Ai,
        CommandSource::Import => ChangeOrigin::Import,
    }
}

fn gui_handler_for_command(command: &CditorCommand) -> Option<GuiInputCommand> {
    Some(match command {
        CditorCommand::Undo => GuiInputCommand::UndoFocusedBlock,
        CditorCommand::Redo => GuiInputCommand::RedoFocusedBlock,
        CditorCommand::SelectAll => GuiInputCommand::SelectAllFocusedText,
        CditorCommand::CopySelection => GuiInputCommand::CopySelection,
        CditorCommand::CutSelection => GuiInputCommand::CutSelection,
        CditorCommand::PasteClipboard => GuiInputCommand::PasteClipboard,
        CditorCommand::DeleteSelection => GuiInputCommand::DeleteBackward,
        CditorCommand::ToggleBold => GuiInputCommand::ToggleBold,
        CditorCommand::ToggleItalic => GuiInputCommand::ToggleItalic,
        CditorCommand::ToggleUnderline => GuiInputCommand::ToggleUnderline,
        CditorCommand::ToggleInlineCode => GuiInputCommand::ToggleInlineCode,
        CditorCommand::InsertParagraphAfterFocused => GuiInputCommand::InsertParagraphAfterFocused,
        CditorCommand::InsertSoftLineBreak => GuiInputCommand::InsertSoftLineBreak,
        CditorCommand::HandleEnter => GuiInputCommand::HandleEnter,
        CditorCommand::IndentBlock => GuiInputCommand::IndentBlock,
        CditorCommand::OutdentBlock => GuiInputCommand::OutdentBlock,
        CditorCommand::DeleteBackward => GuiInputCommand::DeleteBackward,
        CditorCommand::DeleteForward => GuiInputCommand::DeleteForward,
        CditorCommand::MoveCaret {
            direction,
            extend_selection,
        } => match direction {
            CaretDirection::PreviousVisual => GuiInputCommand::MoveCaretLeft {
                extend_selection: *extend_selection,
            },
            CaretDirection::NextVisual => GuiInputCommand::MoveCaretRight {
                extend_selection: *extend_selection,
            },
            CaretDirection::PreviousWord => GuiInputCommand::MoveCaretToPreviousWord {
                extend_selection: *extend_selection,
            },
            CaretDirection::NextWord => GuiInputCommand::MoveCaretToNextWord {
                extend_selection: *extend_selection,
            },
            CaretDirection::PreviousLine => GuiInputCommand::MoveCaretUp {
                extend_selection: *extend_selection,
            },
            CaretDirection::NextLine => GuiInputCommand::MoveCaretDown {
                extend_selection: *extend_selection,
            },
            CaretDirection::LineStart => GuiInputCommand::MoveCaretToLineStart {
                extend_selection: *extend_selection,
            },
            CaretDirection::LineEnd => GuiInputCommand::MoveCaretToLineEnd {
                extend_selection: *extend_selection,
            },
            CaretDirection::DocumentStart => GuiInputCommand::MoveCaretToDocumentStart {
                extend_selection: *extend_selection,
            },
            CaretDirection::DocumentEnd => GuiInputCommand::MoveCaretToDocumentEnd {
                extend_selection: *extend_selection,
            },
        },
        _ => return None,
    })
}

fn command_mutates_document(command: &CditorCommand) -> bool {
    !matches!(
        command,
        CditorCommand::SelectAll
            | CditorCommand::SetDocumentSelection { .. }
            | CditorCommand::SetBlockSelectionRange { .. }
            | CditorCommand::FocusBlock { .. }
            | CditorCommand::FocusTableCell { .. }
            | CditorCommand::BlurTableCell
            | CditorCommand::SetTableCellSelection { .. }
            | CditorCommand::SetTextSurfaceSelection { .. }
            | CditorCommand::CopySelection
            | CditorCommand::CopyBlockText { .. }
            | CditorCommand::MoveCaret { .. }
    )
}

fn error_for_disabled_command(command: &CditorCommand, state: &CommandQueryState) -> CditorError {
    match state.reason {
        Some(CommandUnavailableReason::RuntimeNotReady) => CditorError::NotReady,
        Some(CommandUnavailableReason::Readonly) => CditorError::Readonly,
        Some(CommandUnavailableReason::InvalidSelection)
        | Some(CommandUnavailableReason::MissingTarget) => CditorError::InvalidSelection,
        reason => CditorError::Unsupported(format!(
            "command {} is disabled: {reason:?}",
            command.stable_id()
        )),
    }
}

#[cfg(test)]
#[path = "command_router_tests.rs"]
mod tests;
