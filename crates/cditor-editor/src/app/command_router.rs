use gpui::{ClipboardItem, Context};
use std::sync::OnceLock;

use crate::app::cditor_v2_view::CditorV2View;
use crate::input::GuiInputCommand;
use cditor_api::{CditorError, command::CommandState, event::CditorEvent};
use cditor_core::edit::ChangeOrigin;
use cditor_editor_protocol::command::{
    CaretDirection, CditorCommand, CommandCatalog, CommandCheckState, CommandEnvelope,
    CommandOutcome, CommandQueryState, CommandSource, CommandUnavailableReason, TableAxis,
};

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
            runtime.break_typing_coalescing();
        }

        if let CditorCommand::CopyBlockText { block_id } = command {
            let text = self
                .ready_runtime_ref()
                .and_then(|runtime| runtime.block_payload_record(block_id))
                .map(|payload| payload.plain_text())
                .ok_or(CditorError::BlockNotFound(block_id))?;
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            return Ok(CommandOutcome::applied_side_effect(false));
        }

        if runtime_dispatches(&command) {
            let mutates_document = command_mutates_document(&command);
            let before_revision = self
                .ready_runtime_ref()
                .map(cditor_runtime::DocumentRuntime::revision)
                .ok_or(CditorError::NotReady)?;
            let outcome = self
                .ready_runtime()
                .ok_or(CditorError::NotReady)?
                .dispatch(CommandEnvelope::new(command.clone(), source))
                .map_err(protocol_command_error)?;
            let revision = self
                .ready_runtime_ref()
                .map(cditor_runtime::DocumentRuntime::revision)
                .ok_or(CditorError::NotReady)?;
            if revision != before_revision && mutates_document {
                self.mark_dirty_at_revision(change_origin_for_source(source), revision, cx);
            }
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
        if self.readonly && command_mutates_document(command) {
            return CommandQueryState::disabled(CommandUnavailableReason::Readonly);
        }

        let enabled = match command {
            CditorCommand::Undo => runtime.can_undo(),
            CditorCommand::Redo => runtime.can_redo(),
            CditorCommand::SelectAll => true,
            CditorCommand::CopySelection
            | CditorCommand::CutSelection
            | CditorCommand::DeleteSelection => runtime.has_active_selection(),
            CditorCommand::PasteClipboard => runtime.focused_block_id().is_some(),
            CditorCommand::ApplyClipboardData { .. } | CditorCommand::InsertImageAsset { .. } => {
                runtime.focused_block_id().is_some()
            }
            CditorCommand::ToggleBold
            | CditorCommand::ToggleItalic
            | CditorCommand::ToggleUnderline
            | CditorCommand::ToggleStrike
            | CditorCommand::ToggleInlineCode => runtime.selected_focused_rich_text().is_some(),
            CditorCommand::SetInlineColor { .. } => {
                runtime.focused_text_selection_range().is_some()
            }
            CditorCommand::SetBlockColor { block_id, .. } => {
                runtime.block_payload_record(*block_id).is_some()
            }
            CditorCommand::InsertParagraphAfterBlock { block_id } => {
                runtime.block_payload_record(*block_id).is_some()
            }
            CditorCommand::DeleteBlock { block_id } => runtime.can_delete_block(*block_id),
            CditorCommand::ToggleTodo { block_id } => matches!(
                runtime.block_kind(*block_id),
                Some(cditor_core::rich_text::RichBlockKind::Todo { .. })
            ),
            CditorCommand::SetCodeLanguage { block_id, .. } => matches!(
                runtime.block_kind(*block_id),
                Some(cditor_core::rich_text::RichBlockKind::Code { .. })
            ),
            CditorCommand::CopyBlockText { block_id } => {
                runtime.block_payload_record(*block_id).is_some()
            }
            CditorCommand::MoveBlockBefore {
                block_id,
                before_block_id,
            } => {
                runtime.block_kind(*block_id).is_some()
                    && before_block_id.is_none_or(|target| runtime.block_kind(target).is_some())
            }
            CditorCommand::MoveBlockToParent {
                block_id,
                parent_id,
                ..
            } => {
                runtime.block_kind(*block_id).is_some()
                    && parent_id.is_none_or(|parent| runtime.block_kind(parent).is_some())
            }
            CditorCommand::ApplyAiPreview { .. } => {
                runtime.ai_session_snapshot().is_some_and(|session| {
                    matches!(session.status, cditor_runtime::AiSessionStatus::Ready)
                        && !session.preview.is_empty()
                })
            }
            CditorCommand::TransformBlock(
                cditor_editor_protocol::command::BlockTransform::Kind(kind),
            ) => runtime
                .focused_block_id()
                .is_some_and(|block_id| runtime.can_convert_block_kind(block_id, kind)),
            CditorCommand::DeleteSelectedBlocks => runtime.has_selected_blocks(),
            CditorCommand::FoldHeading | CditorCommand::UnfoldHeading => {
                runtime.focused_block_id().is_some_and(|block_id| {
                    matches!(
                        runtime.block_kind(block_id),
                        Some(cditor_core::rich_text::RichBlockKind::Heading { .. })
                    )
                })
            }
            CditorCommand::InsertParagraphAfterFocused
            | CditorCommand::DeleteBackward
            | CditorCommand::DeleteForward
            | CditorCommand::MoveCaret { .. } => runtime.focused_block_id().is_some(),
            CditorCommand::EnsureTrailingParagraph => runtime.document_block_count() > 0,
            CditorCommand::InsertSoftLineBreak => runtime.can_insert_soft_line_break(),
            CditorCommand::HandleEnter => runtime.can_handle_enter(),
            CditorCommand::IndentBlock => runtime.can_indent_focused_block(),
            CditorCommand::OutdentBlock => runtime.can_outdent_focused_block(),
            CditorCommand::ApplySlashBlock {
                block_id,
                trigger_range,
                kind,
            } => runtime.can_apply_slash_block_kind(*block_id, trigger_range.clone(), kind),
            CditorCommand::TableToggleHeader { block_id, .. } => {
                table_dimensions(runtime, *block_id).is_some()
            }
            CditorCommand::TableInsertAxis {
                block_id,
                axis,
                index,
            } => table_axis_insert_is_valid(runtime, *block_id, *axis, *index),
            CditorCommand::TableDeleteAxis {
                block_id,
                axis,
                index,
            }
            | CditorCommand::TableDuplicateAxis {
                block_id,
                axis,
                index,
            } => table_axis_index_is_valid(runtime, *block_id, *axis, *index),
            CditorCommand::TableClearRange { block_id, range }
            | CditorCommand::TableSetRangeBackground {
                block_id, range, ..
            } => table_range_is_valid(runtime, *block_id, *range),
            CditorCommand::SetMediaWidthRatio { block_id, .. } => matches!(
                runtime.block_kind(*block_id),
                Some(cditor_core::rich_text::RichBlockKind::Image)
            ),
            CditorCommand::TableResizeAxis {
                block_id,
                axis,
                index,
                ..
            } => table_axis_index_is_valid(runtime, *block_id, *axis, *index),
            CditorCommand::TableMoveAxis {
                block_id,
                axis,
                from_index,
                to_index,
            } => {
                table_axis_index_is_valid(runtime, *block_id, *axis, *from_index)
                    && table_axis_index_is_valid(runtime, *block_id, *axis, *to_index)
            }
            CditorCommand::InsertBlock(_)
            | CditorCommand::DuplicateSelectedBlocks
            | CditorCommand::InsertTable { .. }
            | CditorCommand::InsertImage
            | CditorCommand::InsertWhiteboard
            | CditorCommand::InsertMermaid => false,
            _ => false,
        };

        if enabled {
            CommandQueryState::ENABLED.with_check(command_check_state(command, runtime))
        } else {
            CommandQueryState::disabled(CommandUnavailableReason::InvalidSelection)
        }
    }
}

fn runtime_dispatches(command: &CditorCommand) -> bool {
    matches!(
        command,
        CditorCommand::SelectAll
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
            | CditorCommand::SetMediaWidthRatio { .. }
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

fn table_dimensions(
    runtime: &cditor_runtime::DocumentRuntime,
    block_id: cditor_core::ids::BlockId,
) -> Option<(usize, usize)> {
    let record = runtime.block_payload_record(block_id)?;
    let cditor_core::rich_text::BlockPayload::Table(table) = &record.payload else {
        return None;
    };
    Some((table.rows.len(), table.columns.len()))
}

fn table_axis_index_is_valid(
    runtime: &cditor_runtime::DocumentRuntime,
    block_id: cditor_core::ids::BlockId,
    axis: TableAxis,
    index: usize,
) -> bool {
    let Some((rows, columns)) = table_dimensions(runtime, block_id) else {
        return false;
    };
    index
        < match axis {
            TableAxis::Row => rows,
            TableAxis::Column => columns,
        }
}

fn table_axis_insert_is_valid(
    runtime: &cditor_runtime::DocumentRuntime,
    block_id: cditor_core::ids::BlockId,
    axis: TableAxis,
    index: usize,
) -> bool {
    let Some((rows, columns)) = table_dimensions(runtime, block_id) else {
        return false;
    };
    index
        <= match axis {
            TableAxis::Row => rows,
            TableAxis::Column => columns,
        }
}

fn table_range_is_valid(
    runtime: &cditor_runtime::DocumentRuntime,
    block_id: cditor_core::ids::BlockId,
    range: cditor_core::rich_text::TableRange,
) -> bool {
    let Some((rows, columns)) = table_dimensions(runtime, block_id) else {
        return false;
    };
    range.start_row <= range.end_row
        && range.start_col <= range.end_col
        && range.end_row < rows
        && range.end_col < columns
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
            | CditorCommand::CopySelection
            | CditorCommand::CopyBlockText { .. }
            | CditorCommand::MoveCaret { .. }
    )
}

fn command_check_state(
    command: &CditorCommand,
    runtime: &cditor_runtime::DocumentRuntime,
) -> CommandCheckState {
    match command {
        CditorCommand::FoldHeading => {
            if runtime
                .focused_block_id()
                .is_some_and(|block_id| runtime.is_block_folded(block_id))
            {
                CommandCheckState::Checked
            } else {
                CommandCheckState::Unchecked
            }
        }
        CditorCommand::UnfoldHeading => {
            if runtime
                .focused_block_id()
                .is_some_and(|block_id| !runtime.is_block_folded(block_id))
            {
                CommandCheckState::Checked
            } else {
                CommandCheckState::Unchecked
            }
        }
        CditorCommand::ToggleBold
        | CditorCommand::ToggleItalic
        | CditorCommand::ToggleUnderline
        | CditorCommand::ToggleStrike
        | CditorCommand::ToggleInlineCode => {
            let mark = match command {
                CditorCommand::ToggleBold => cditor_core::rich_text::InlineMark::Bold,
                CditorCommand::ToggleItalic => cditor_core::rich_text::InlineMark::Italic,
                CditorCommand::ToggleUnderline => cditor_core::rich_text::InlineMark::Underline,
                CditorCommand::ToggleStrike => cditor_core::rich_text::InlineMark::Strike,
                CditorCommand::ToggleInlineCode => cditor_core::rich_text::InlineMark::Code,
                _ => unreachable!("matched inline mark command"),
            };
            inline_mark_check_state(runtime, &mark)
        }
        _ => CommandCheckState::NotCheckable,
    }
}

fn inline_mark_check_state(
    runtime: &cditor_runtime::DocumentRuntime,
    mark: &cditor_core::rich_text::InlineMark,
) -> CommandCheckState {
    let Some(selection) = runtime.selected_focused_rich_text() else {
        return CommandCheckState::Unchecked;
    };
    let mut saw_text = false;
    let mut any = false;
    let mut all = true;
    for span in selection.spans.iter().filter(|span| !span.text.is_empty()) {
        saw_text = true;
        let contains = span.marks.contains(mark);
        any |= contains;
        all &= contains;
    }
    match (saw_text, any, all) {
        (true, true, true) => CommandCheckState::Checked,
        (true, true, false) => CommandCheckState::Mixed,
        _ => CommandCheckState::Unchecked,
    }
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
