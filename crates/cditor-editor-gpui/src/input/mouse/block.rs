use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{CditorCommand, CommandSource};
use gpui::{Context, Pixels, Point, Window};

use crate::editor_view::{CditorV2View, CditorViewState, block_focus_offset_after_missed_hit_test};
use crate::input::trace::trace_input;
use crate::interaction::selection_drag::GuiTextDragSelection;
use crate::interaction::table_mode::GuiTableInteractionMode;
use crate::persistence::EditorSaveStatus;

impl CditorV2View {
    pub(crate) fn focus_block_from_gui_at_position(
        &mut self,
        block_id: BlockId,
        position: impl Into<Option<Point<Pixels>>>,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pause_caret_blink(cx);
        window.focus(&self.focus.editor, cx);
        if self.interaction.table_interaction_mode.block_id().is_some() {
            self.interaction.table_interaction_mode = GuiTableInteractionMode::Idle;
            self.overlay.table_menu_ui = Default::default();
        }
        self.clear_gutter_action();
        let position = position.into();
        let text_position = position
            .and_then(|position| self.text_position_for_block_at_position(block_id, position));
        let click_selection = if let Some(kind) =
            crate::surfaces::text::selection_kind_for_click_count(click_count)
        {
            position.and_then(|position| {
                self.text_selection_for_block_at_position(block_id, position, kind)
            })
        } else {
            None
        };
        trace_input(
            "focus_block_from_gui_at_position",
            format_args!(
                "block={block_id} position={position:?} resolved_position={text_position:?}"
            ),
        );
        if matches!(&self.state, CditorViewState::Ready(_)) {
            let (selection, drag_anchor) = if let Some(selection) = click_selection {
                (
                    cditor_core::edit::DocumentSelection {
                        anchor: cditor_core::edit::TextPosition {
                            block_id,
                            offset: selection.anchor.offset,
                            affinity: selection.anchor.affinity,
                        },
                        focus: cditor_core::edit::TextPosition {
                            block_id,
                            offset: selection.focus.offset,
                            affinity: selection.focus.affinity,
                        },
                    },
                    None,
                )
            } else {
                let text_position = text_position.unwrap_or_else(|| {
                    let session = self
                        .ready_session()
                        .expect("ready view state must expose an editor session");
                    let focused_block_id = session
                        .document_snapshot()
                        .ok()
                        .and_then(|snapshot| snapshot.focused_block_id);
                    let caret_offset = session
                        .text_block_context(block_id)
                        .ok()
                        .flatten()
                        .and_then(|context| context.caret);
                    let anchor_offset = block_focus_offset_after_missed_hit_test(
                        focused_block_id,
                        block_id,
                        caret_offset,
                    );
                    crate::text::TextLayoutPosition::downstream(anchor_offset)
                });
                (
                    cditor_core::edit::DocumentSelection::caret(cditor_core::edit::TextPosition {
                        block_id,
                        offset: text_position.offset,
                        affinity: text_position.affinity,
                    }),
                    Some(text_position),
                )
            };
            match self.dispatch_command(
                CditorCommand::SetDocumentSelection { selection },
                CommandSource::Toolbar,
                cx,
            ) {
                Ok(_) => {
                    self.interaction.text_drag_selection =
                        drag_anchor.map(|anchor_position| GuiTextDragSelection {
                            anchor_block_id: block_id,
                            anchor_position,
                            pointer_position: position.unwrap_or_default(),
                        });
                }
                Err(error) => {
                    self.interaction.text_drag_selection = None;
                    trace_input(
                        "focus_block_from_gui_at_position.rejected",
                        format_args!("block={block_id} error={error}"),
                    );
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn toggle_todo_from_gui(&mut self, block_id: BlockId, cx: &mut Context<Self>) {
        let _ = self.dispatch_command(
            CditorCommand::ToggleTodo { block_id },
            CommandSource::Toolbar,
            cx,
        );
    }

    pub(crate) fn focus_down_placer_from_gui(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus.editor, cx);
        if self.status.readonly {
            return;
        }
        let result = self.dispatch_command(
            CditorCommand::EnsureTrailingParagraph,
            CommandSource::Toolbar,
            cx,
        );
        if result.is_ok()
            && let Some(session) = self.ready_session()
        {
            let _ = session.ensure_focused_block_visible();
        }
        match result {
            Ok(_) => cx.notify(),
            Err(error) => {
                self.status.save_status = EditorSaveStatus::Failed(error.to_string());
                cx.notify();
            }
        }
    }

    pub(crate) fn hover_block_from_gui(
        &mut self,
        block_id: BlockId,
        dragging: bool,
        cx: &mut Context<Self>,
    ) {
        let hover_changed = self.interaction.hovered_block_id != Some(block_id);
        self.interaction.hovered_block_id = Some(block_id);
        let mut selection_changed = false;
        if dragging
            && self.interaction.block_drag_selection.is_dragging()
            && let CditorViewState::Ready(session) = &self.state
        {
            selection_changed = self
                .interaction
                .block_drag_selection
                .update(block_id, session);
        }
        if hover_changed || selection_changed {
            cx.notify();
        }
    }
}
