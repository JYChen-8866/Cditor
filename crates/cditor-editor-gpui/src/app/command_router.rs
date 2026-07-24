use gpui::{ClipboardItem, Context};
use std::sync::OnceLock;

use crate::editor_view::CditorV2View;
use crate::input::GuiInputCommand;
use cditor_api::{CditorError, command::CommandState, event::CditorEvent};
use cditor_core::edit::ChangeOrigin;
use cditor_editor_protocol::command::{
    CaretDirection, CditorCommand, CommandCatalog, CommandEnvelope, CommandOutcome,
    CommandQueryState, CommandSource, CommandUnavailableReason,
};

impl CditorV2View {
    pub(crate) fn apply_input_command(&mut self, command: GuiInputCommand, cx: &mut Context<Self>) {
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

    pub(crate) fn dispatch_command(
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

        if let Some(session) = self.ready_session() {
            let _ = session.end_input_batch();
        }

        if let CditorCommand::CopyBlockText { block_id } = command {
            let text = self
                .ready_session()
                .and_then(|session| session.block_plain_text(block_id).ok().flatten())
                .ok_or(CditorError::BlockNotFound(block_id))?;
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            return Ok(CommandOutcome::applied_side_effect(false));
        }

        if runtime_dispatches(&command) {
            let mutates_document = command_mutates_document(&command);
            let dispatched = self
                .ready_session()
                .ok_or(CditorError::NotReady)?
                .dispatch_with_snapshot(CommandEnvelope::new(command.clone(), source))
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
            let before = self
                .ready_session()
                .and_then(|session| session.document_snapshot().ok());
            let before_revision = before.as_ref().map(|snapshot| snapshot.revision);
            let before_selection = self
                .ready_session()
                .and_then(|session| session.document_snapshot().ok())
                .and_then(|snapshot| snapshot.selection);
            self.execute_gui_input_command_handler(gui_command, cx);
            let after = self
                .ready_session()
                .and_then(|session| session.document_snapshot().ok());
            let after_revision = after.as_ref().map(|snapshot| snapshot.revision);
            let after_selection = self
                .ready_session()
                .and_then(|session| session.document_snapshot().ok())
                .and_then(|snapshot| snapshot.selection);
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
        let Some(session) = self.ready_session() else {
            return CommandQueryState::disabled(CommandUnavailableReason::RuntimeNotReady);
        };
        session.query_editor_command(command).unwrap_or_else(|_| {
            CommandQueryState::disabled(CommandUnavailableReason::RuntimeNotReady)
        })
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
