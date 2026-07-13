use gpui::Context;

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::app::input_trace::trace_input;
use crate::gui::image_preview::close_active_preview_if_escape_enabled;
use crate::gui::input::{AiPromptEditAction, CodeLanguageEditAction, GuiInputCommand};
use crate::gui::platform::normalize_external_line_endings;
use cditor_runtime::DocumentRuntime;

use super::keyboard::mermaid_preview_blocks_command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::gui::app) enum BoundInputAction {
    Newline,
    SoftLineBreak,
    NewlineBelow,
    Tab { backwards: bool },
    Cancel,
    MoveLeft { extend_selection: bool },
    MoveRight { extend_selection: bool },
    MoveUp { extend_selection: bool },
    MoveDown { extend_selection: bool },
    MoveToLineStart { extend_selection: bool },
    MoveToLineEnd { extend_selection: bool },
    DeleteBackward,
    DeleteForward,
    Duplicate,
    Command(GuiInputCommand),
}

impl CditorV2View {
    pub(in crate::gui::app) fn handle_bound_input_action(
        &mut self,
        action: BoundInputAction,
        cx: &mut Context<Self>,
    ) {
        trace_input(
            "action.dispatch",
            format_args!(
                "action={action:?} focus={:?} target={:?}",
                self.ready_runtime_ref()
                    .and_then(DocumentRuntime::focused_block_id),
                self.ready_runtime_ref()
                    .and_then(DocumentRuntime::input_session_target),
            ),
        );

        if self.ai_prompt.is_some() {
            self.handle_bound_ai_prompt_action(action, cx);
            cx.stop_propagation();
            return;
        }
        if self.code_language_edit.is_some() {
            self.handle_bound_code_language_action(action, cx);
            cx.stop_propagation();
            return;
        }

        if self
            .ready_runtime_ref()
            .is_some_and(|runtime| runtime.ai_session_snapshot().is_some())
        {
            match action {
                BoundInputAction::Tab { .. } => {
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

        if self.table_interaction_mode.axis_selection().is_some() {
            match action {
                BoundInputAction::Newline => self.confirm_table_menu_from_gui(cx),
                BoundInputAction::Cancel => self.dismiss_table_menu_from_gui(cx),
                BoundInputAction::DeleteBackward => {
                    self.delete_table_menu_query_backward_from_gui(cx)
                }
                BoundInputAction::DeleteForward => {
                    self.table_menu_ui.delete_forward();
                    cx.notify();
                    true
                }
                BoundInputAction::MoveLeft { .. } => {
                    self.table_menu_ui.move_left();
                    cx.notify();
                    true
                }
                BoundInputAction::MoveRight { .. } => {
                    self.table_menu_ui.move_right();
                    cx.notify();
                    true
                }
                BoundInputAction::MoveToLineStart { .. } => {
                    self.table_menu_ui.move_to_start();
                    cx.notify();
                    true
                }
                BoundInputAction::MoveToLineEnd { .. } => {
                    self.table_menu_ui.move_to_end();
                    cx.notify();
                    true
                }
                BoundInputAction::Command(GuiInputCommand::PasteClipboard) => {
                    if let Some(text) = cx
                        .read_from_clipboard()
                        .and_then(|item| item.text())
                        .map(|text| normalize_external_line_endings(&text).replace('\n', " "))
                    {
                        self.table_menu_ui
                            .replace_range(self.table_menu_ui.input_replacement_range(), &text);
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

        if self.slash_menu.is_some() {
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
                && let Some(prompt) = self.ai_prompt.as_mut()
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
                && let Some(edit) = self.code_language_edit.as_mut()
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
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return false;
        };
        if runtime.focused_table_cell_offset().is_none() {
            return false;
        }
        let changed = match action {
            BoundInputAction::Cancel => {
                runtime.blur_table_cell();
                false
            }
            BoundInputAction::Tab { backwards } => runtime
                .move_focused_table_cell_tab(backwards)
                .unwrap_or(false),
            BoundInputAction::MoveLeft { .. } => {
                runtime.move_focused_table_cell_left().unwrap_or(false)
            }
            BoundInputAction::MoveRight { .. } => {
                runtime.move_focused_table_cell_right().unwrap_or(false)
            }
            BoundInputAction::MoveUp { .. } => {
                runtime.move_focused_table_cell_up().unwrap_or(false)
            }
            BoundInputAction::MoveDown { .. } => {
                runtime.move_focused_table_cell_down().unwrap_or(false)
            }
            _ => return false,
        };
        self.slash_menu = None;
        if changed {
            self.mark_dirty(cx);
        }
        true
    }

    pub(in crate::gui::app) fn focused_mermaid_is_preview(&self) -> bool {
        let Some(runtime) = self.ready_runtime_ref() else {
            return false;
        };
        let Some(block_id) = runtime.focused_block_id() else {
            return false;
        };
        !self.mermaid_source_blocks.contains(&block_id)
            && runtime
                .block_payload_record(block_id)
                .is_some_and(|payload| {
                    matches!(payload.kind, cditor_core::rich_text::RichBlockKind::Mermaid)
                })
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
