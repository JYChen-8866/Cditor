use gpui::{ClipboardItem, Context};
use std::sync::OnceLock;

use crate::api::{
    CaretDirection, CditorCommand, CditorError, ChangeOrigin, CommandCatalog, CommandCheckState,
    CommandOutcome, CommandOutcomeStatus, CommandQueryState, CommandSource, CommandState,
    CommandUnavailableReason, TableAxis,
};
use crate::gui::app::cditor_v2_view::CditorV2View;
use crate::gui::input::GuiInputCommand;

impl CditorV2View {
    pub(in crate::gui::app) fn apply_input_command(
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

    pub(crate) fn sdk_execute_command(
        &mut self,
        command: CditorCommand,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, CditorError> {
        self.dispatch_command(command, CommandSource::Sdk, cx)
    }

    pub(crate) fn sdk_command_state(&self, command: &CditorCommand) -> CommandState {
        CommandState::from_query(self.query_command(command))
    }

    pub(in crate::gui::app) fn dispatch_command(
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
            return Ok(CommandOutcome {
                changed: false,
                transaction_id: None,
                status: CommandOutcomeStatus::Applied,
            });
        }

        let table_before_revision = self.ready_runtime_ref().map(|runtime| runtime.revision());
        let table_before_transaction_id = self
            .ready_runtime_ref()
            .and_then(|runtime| runtime.last_committed_transaction_id());
        if let Some(result) = self.execute_table_document_command(&command) {
            let changed = result.map_err(CditorError::Internal)?;
            let after_revision = self.ready_runtime_ref().map(|runtime| runtime.revision());
            let transaction_id = self
                .ready_runtime_ref()
                .and_then(|runtime| runtime.last_committed_transaction_id())
                .filter(|transaction_id| Some(*transaction_id) != table_before_transaction_id);
            if changed {
                if let Some(revision) =
                    after_revision.filter(|after| Some(*after) != table_before_revision)
                {
                    self.mark_dirty_at_revision(change_origin_for_source(source), revision, cx);
                } else {
                    self.mark_dirty_with_origin(change_origin_for_source(source), cx);
                }
                cx.notify();
            }
            return Ok(CommandOutcome {
                changed,
                transaction_id,
                status: if changed {
                    CommandOutcomeStatus::Applied
                } else {
                    CommandOutcomeStatus::NoOp
                },
            });
        }
        let format_before_revision = self.ready_runtime_ref().map(|runtime| runtime.revision());
        let format_before_transaction_id = self
            .ready_runtime_ref()
            .and_then(|runtime| runtime.last_committed_transaction_id());
        if let Some(result) = self.execute_format_document_command(&command) {
            let changed = result.map_err(CditorError::Internal)?;
            let after_revision = self.ready_runtime_ref().map(|runtime| runtime.revision());
            let transaction_id = self
                .ready_runtime_ref()
                .and_then(|runtime| runtime.last_committed_transaction_id())
                .filter(|transaction_id| Some(*transaction_id) != format_before_transaction_id);
            if changed {
                if let Some(revision) =
                    after_revision.filter(|after| Some(*after) != format_before_revision)
                {
                    self.mark_dirty_at_revision(change_origin_for_source(source), revision, cx);
                } else {
                    self.mark_dirty_with_origin(change_origin_for_source(source), cx);
                }
                cx.notify();
            }
            return Ok(CommandOutcome {
                changed,
                transaction_id,
                status: if changed {
                    CommandOutcomeStatus::Applied
                } else {
                    CommandOutcomeStatus::NoOp
                },
            });
        }
        if let Some(result) = self.execute_block_document_command(&command) {
            let changed = result.map_err(CditorError::Internal)?;
            if changed {
                self.mark_dirty_with_origin(change_origin_for_source(source), cx);
                cx.notify();
            }
            return Ok(CommandOutcome {
                changed,
                transaction_id: None,
                status: if changed {
                    CommandOutcomeStatus::Applied
                } else {
                    CommandOutcomeStatus::NoOp
                },
            });
        }
        if let CditorCommand::ApplyAiPreview { mode } = command {
            let mode = match mode {
                crate::api::AiApplyCommandMode::Replace => cditor_runtime::AiApplyMode::Replace,
                crate::api::AiApplyCommandMode::InsertAfter => {
                    cditor_runtime::AiApplyMode::InsertAfter
                }
            };
            let changed = self
                .ready_runtime()
                .ok_or(CditorError::NotReady)?
                .apply_ai_preview(mode)
                .map_err(CditorError::Ai)?;
            if changed {
                self.mark_dirty_with_origin(ChangeOrigin::Ai, cx);
                cx.notify();
            }
            return Ok(CommandOutcome {
                changed,
                transaction_id: None,
                status: if changed {
                    CommandOutcomeStatus::Applied
                } else {
                    CommandOutcomeStatus::NoOp
                },
            });
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
            return Ok(CommandOutcome {
                changed,
                transaction_id: None,
                status: if changed || selection_changed || side_effect_only {
                    CommandOutcomeStatus::Applied
                } else {
                    CommandOutcomeStatus::NoOp
                },
            });
        }

        self.execute_sdk_command_handler(command, source, cx)
    }

    pub(in crate::gui::app) fn query_command(&self, command: &CditorCommand) -> CommandQueryState {
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
            CditorCommand::ApplyAiPreview { .. } => {
                runtime.ai_session_snapshot().is_some_and(|session| {
                    matches!(session.status, cditor_runtime::AiSessionStatus::Ready)
                        && !session.preview.is_empty()
                })
            }
            CditorCommand::TransformBlock(crate::api::BlockTransform::Kind(kind)) => runtime
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
            CditorCommand::InsertBlock(_)
            | CditorCommand::DuplicateSelectedBlocks
            | CditorCommand::InsertTable { .. }
            | CditorCommand::InsertImage
            | CditorCommand::InsertWhiteboard
            | CditorCommand::InsertMermaid => false,
        };

        if enabled {
            CommandQueryState::ENABLED.with_check(command_check_state(command, runtime))
        } else {
            CommandQueryState::disabled(CommandUnavailableReason::InvalidSelection)
        }
    }

    fn execute_table_document_command(
        &mut self,
        command: &CditorCommand,
    ) -> Option<Result<bool, String>> {
        let runtime = self.ready_runtime()?;
        Some(match command {
            CditorCommand::TableToggleHeader { block_id, axis } => {
                let Some(record) = runtime.block_payload_record(*block_id) else {
                    return Some(Ok(false));
                };
                let cditor_core::rich_text::BlockPayload::Table(table) = &record.payload else {
                    return Some(Ok(false));
                };
                let enabled = match axis {
                    TableAxis::Row => table.header_rows > 0,
                    TableAxis::Column => table.header_cols > 0,
                };
                let count = usize::from(!enabled);
                match axis {
                    TableAxis::Row => runtime.set_table_header_rows(*block_id, count),
                    TableAxis::Column => runtime.set_table_header_columns(*block_id, count),
                }
            }
            CditorCommand::TableInsertAxis {
                block_id,
                axis,
                index,
            } => match axis {
                TableAxis::Row => runtime.insert_table_row(*block_id, *index),
                TableAxis::Column => runtime.insert_table_column(*block_id, *index),
            },
            CditorCommand::TableDeleteAxis {
                block_id,
                axis,
                index,
            } => match axis {
                TableAxis::Row => runtime.delete_table_row(*block_id, *index),
                TableAxis::Column => runtime.delete_table_column(*block_id, *index),
            },
            CditorCommand::TableDuplicateAxis {
                block_id,
                axis,
                index,
            } => match axis {
                TableAxis::Row => runtime.duplicate_table_row(*block_id, *index),
                TableAxis::Column => runtime.duplicate_table_column(*block_id, *index),
            },
            CditorCommand::TableClearRange { block_id, range } => {
                runtime.clear_table_range(*block_id, *range)
            }
            CditorCommand::TableSetRangeBackground {
                block_id,
                range,
                color,
            } => runtime.set_table_cell_background_color(*block_id, *range, color.clone()),
            _ => return None,
        })
    }

    fn execute_format_document_command(
        &mut self,
        command: &CditorCommand,
    ) -> Option<Result<bool, String>> {
        let runtime = self.ready_runtime()?;
        Some(match command {
            CditorCommand::ToggleBold => {
                runtime.toggle_inline_mark_on_selection(cditor_core::rich_text::InlineMark::Bold)
            }
            CditorCommand::ToggleItalic => {
                runtime.toggle_inline_mark_on_selection(cditor_core::rich_text::InlineMark::Italic)
            }
            CditorCommand::ToggleUnderline => runtime
                .toggle_inline_mark_on_selection(cditor_core::rich_text::InlineMark::Underline),
            CditorCommand::ToggleStrike => {
                runtime.toggle_inline_mark_on_selection(cditor_core::rich_text::InlineMark::Strike)
            }
            CditorCommand::ToggleInlineCode => {
                runtime.toggle_inline_mark_on_selection(cditor_core::rich_text::InlineMark::Code)
            }
            CditorCommand::SetInlineColor { target, color } => {
                runtime.set_inline_color_on_selection(*target, color.as_deref())
            }
            CditorCommand::SetBlockColor {
                block_id,
                target,
                color,
            } => runtime.set_block_color(*block_id, *target, color.as_deref()),
            _ => return None,
        })
    }

    fn execute_block_document_command(
        &mut self,
        command: &CditorCommand,
    ) -> Option<Result<bool, String>> {
        let runtime = self.ready_runtime()?;
        Some(match command {
            CditorCommand::InsertParagraphAfterBlock { block_id } => runtime
                .insert_paragraph_after_block(*block_id)
                .map(|_| true),
            CditorCommand::DeleteBlock { block_id } => runtime.delete_block_by_id(*block_id),
            CditorCommand::ToggleTodo { block_id } => runtime.toggle_todo_checked(*block_id),
            CditorCommand::SetCodeLanguage { block_id, language } => {
                runtime.set_code_block_language(*block_id, language.clone())
            }
            _ => return None,
        })
    }
}

fn command_catalog() -> &'static CommandCatalog {
    static CATALOG: OnceLock<CommandCatalog> = OnceLock::new();
    CATALOG.get_or_init(CommandCatalog::builtin)
}

pub(in crate::gui::app) fn change_origin_for_source(source: CommandSource) -> ChangeOrigin {
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
