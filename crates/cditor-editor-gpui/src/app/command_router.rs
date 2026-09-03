use crate::clipboard_assets::image_asset_from_clipboard_item;
use gpui::{AppContext, ClipboardItem, Context};
use std::sync::OnceLock;
use std::time::Duration;

use crate::editor_view::CditorV2View;
use crate::input::GuiInputCommand;
use cditor_core::clipboard::ClipboardSelection;
use cditor_core::edit::ChangeOrigin;
use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{
    CaretDirection, CditorCommand, CommandCatalog, CommandEnvelope, CommandOutcome,
    CommandQueryState, CommandSource, CommandUnavailableReason,
};
use cditor_sdk::{CditorError, command::CommandState, event::CditorEvent};

impl CditorV2View {
    pub(crate) fn apply_input_command(&mut self, command: GuiInputCommand, cx: &mut Context<Self>) {
        let show_mermaid_source_after_enter = matches!(command, GuiInputCommand::HandleEnter);
        let Some(command) = command.cditor_command() else {
            return;
        };
        if self
            .dispatch_command(command, CommandSource::Keyboard, cx)
            .is_ok()
            && show_mermaid_source_after_enter
        {
            #[cfg(feature = "mermaid")]
            crate::features::mermaid::show_focused_source_after_enter(self, cx);
        }
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
        mut command: CditorCommand,
        source: CommandSource,
        cx: &mut Context<Self>,
    ) -> Result<CommandOutcome, CditorError> {
        if command_requires_selection_materialization(&command)
            && let Some(request) = self
                .ready_session()
                .and_then(|session| session.selection_materialization_request().ok().flatten())
            && self.schedule_selection_materialization(command.clone(), source, request, cx)
        {
            return Ok(CommandOutcome::no_op());
        }

        if let CditorCommand::PasteClipboard = &command {
            command = if let Some(item) = cx.read_from_clipboard() {
                if let Some(asset) = image_asset_from_clipboard_item(&item) {
                    if let Some(provider) = self.features.asset_provider.clone() {
                        self.schedule_clipboard_asset_import(asset, provider, source, cx);
                        return Ok(CommandOutcome::no_op());
                    }
                    let Some(payload) = asset.into_fallback_payload() else {
                        return Ok(CommandOutcome::no_op());
                    };
                    CditorCommand::InsertImageAsset {
                        payload,
                        asset: None,
                        after_block_id: None,
                    }
                } else if let Some(text) = item.text() {
                    CditorCommand::ApplyClipboardData {
                        text,
                        metadata_json: item.metadata().cloned(),
                    }
                } else {
                    return Ok(CommandOutcome::no_op());
                }
            } else {
                return Ok(CommandOutcome::no_op());
            };
        }

        if let CditorCommand::CutSelection = &command {
            if let Some(session) = self.ready_session() {
                if let Ok(Some(text)) = session.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            command = CditorCommand::DeleteSelection;
        }

        let code_line_break_target = code_line_break_target(self, &command);
        let mermaid_source_line_break_target = mermaid_source_line_break_target(self, &command);
        let reveal_focused_block = keyboard_command_reveals_focused_block(&command, source);
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

        if matches!(command, CditorCommand::CopySelectionAsMarkdown) {
            let session = self.ready_session().ok_or(CditorError::NotReady)?;
            let selection = if let Some((block_id, range)) =
                crate::input::keyboard::selected_table_axis_range(
                    session,
                    self.projected_table_axis_selection(),
                ) {
                session
                    .table_clipboard_selection(block_id, range)
                    .map_err(protocol_command_error)?
            } else {
                session
                    .clipboard_snapshot()
                    .map_err(protocol_command_error)?
                    .selection
            }
            .ok_or(CditorError::InvalidSelection)?;
            let markdown =
                cditor_import_export::markdown::export_clipboard_selection_markdown(&selection);
            cx.write_to_clipboard(ClipboardItem::new_string(markdown));
            return Ok(CommandOutcome::applied_side_effect(false));
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

        if let CditorCommand::CopyBlockLink { block_id } = command {
            let document_id = self
                .ready_session()
                .ok_or(CditorError::NotReady)?
                .snapshot()
                .map_err(protocol_command_error)?
                .document_id;
            let link = self
                .features
                .block_link_provider
                .as_ref()
                .map(|provider| provider(block_id))
                .unwrap_or_else(|| {
                    cditor_core::internal_link::BlockLinkPresentation::plain(
                        cditor_core::internal_link::block_link(document_id, block_id),
                    )
                });
            let selection = ClipboardSelection::DocumentLink {
                label: link.label,
                href: link.href,
            };
            let (text, envelope) =
                crate::input::clipboard::envelope_for_selection(Some(document_id), selection);
            cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(text, envelope));
            return Ok(CommandOutcome::applied_side_effect(false));
        }

        if runtime_dispatches(&command) {
            let mutates_document = command_mutates_document(&command);
            let dispatched = self
                .ready_session()
                .ok_or(CditorError::NotReady)?
                .dispatch_with_snapshot(CommandEnvelope::new(command, source))
                .map_err(protocol_command_error)?;
            if dispatched.revision != dispatched.before_revision && mutates_document {
                self.mark_dirty_at_revision(
                    change_origin_for_source(source),
                    dispatched.revision,
                    cx,
                );
            }
            let outcome = dispatched.outcome;
            if outcome.changed()
                && let Some(block_id) = code_line_break_target
            {
                self.request_code_caret_reveal_after_line_break(block_id);
            }
            if outcome.changed()
                && let Some(block_id) = mermaid_source_line_break_target
            {
                self.request_mermaid_source_caret_reveal_after_line_break(block_id);
            }
            let document_scroll_changed = if outcome.changed() && reveal_focused_block {
                self.ready_session()
                    .and_then(|session| session.ensure_focused_block_visible().ok())
                    .is_some_and(|snapshot| snapshot.changed)
            } else {
                false
            };
            if outcome.selection_changed
                && let Some(selection) = self.sdk_selection()
            {
                cx.emit(CditorEvent::SelectionChanged { selection });
            }
            if outcome.request_repaint || document_scroll_changed {
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

    fn schedule_clipboard_asset_import(
        &mut self,
        asset: crate::clipboard_assets::ClipboardImageAsset,
        provider: std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>,
        source: CommandSource,
        cx: &mut Context<Self>,
    ) {
        let after_block_id = self
            .ready_session()
            .and_then(|session| session.document_snapshot().ok())
            .and_then(|snapshot| snapshot.focused_block_id);
        let task = cx.background_spawn(async move {
            let input = asset
                .into_asset_input()
                .map_err(|message| cditor_sdk::providers::AssetError { message })?;
            crate::provider_io::import_asset(provider, input).await
        });
        cx.spawn(async move |view, cx| {
            let imported = task.await;
            let _ = view.update(cx, |view, cx| match imported {
                Ok(imported) => {
                    let payload = cditor_core::rich_text::ImagePayload {
                        source: imported.reference.source,
                        alt: imported
                            .reference
                            .name
                            .unwrap_or_else(|| imported.snapshot.file_name.clone()),
                        caption: String::new().into(),
                        display_width_ratio_milli: None,
                    };
                    if let Err(error) = view.dispatch_command(
                        CditorCommand::InsertImageAsset {
                            payload,
                            asset: Some(imported.snapshot),
                            after_block_id,
                        },
                        source,
                        cx,
                    ) {
                        crate::overlays::show_toast(
                            view,
                            format!("Failed to insert image: {error}"),
                            Duration::from_secs(5),
                            cx,
                        );
                    }
                }
                Err(error) => crate::overlays::show_toast(
                    view,
                    format!("Failed to import image: {error}"),
                    Duration::from_secs(5),
                    cx,
                ),
            });
        })
        .detach();
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

fn code_line_break_target(view: &CditorV2View, command: &CditorCommand) -> Option<BlockId> {
    if !matches!(
        command,
        CditorCommand::HandleEnter | CditorCommand::InsertSoftLineBreak
    ) {
        return None;
    }
    let (block_id, kind) = view.ready_session()?.focused_block_kind().ok().flatten()?;
    matches!(kind, cditor_core::rich_text::RichBlockKind::Code { .. }).then_some(block_id)
}

fn mermaid_source_line_break_target(
    view: &CditorV2View,
    command: &CditorCommand,
) -> Option<BlockId> {
    if !matches!(
        command,
        CditorCommand::HandleEnter | CditorCommand::InsertSoftLineBreak
    ) {
        return None;
    }
    let (block_id, kind) = view.ready_session()?.focused_block_kind().ok().flatten()?;
    if !matches!(kind, cditor_core::rich_text::RichBlockKind::Mermaid) {
        return None;
    }
    // Only when the block is currently showing/editing its source.
    if view.cache.mermaid_source_blocks.contains(&block_id) {
        Some(block_id)
    } else {
        None
    }
}

fn keyboard_command_reveals_focused_block(command: &CditorCommand, source: CommandSource) -> bool {
    source == CommandSource::Keyboard
        && matches!(
            command,
            CditorCommand::InsertParagraphAfterFocused
                | CditorCommand::InsertSoftLineBreak
                | CditorCommand::HandleEnter
                | CditorCommand::IndentBlock
                | CditorCommand::OutdentBlock
                | CditorCommand::DeleteBackward
                | CditorCommand::DeleteForward
        )
}

fn command_requires_selection_materialization(command: &CditorCommand) -> bool {
    matches!(
        command,
        CditorCommand::CopySelection
            | CditorCommand::CopySelectionAsMarkdown
            | CditorCommand::CutSelection
            | CditorCommand::DeleteSelection
            | CditorCommand::DeleteBackward
            | CditorCommand::DeleteForward
    )
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
            | CditorCommand::NavigateTableCell { .. }
            | CditorCommand::SetTextSurfaceSelection { .. }
            | CditorCommand::DeleteSelection
            | CditorCommand::ApplyClipboardData { .. }
            | CditorCommand::InsertImageAsset { .. }
            | CditorCommand::InsertVideoAsset { .. }
            | CditorCommand::SetVideoSource { .. }
            | CditorCommand::SetPageCover { .. }
            | CditorCommand::SetPageIconEmoji { .. }
            | CditorCommand::SetPageIconAsset { .. }
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
            | CditorCommand::NavigateTableCell { .. }
            | CditorCommand::SetTextSurfaceSelection { .. }
            | CditorCommand::CopySelection
            | CditorCommand::CopySelectionAsMarkdown
            | CditorCommand::CopyBlockText { .. }
            | CditorCommand::CopyBlockLink { .. }
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
