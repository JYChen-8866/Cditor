use gpui::Context;

use crate::app::cditor_v2_view::{CditorV2View, CditorViewState, TableCellLayoutKey};
use crate::app::input_trace::trace_input;
use crate::image_preview::close_active_preview_if_escape_enabled;
use crate::input::{AiPromptEditAction, CodeLanguageEditAction, GuiInputCommand};
use crate::platform::normalize_external_line_endings;
use cditor_runtime::DocumentRuntime;

use super::keyboard::mermaid_preview_blocks_command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum BoundInputAction {
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
    pub(in crate::app) fn handle_bound_input_action(
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

        if self.table_interaction_mode.cell_selection().is_some() {
            if matches!(action, BoundInputAction::Cancel) {
                self.dismiss_table_menu_from_gui(cx);
            }
            // The compact cell menu has no text field. While it is open, all
            // keyboard input is consumed so it cannot edit the cell behind it.
            cx.stop_propagation();
            return;
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
        _cx: &mut Context<Self>,
    ) -> bool {
        let (parley_target, next_preferred_x) = table_cell_parley_target(
            &self.table_cell_layouts,
            self.ready_runtime_ref(),
            action,
            self.preferred_text_navigation_x,
        );
        self.preferred_text_navigation_x = next_preferred_x;
        let vertical_selection_target = match action {
            BoundInputAction::MoveUp {
                extend_selection: true,
            } => table_cell_vertical_selection_target(
                &self.table_cell_layouts,
                self.ready_runtime_ref(),
                -1,
            ),
            BoundInputAction::MoveDown {
                extend_selection: true,
            } => table_cell_vertical_selection_target(
                &self.table_cell_layouts,
                self.ready_runtime_ref(),
                1,
            ),
            _ => None,
        };
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return false;
        };
        if runtime.focused_table_cell_offset().is_none() {
            return false;
        }
        match action {
            BoundInputAction::Cancel => {
                runtime.blur_table_cell();
                false
            }
            BoundInputAction::Tab { backwards } => runtime
                .move_focused_table_cell_tab(backwards)
                .unwrap_or(false),
            BoundInputAction::MoveLeft {
                extend_selection: true,
            } => parley_target
                .and_then(|position| {
                    runtime
                        .move_focused_table_cell_to_text_position(
                            position.offset,
                            position.affinity,
                            true,
                        )
                        .ok()
                })
                .unwrap_or_else(|| {
                    runtime
                        .extend_focused_table_cell_selection_left()
                        .unwrap_or(false)
                }),
            BoundInputAction::MoveLeft {
                extend_selection: false,
            } => parley_target
                .and_then(|position| {
                    runtime
                        .move_focused_table_cell_to_text_position(
                            position.offset,
                            position.affinity,
                            false,
                        )
                        .ok()
                })
                .unwrap_or_else(|| runtime.move_focused_table_cell_left().unwrap_or(false)),
            BoundInputAction::MoveRight {
                extend_selection: true,
            } => parley_target
                .and_then(|position| {
                    runtime
                        .move_focused_table_cell_to_text_position(
                            position.offset,
                            position.affinity,
                            true,
                        )
                        .ok()
                })
                .unwrap_or_else(|| {
                    runtime
                        .extend_focused_table_cell_selection_right()
                        .unwrap_or(false)
                }),
            BoundInputAction::MoveRight {
                extend_selection: false,
            } => parley_target
                .and_then(|position| {
                    runtime
                        .move_focused_table_cell_to_text_position(
                            position.offset,
                            position.affinity,
                            false,
                        )
                        .ok()
                })
                .unwrap_or_else(|| runtime.move_focused_table_cell_right().unwrap_or(false)),
            BoundInputAction::MoveUp {
                extend_selection: true,
            }
            | BoundInputAction::MoveDown {
                extend_selection: true,
            } => parley_target
                .and_then(|position| {
                    runtime
                        .move_focused_table_cell_to_text_position(
                            position.offset,
                            position.affinity,
                            true,
                        )
                        .ok()
                })
                .or_else(|| {
                    vertical_selection_target.and_then(|target| {
                        runtime
                            .extend_focused_table_cell_selection_to_offset(target)
                            .ok()
                    })
                })
                .unwrap_or(false),
            BoundInputAction::MoveUp {
                extend_selection: false,
            } => parley_target
                .and_then(|position| {
                    runtime
                        .move_focused_table_cell_to_text_position(
                            position.offset,
                            position.affinity,
                            false,
                        )
                        .ok()
                })
                .unwrap_or_else(|| runtime.move_focused_table_cell_up().unwrap_or(false)),
            BoundInputAction::MoveDown {
                extend_selection: false,
            } => parley_target
                .and_then(|position| {
                    runtime
                        .move_focused_table_cell_to_text_position(
                            position.offset,
                            position.affinity,
                            false,
                        )
                        .ok()
                })
                .unwrap_or_else(|| runtime.move_focused_table_cell_down().unwrap_or(false)),
            BoundInputAction::MoveToLineStart { extend_selection }
            | BoundInputAction::MoveToLineEnd { extend_selection }
            | BoundInputAction::MoveToPreviousWord { extend_selection }
            | BoundInputAction::MoveToNextWord { extend_selection } => parley_target
                .and_then(|position| {
                    runtime
                        .move_focused_table_cell_to_text_position(
                            position.offset,
                            position.affinity,
                            extend_selection,
                        )
                        .ok()
                })
                .unwrap_or(false),
            _ => return false,
        };
        self.slash_menu = None;
        true
    }

    pub(in crate::app) fn focused_mermaid_is_preview(&self) -> bool {
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

fn table_cell_parley_target(
    layouts: &std::collections::HashMap<TableCellLayoutKey, crate::text::RichTextPlatformLayout>,
    runtime: Option<&DocumentRuntime>,
    action: BoundInputAction,
    preferred_x: Option<(cditor_core::ids::SurfaceId, f32)>,
) -> (
    Option<crate::text::ParleyTextPosition>,
    Option<(cditor_core::ids::SurfaceId, f32)>,
) {
    use crate::text::{
        ParleyMoveCommand, ParleySelection, ParleyTextPosition, TextGeometryOperation,
        record_snapshot_geometry, record_unavailable_geometry,
    };

    let (command, extend) = match action {
        BoundInputAction::MoveLeft { extend_selection } => {
            (ParleyMoveCommand::PreviousVisual, extend_selection)
        }
        BoundInputAction::MoveRight { extend_selection } => {
            (ParleyMoveCommand::NextVisual, extend_selection)
        }
        BoundInputAction::MoveUp { extend_selection } => {
            (ParleyMoveCommand::PreviousLine, extend_selection)
        }
        BoundInputAction::MoveDown { extend_selection } => {
            (ParleyMoveCommand::NextLine, extend_selection)
        }
        BoundInputAction::MoveToLineStart { extend_selection } => {
            (ParleyMoveCommand::LineStart, extend_selection)
        }
        BoundInputAction::MoveToLineEnd { extend_selection } => {
            (ParleyMoveCommand::LineEnd, extend_selection)
        }
        BoundInputAction::MoveToPreviousWord { extend_selection } => {
            (ParleyMoveCommand::PreviousVisualWord, extend_selection)
        }
        BoundInputAction::MoveToNextWord { extend_selection } => {
            (ParleyMoveCommand::NextVisualWord, extend_selection)
        }
        _ => return (None, None),
    };
    let Some(runtime) = runtime else {
        return (None, None);
    };
    let Some((block_id, row, col, offset, affinity)) = runtime.focused_table_cell_text_position()
    else {
        return (None, None);
    };
    let surface_id = cditor_core::ids::SurfaceId::TableCell {
        block_id,
        row,
        column: col,
    };
    let Some(cache) = layouts.get(&TableCellLayoutKey { block_id, row, col }) else {
        record_unavailable_geometry();
        return (
            None,
            preferred_x.filter(|(surface, _)| *surface == surface_id),
        );
    };
    if Some(cache.content_version) != runtime.block_content_version(block_id) {
        record_unavailable_geometry();
        return (
            None,
            preferred_x.filter(|(surface, _)| *surface == surface_id),
        );
    }
    let layout = &cache.snapshot;
    record_snapshot_geometry(TextGeometryOperation::Navigation);
    let anchor_offset = runtime
        .focused_table_cell_selection_state()
        .filter(|(id, focused_row, focused_col, _, _, _)| {
            (*id, *focused_row, *focused_col) == (block_id, row, col)
        })
        .map(|(_, _, _, range, reversed, _)| {
            if range.is_empty() {
                offset
            } else if reversed {
                range.end
            } else {
                range.start
            }
        })
        .unwrap_or(offset);
    let selection = ParleySelection {
        anchor: ParleyTextPosition::downstream(anchor_offset),
        focus: ParleyTextPosition { offset, affinity },
    };
    let current_preferred_x = preferred_x
        .filter(|(surface, _)| *surface == surface_id)
        .map(|(_, x)| x);
    let (moved, next_preferred_x) =
        layout.move_selection_with_preferred_x(selection, command, extend, current_preferred_x);
    (
        (moved.focus != (ParleyTextPosition { offset, affinity })).then_some(moved.focus),
        next_preferred_x.map(|x| (surface_id, x)),
    )
}

fn table_cell_vertical_selection_target(
    layouts: &std::collections::HashMap<TableCellLayoutKey, crate::text::RichTextPlatformLayout>,
    runtime: Option<&DocumentRuntime>,
    direction: i32,
) -> Option<usize> {
    let runtime = runtime?;
    let (block_id, row, col, caret) = runtime.focused_table_cell_offset()?;
    let cache = layouts.get(&TableCellLayoutKey { block_id, row, col })?;
    if cache.content_version != runtime.block_content_version(block_id)? {
        return None;
    }
    let bounds = crate::text::platform_range_bounds(cache, caret..caret);
    let target = gpui::point(
        bounds.left() + bounds.size.width / 2.0,
        bounds.top() + bounds.size.height * direction as f32,
    );
    let next = crate::text::platform_index_for_point(cache, target);
    Some(if next == caret {
        if direction < 0 {
            0
        } else {
            cache.snapshot.text().len()
        }
    } else {
        next
    })
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
