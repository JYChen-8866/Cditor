use gpui::Context;

use crate::editor_view::CditorV2View;
use crate::image_preview::close_active_preview_if_escape_enabled;
use crate::input::trace::trace_input;
use crate::input::{AiPromptEditAction, CodeLanguageEditAction, GuiInputCommand};
use crate::platform::normalize_external_line_endings;

use super::keyboard::mermaid_preview_blocks_command;
use crate::input::table_cell_navigation::{
    table_cell_navigation_command, table_cell_offset_selection_command,
    table_cell_text_layout_selection_command, table_cell_text_layout_target,
    table_cell_vertical_selection_target,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundInputAction {
    Newline,
    SoftLineBreak,
    NewlineBelow,
    Tab { backwards: bool },
    Cancel,
    MoveLeft { extend_selection: bool },
    MoveRight { extend_selection: bool },
    MoveUp { extend_selection: bool },
    MoveDown { extend_selection: bool },
    MoveToPreviousWord { extend_selection: bool },
    MoveToNextWord { extend_selection: bool },
    MoveToDocumentStart { extend_selection: bool },
    MoveToDocumentEnd { extend_selection: bool },
    MoveToLineStart { extend_selection: bool },
    MoveToLineEnd { extend_selection: bool },
    DeleteBackward,
    DeleteForward,
    Duplicate,
    Command(GuiInputCommand),
}

impl CditorV2View {
    pub(crate) fn handle_bound_input_action(
        &mut self,
        action: BoundInputAction,
        cx: &mut Context<Self>,
    ) {
        self.interaction.note_input();
        self.pause_caret_blink(cx);
        trace_input(
            "action.dispatch",
            format_args!(
                "action={action:?} focus={:?} target={:?}",
                self.ready_session()
                    .and_then(|session| { session.document_snapshot().ok()?.focused_block_id }),
                self.ready_session()
                    .and_then(|session| session.input_context().ok()?.target),
            ),
        );

        if matches!(action, BoundInputAction::Cancel) && self.interaction.cancel_document_drags() {
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if self.overlay.ai_prompt.is_some() {
            self.handle_bound_ai_prompt_action(action, cx);
            cx.stop_propagation();
            return;
        }
        if self.overlay.code_language_edit.is_some() {
            self.handle_bound_code_language_action(action, cx);
            cx.stop_propagation();
            return;
        }

        // AI preview 处理：但要检查是否在表格单元格内，如果是则让表格优先处理 Tab
        let in_table_cell = self
            .ready_session()
            .and_then(|session| session.table_interaction(None).ok())
            .and_then(|context| context.focused_cell)
            .is_some();

        eprintln!(
            "[routing] in_table_cell={}, action={:?}",
            in_table_cell, action
        );

        if !in_table_cell
            && self.ready_session().is_some_and(|session| {
                session
                    .ai_context()
                    .is_ok_and(|context| context.session_active)
            })
        {
            match action {
                BoundInputAction::Tab { .. } => {
                    eprintln!("[routing] AI preview intercepting Tab");
                    let _ = self.accept_ai_preview_from_gui(cx);
                    cx.stop_propagation();
                    return;
                }
                BoundInputAction::Cancel => {
                    let _ = self.reject_ai_preview_from_gui(cx);
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }

        if self
            .interaction
            .table_interaction_mode
            .cell_selection()
            .is_some()
        {
            if matches!(action, BoundInputAction::Cancel) {
                self.dismiss_table_menu_from_gui(cx);
            }
            // The compact cell menu has no text field. While it is open, all
            // keyboard input is consumed so it cannot edit the cell behind it.
            cx.stop_propagation();
            return;
        }

        if self
            .interaction
            .table_interaction_mode
            .axis_selection()
            .is_some()
        {
            match action {
                BoundInputAction::Newline => self.confirm_table_menu_from_gui(cx),
                BoundInputAction::Cancel => self.dismiss_table_menu_from_gui(cx),
                BoundInputAction::DeleteBackward => {
                    self.delete_table_menu_query_backward_from_gui(cx)
                }
                BoundInputAction::DeleteForward => {
                    self.overlay.table_menu_ui.delete_forward();
                    cx.notify();
                    true
                }
                BoundInputAction::MoveLeft { .. } => {
                    self.overlay.table_menu_ui.move_left();
                    cx.notify();
                    true
                }
                BoundInputAction::MoveRight { .. } => {
                    self.overlay.table_menu_ui.move_right();
                    cx.notify();
                    true
                }
                BoundInputAction::MoveToLineStart { .. } => {
                    self.overlay.table_menu_ui.move_to_start();
                    cx.notify();
                    true
                }
                BoundInputAction::MoveToLineEnd { .. } => {
                    self.overlay.table_menu_ui.move_to_end();
                    cx.notify();
                    true
                }
                BoundInputAction::Command(GuiInputCommand::PasteClipboard) => {
                    if let Some(text) = cx
                        .read_from_clipboard()
                        .and_then(|item| item.text())
                        .map(|text| normalize_external_line_endings(&text).replace('\n', " "))
                    {
                        self.overlay.table_menu_ui.replace_range(
                            self.overlay.table_menu_ui.input_replacement_range(),
                            &text,
                        );
                        cx.notify();
                    }
                    true
                }
                BoundInputAction::Duplicate => self.duplicate_selected_table_axis_from_gui(cx),
                _ => false,
            };
            // An open table menu owns the keyboard context. Unsupported
            // document actions are consumed so they cannot mutate the table
            // behind the menu.
            cx.stop_propagation();
            return;
        }

        if self.overlay.slash_menu.is_some() {
            let handled = match action {
                BoundInputAction::Cancel => self.cancel_slash_menu(cx),
                BoundInputAction::MoveUp { .. } => self.move_slash_menu_selection(-1, cx),
                BoundInputAction::MoveDown { .. } => self.move_slash_menu_selection(1, cx),
                BoundInputAction::Newline
                | BoundInputAction::SoftLineBreak
                | BoundInputAction::Tab { .. } => {
                    let _ = self.apply_selected_slash_menu_item(cx);
                    true
                }
                _ => false,
            };
            if handled {
                cx.stop_propagation();
                return;
            }
        }

        if self.handle_bound_table_cell_action(action, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if matches!(action, BoundInputAction::Cancel) {
            let _ = self.dismiss_table_menu_from_gui(cx);
            let _ = self.overlay.dismiss_topmost_formatting_layer();
            let _ = close_active_preview_if_escape_enabled(cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }

        let command = command_for_bound_action(action);
        if self.focused_mermaid_is_preview() {
            if matches!(command, GuiInputCommand::HandleEnter) {
                self.apply_input_command(GuiInputCommand::InsertParagraphAfterFocused, cx);
            } else if !mermaid_preview_blocks_command(command) {
                self.apply_input_command(command, cx);
            }
        } else {
            self.apply_input_command(command, cx);
        }
        self.sync_slash_menu_from_runtime(cx);
        cx.stop_propagation();
        cx.notify();
    }

    fn handle_bound_ai_prompt_action(&mut self, action: BoundInputAction, cx: &mut Context<Self>) {
        let edit_action = match action {
            BoundInputAction::Newline | BoundInputAction::NewlineBelow => {
                Some(AiPromptEditAction::Submit)
            }
            BoundInputAction::Cancel => Some(AiPromptEditAction::Cancel),
            BoundInputAction::MoveLeft { .. } => Some(AiPromptEditAction::MoveLeft),
            BoundInputAction::MoveRight { .. } => Some(AiPromptEditAction::MoveRight),
            BoundInputAction::MoveToLineStart { .. } => Some(AiPromptEditAction::MoveToStart),
            BoundInputAction::MoveToLineEnd { .. } => Some(AiPromptEditAction::MoveToEnd),
            BoundInputAction::DeleteBackward => Some(AiPromptEditAction::DeleteBackward),
            BoundInputAction::DeleteForward => Some(AiPromptEditAction::DeleteForward),
            _ => None,
        };
        if let Some(edit_action) = edit_action {
            let _ = self.apply_ai_prompt_action_from_gui(edit_action, cx);
            return;
        }
        if matches!(
            action,
            BoundInputAction::Command(GuiInputCommand::PasteClipboard)
        ) {
            let text = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .map(|text| normalize_external_line_endings(&text).replace('\n', " "));
            if let Some(text) = text
                && let Some(prompt) = self.overlay.ai_prompt.as_mut()
            {
                prompt.replace_range(prompt.input_replacement_range(), &text);
                cx.notify();
            }
        }
        // The prompt is intentionally single-line. Soft breaks, tab and
        // document-only formatting commands are consumed instead of leaking
        // through to the document behind the prompt.
    }

    fn handle_bound_code_language_action(
        &mut self,
        action: BoundInputAction,
        cx: &mut Context<Self>,
    ) {
        let edit_action = match action {
            BoundInputAction::Newline
            | BoundInputAction::NewlineBelow
            | BoundInputAction::SoftLineBreak
            | BoundInputAction::Tab { .. } => Some(CodeLanguageEditAction::Commit),
            BoundInputAction::Cancel => Some(CodeLanguageEditAction::Cancel),
            BoundInputAction::MoveLeft { .. } => Some(CodeLanguageEditAction::MoveLeft),
            BoundInputAction::MoveRight { .. } => Some(CodeLanguageEditAction::MoveRight),
            BoundInputAction::MoveUp { .. } => Some(CodeLanguageEditAction::SelectPrevious),
            BoundInputAction::MoveDown { .. } => Some(CodeLanguageEditAction::SelectNext),
            BoundInputAction::MoveToLineStart { .. } => Some(CodeLanguageEditAction::MoveToStart),
            BoundInputAction::MoveToLineEnd { .. } => Some(CodeLanguageEditAction::MoveToEnd),
            BoundInputAction::DeleteBackward => Some(CodeLanguageEditAction::DeleteBackward),
            BoundInputAction::DeleteForward => Some(CodeLanguageEditAction::DeleteForward),
            _ => None,
        };
        if let Some(edit_action) = edit_action {
            let _ = self.apply_code_language_action_from_gui(edit_action, cx);
            return;
        }
        if matches!(
            action,
            BoundInputAction::Command(GuiInputCommand::PasteClipboard)
        ) {
            let text = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .map(|text| normalize_external_line_endings(&text).replace('\n', " "));
            if let Some(text) = text
                && let Some(edit) = self.overlay.code_language_edit.as_mut()
            {
                edit.replace_range(edit.input_replacement_range(), &text);
                cx.notify();
            }
        }
    }

    fn handle_bound_table_cell_action(
        &mut self,
        action: BoundInputAction,
        cx: &mut Context<Self>,
    ) -> bool {
        eprintln!("[routing] handle_bound_table_cell_action: {:?}", action);
        let table_context = self
            .ready_session()
            .and_then(|session| session.table_interaction(None).ok());
        eprintln!(
            "[routing] table_context present: {}",
            table_context.is_some()
        );
        if let Some(ref ctx) = table_context {
            eprintln!(
                "[routing] focused_cell present: {}",
                ctx.focused_cell.is_some()
            );
        }
        let (text_layout_target, next_preferred_x, vertical_selection_target) = {
            let current_layout = table_context
                .as_ref()
                .and_then(|context| context.focused_cell.as_ref())
                .and_then(|focused| {
                    let surface_id = crate::surfaces::table_cell::surface_id(
                        focused.block_id,
                        cditor_runtime::TableCellPosition {
                            row: focused.row,
                            col: focused.col,
                        },
                    );
                    let current = self
                        .ready_session()?
                        .surface_version(surface_id)
                        .ok()
                        .flatten()?;
                    self.current_table_cell_layout_cache(
                        current,
                        focused.block_id,
                        focused.row,
                        focused.col,
                    )
                });
            let (text_layout_target, next_preferred_x) = table_cell_text_layout_target(
                current_layout,
                table_context.as_ref(),
                action,
                self.input.preferred_navigation_x,
            );
            let vertical_selection_target = match action {
                BoundInputAction::MoveUp {
                    extend_selection: true,
                } => {
                    table_cell_vertical_selection_target(current_layout, table_context.as_ref(), -1)
                }
                BoundInputAction::MoveDown {
                    extend_selection: true,
                } => {
                    table_cell_vertical_selection_target(current_layout, table_context.as_ref(), 1)
                }
                _ => None,
            };
            (
                text_layout_target,
                next_preferred_x,
                vertical_selection_target,
            )
        };
        self.input.preferred_navigation_x = next_preferred_x;
        let Some(table_context) = table_context else {
            return false;
        };
        if table_context.focused_cell.is_none() {
            return false;
        }
        let mut dispatch = |command| {
            self.dispatch_command(
                command,
                cditor_editor_protocol::command::CommandSource::Keyboard,
                cx,
            )
            .is_ok_and(|outcome| outcome.changed())
        };
        eprintln!("[routing] match action: {:?}", action);
        match action {
            BoundInputAction::Cancel => {
                let _ = dispatch(cditor_editor_protocol::command::CditorCommand::BlurTableCell);
                false
            }
            BoundInputAction::Tab { backwards } => {
                eprintln!("[routing] Tab handler reached!");
                let direction = if backwards {
                    cditor_editor_protocol::command::TableCellNavigationDirection::TabBackward
                } else {
                    cditor_editor_protocol::command::TableCellNavigationDirection::TabForward
                };
                eprintln!(
                    "[routing] dispatching Tab command: direction={:?}",
                    direction
                );
                let result = dispatch(table_cell_navigation_command(direction, false));
                eprintln!("[routing] Tab command result: {}", result);
                result
            }
            BoundInputAction::MoveLeft {
                extend_selection: true,
            } => text_layout_target
                .and_then(|position| {
                    table_cell_text_layout_selection_command(&table_context, position, true)
                })
                .map(&mut dispatch)
                .unwrap_or_else(|| {
                    dispatch(table_cell_navigation_command(
                        cditor_editor_protocol::command::TableCellNavigationDirection::Left,
                        true,
                    ))
                }),
            BoundInputAction::MoveLeft {
                extend_selection: false,
            } => text_layout_target
                .and_then(|position| {
                    table_cell_text_layout_selection_command(&table_context, position, false)
                })
                .map(&mut dispatch)
                .unwrap_or_else(|| {
                    dispatch(table_cell_navigation_command(
                        cditor_editor_protocol::command::TableCellNavigationDirection::Left,
                        false,
                    ))
                }),
            BoundInputAction::MoveRight {
                extend_selection: true,
            } => text_layout_target
                .and_then(|position| {
                    table_cell_text_layout_selection_command(&table_context, position, true)
                })
                .map(&mut dispatch)
                .unwrap_or_else(|| {
                    dispatch(table_cell_navigation_command(
                        cditor_editor_protocol::command::TableCellNavigationDirection::Right,
                        true,
                    ))
                }),
            BoundInputAction::MoveRight {
                extend_selection: false,
            } => text_layout_target
                .and_then(|position| {
                    table_cell_text_layout_selection_command(&table_context, position, false)
                })
                .map(&mut dispatch)
                .unwrap_or_else(|| {
                    dispatch(table_cell_navigation_command(
                        cditor_editor_protocol::command::TableCellNavigationDirection::Right,
                        false,
                    ))
                }),
            BoundInputAction::MoveUp {
                extend_selection: true,
            }
            | BoundInputAction::MoveDown {
                extend_selection: true,
            } => text_layout_target
                .and_then(|position| {
                    table_cell_text_layout_selection_command(&table_context, position, true)
                })
                .map(&mut dispatch)
                .or_else(|| {
                    vertical_selection_target.and_then(|target| {
                        table_cell_offset_selection_command(
                            &table_context,
                            target,
                            cditor_core::edit::TextAffinity::Downstream,
                            true,
                        )
                        .map(&mut dispatch)
                    })
                })
                .unwrap_or(false),
            BoundInputAction::MoveUp {
                extend_selection: false,
            } => text_layout_target
                .and_then(|position| {
                    table_cell_text_layout_selection_command(&table_context, position, false)
                })
                .map(&mut dispatch)
                .unwrap_or_else(|| {
                    dispatch(table_cell_navigation_command(
                        cditor_editor_protocol::command::TableCellNavigationDirection::Up,
                        false,
                    ))
                }),
            BoundInputAction::MoveDown {
                extend_selection: false,
            } => text_layout_target
                .and_then(|position| {
                    table_cell_text_layout_selection_command(&table_context, position, false)
                })
                .map(&mut dispatch)
                .unwrap_or_else(|| {
                    dispatch(table_cell_navigation_command(
                        cditor_editor_protocol::command::TableCellNavigationDirection::Down,
                        false,
                    ))
                }),
            BoundInputAction::MoveToLineStart { extend_selection }
            | BoundInputAction::MoveToLineEnd { extend_selection }
            | BoundInputAction::MoveToPreviousWord { extend_selection }
            | BoundInputAction::MoveToNextWord { extend_selection } => text_layout_target
                .and_then(|position| {
                    table_cell_text_layout_selection_command(
                        &table_context,
                        position,
                        extend_selection,
                    )
                })
                .map(&mut dispatch)
                .unwrap_or(false),
            _ => return false,
        };
        eprintln!("[routing] after match, returning from handle_bound_table_cell_action");
        self.overlay.slash_menu = None;
        true
    }

    pub(crate) fn focused_mermaid_is_preview(&self) -> bool {
        #[cfg(not(feature = "mermaid"))]
        return false;

        #[cfg(feature = "mermaid")]
        {
            let Some(session) = self.ready_session() else {
                return false;
            };
            let Some(block_id) = session
                .document_snapshot()
                .ok()
                .and_then(|snapshot| snapshot.focused_block_id)
            else {
                return false;
            };
            !self.cache.mermaid_source_blocks.contains(&block_id)
                && session.text_block_context(block_id).is_ok_and(|context| {
                    context.is_some_and(|context| {
                        matches!(context.kind, cditor_core::rich_text::RichBlockKind::Mermaid)
                    })
                })
        }
    }
}

fn command_for_bound_action(action: BoundInputAction) -> GuiInputCommand {
    match action {
        BoundInputAction::Newline => GuiInputCommand::HandleEnter,
        BoundInputAction::SoftLineBreak => GuiInputCommand::InsertSoftLineBreak,
        BoundInputAction::NewlineBelow => GuiInputCommand::InsertParagraphAfterFocused,
        BoundInputAction::Tab { backwards: true } => GuiInputCommand::OutdentBlock,
        BoundInputAction::Tab { backwards: false } => GuiInputCommand::IndentBlock,
        BoundInputAction::MoveLeft { extend_selection } => {
            GuiInputCommand::MoveCaretLeft { extend_selection }
        }
        BoundInputAction::MoveRight { extend_selection } => {
            GuiInputCommand::MoveCaretRight { extend_selection }
        }
        BoundInputAction::MoveUp { extend_selection } => {
            GuiInputCommand::MoveCaretUp { extend_selection }
        }
        BoundInputAction::MoveDown { extend_selection } => {
            GuiInputCommand::MoveCaretDown { extend_selection }
        }
        BoundInputAction::MoveToPreviousWord { extend_selection } => {
            GuiInputCommand::MoveCaretToPreviousWord { extend_selection }
        }
        BoundInputAction::MoveToNextWord { extend_selection } => {
            GuiInputCommand::MoveCaretToNextWord { extend_selection }
        }
        BoundInputAction::MoveToDocumentStart { extend_selection } => {
            GuiInputCommand::MoveCaretToDocumentStart { extend_selection }
        }
        BoundInputAction::MoveToDocumentEnd { extend_selection } => {
            GuiInputCommand::MoveCaretToDocumentEnd { extend_selection }
        }
        BoundInputAction::MoveToLineStart { extend_selection } => {
            GuiInputCommand::MoveCaretToLineStart { extend_selection }
        }
        BoundInputAction::MoveToLineEnd { extend_selection } => {
            GuiInputCommand::MoveCaretToLineEnd { extend_selection }
        }
        BoundInputAction::DeleteBackward => GuiInputCommand::DeleteBackward,
        BoundInputAction::DeleteForward => GuiInputCommand::DeleteForward,
        BoundInputAction::Duplicate => GuiInputCommand::Ignore,
        BoundInputAction::Command(command) => command,
        BoundInputAction::Cancel => GuiInputCommand::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_enter_uses_the_document_structure_command() {
        assert_eq!(
            command_for_bound_action(BoundInputAction::Newline),
            GuiInputCommand::HandleEnter
        );
        assert_eq!(
            command_for_bound_action(BoundInputAction::NewlineBelow),
            GuiInputCommand::InsertParagraphAfterFocused
        );
        assert_eq!(
            command_for_bound_action(BoundInputAction::Duplicate),
            GuiInputCommand::Ignore
        );
    }
}
