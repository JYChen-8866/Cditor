use cditor_core::ids::BlockId;
use gpui::Context;

use crate::editor_view::CditorV2View;
use crate::menu_metrics::EditorViewport;
use crate::overlays::{SlashMenuCommand, SlashMenuItem, SlashMenuState};
use crate::persistence::EditorSaveStatus;
use cditor_runtime::AiRequestPresentation;

use cditor_editor_protocol::command::{CditorCommand, CommandOutcomeStatus, CommandSource};

impl CditorV2View {
    pub(crate) fn refresh_text_overlay_anchors(&mut self) {
        let slash_target = self.overlay.slash_menu.as_ref().and_then(|menu| {
            self.ready_session()
                .and_then(|session| session.text_block_context(menu.block_id).ok().flatten())
                .and_then(|context| context.caret)
                .map(|caret| (menu.block_id, caret))
        });
        let prompt_target = self.overlay.ai_prompt.as_ref().and_then(|prompt| {
            self.ready_session()
                .and_then(|session| session.text_block_context(prompt.block_id).ok().flatten())
                .and_then(|context| context.caret)
                .map(|caret| (prompt.block_id, caret))
        });
        let slash_anchor = slash_target
            .and_then(|(block_id, caret)| self.resolved_slash_menu_anchor(block_id, caret));
        let prompt_anchor = prompt_target
            .and_then(|(block_id, caret)| self.resolved_ai_prompt_line_anchor(block_id, caret));

        if let (Some(menu), Some((x, y))) = (self.overlay.slash_menu.as_mut(), slash_anchor) {
            menu.x = x;
            menu.y = y;
        }
        if let (Some(prompt), Some((x, y))) = (self.overlay.ai_prompt.as_mut(), prompt_anchor) {
            prompt.x = gpui::px(x);
            prompt.y = gpui::px(y);
        }
    }

    pub(crate) fn sync_slash_menu_from_runtime(&mut self, cx: &mut Context<Self>) {
        let Some(context) = self
            .ready_session()
            .and_then(|session| session.focused_text_block_context().ok().flatten())
        else {
            self.overlay.slash_menu = None;
            self.clear_slash_popup_menus();
            return;
        };
        let Some(caret) = context.caret else {
            self.overlay.slash_menu = None;
            self.clear_slash_popup_menus();
            return;
        };
        let block_id = context.block_id;
        let text = context.text;
        let Some((trigger_start, query)) = crate::overlays::slash_query_before_caret(&text, caret)
        else {
            self.overlay.slash_menu = None;
            self.clear_slash_popup_menus();
            return;
        };
        let (x, y) = self.slash_menu_anchor(block_id, caret);
        let mut next = SlashMenuState::new(block_id, trigger_start, query, x, y);
        if let Some(previous) = self
            .overlay
            .slash_menu
            .as_ref()
            .filter(|menu| menu.block_id == block_id && menu.trigger_start == trigger_start)
        {
            next.selected_index = previous
                .selected_index
                .min(next.visible_items().len().saturating_sub(1));
            next.scroll_start = previous.scroll_start;
            next.callout_submenu_open = previous.callout_submenu_open
                && next.selected_item().is_some_and(|item| item.is_callout());
        }
        self.overlay.slash_menu = Some(next);
        cx.notify();
    }

    pub(crate) fn apply_slash_menu_index_from_gui(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut menu) = self.overlay.slash_menu.clone() else {
            return false;
        };
        let Some(item) = menu.visible_items().get(index).cloned() else {
            return false;
        };
        menu.selected_index = index;
        self.apply_slash_menu_item(menu, item, cx)
    }

    pub(crate) fn apply_selected_slash_menu_item(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.overlay.slash_menu.clone() else {
            return false;
        };
        let Some(item) = menu.selected_item() else {
            return false;
        };
        self.apply_slash_menu_item(menu, item, cx)
    }

    pub(crate) fn move_slash_menu_selection(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.overlay.slash_menu.as_mut() else {
            return false;
        };
        let changed = menu.move_selection(delta);
        if changed {
            cx.notify();
        }
        changed
    }

    pub(crate) fn scroll_slash_menu_from_gui(
        &mut self,
        delta_rows: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.overlay.slash_menu.as_mut() else {
            return false;
        };
        let changed = menu.scroll(delta_rows);
        if changed {
            cx.notify();
        }
        changed
    }

    pub(crate) fn set_slash_menu_scroll_start_from_gui(
        &mut self,
        scroll_start: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.overlay.slash_menu.as_mut() else {
            return false;
        };
        let changed = menu.set_scroll_start(scroll_start);
        if changed {
            cx.notify();
        }
        changed
    }

    pub(crate) fn select_slash_menu_index_from_gui(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.overlay.slash_menu.as_mut() else {
            return false;
        };
        let items = menu.visible_items();
        if index >= items.len() {
            return false;
        }
        if menu.selected_index == index {
            return false;
        }
        menu.selected_index = index;
        menu.callout_submenu_open = false;
        cx.notify();
        true
    }

    pub(crate) fn cancel_slash_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let had_menu = self.overlay.slash_menu.take().is_some();
        if had_menu {
            self.clear_slash_popup_menus();
            cx.notify();
        }
        had_menu
    }

    fn apply_slash_menu_item(
        &mut self,
        menu: SlashMenuState,
        item: SlashMenuItem,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.status.readonly {
            return false;
        }
        if item.is_callout() {
            let mut menu = menu;
            menu.callout_submenu_open = true;
            self.overlay.slash_menu = Some(menu);
            cx.notify();
            return true;
        }
        if item.command == Some(SlashMenuCommand::AskAi) {
            let command = self.ready_session().and_then(|session| {
                let context = session.text_block_context(menu.block_id).ok().flatten()?;
                let caret = context.caret?;
                Some(CditorCommand::ApplySlashBlock {
                    block_id: menu.block_id,
                    trigger_range: menu.trigger_start..caret,
                    kind: context.kind,
                })
            });
            let changed = command.is_some_and(|command| {
                matches!(
                    self.dispatch_command(command, CommandSource::SlashMenu, cx),
                    Ok(outcome) if outcome.status == CommandOutcomeStatus::Applied
                )
            });
            self.overlay.slash_menu = None;
            self.clear_slash_popup_menus();
            if !changed {
                return false;
            }
            return self.open_ai_prompt_from_gui_with_presentation(
                menu.x,
                menu.y,
                slash_ai_presentation(),
                cx,
            );
        }
        self.apply_slash_menu_kind(menu, item.kind, cx)
    }

    pub(crate) fn apply_slash_callout_variant_from_gui(
        &mut self,
        variant: cditor_core::rich_text::CalloutVariant,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.overlay.slash_menu.clone() else {
            return false;
        };
        self.apply_slash_menu_kind(
            menu,
            cditor_core::rich_text::RichBlockKind::Callout { variant },
            cx,
        )
    }

    fn apply_slash_menu_kind(
        &mut self,
        menu: SlashMenuState,
        kind: cditor_core::rich_text::RichBlockKind,
        cx: &mut Context<Self>,
    ) -> bool {
        #[cfg(feature = "whiteboard")]
        let opens_whiteboard = matches!(kind, cditor_core::rich_text::RichBlockKind::Whiteboard);
        let caret = self
            .ready_session()
            .and_then(|session| {
                session
                    .text_block_context(menu.block_id)
                    .ok()
                    .flatten()?
                    .caret
            })
            .unwrap_or(menu.trigger_start);
        let result = self.dispatch_command(
            CditorCommand::ApplySlashBlock {
                block_id: menu.block_id,
                trigger_range: menu.trigger_start..caret,
                kind,
            },
            CommandSource::SlashMenu,
            cx,
        );
        match result {
            Ok(outcome) => {
                self.overlay.slash_menu = None;
                self.clear_slash_popup_menus();
                #[cfg(feature = "whiteboard")]
                if outcome.status == CommandOutcomeStatus::Applied && opens_whiteboard {
                    self.open_whiteboard_editor_from_gui(menu.block_id, cx);
                }
                cx.notify();
                outcome.status == CommandOutcomeStatus::Applied
            }
            Err(error) => {
                self.status.save_status = EditorSaveStatus::Failed(error.to_string());
                self.overlay.slash_menu = None;
                self.clear_slash_popup_menus();
                cx.notify();
                false
            }
        }
    }

    fn clear_slash_popup_menus(&mut self) {
        self.overlay.slash_popup_menu = None;
        self.overlay.slash_popup_menu_dismiss_subscription = None;
        self.overlay.slash_callout_popup_menu = None;
        self.overlay.slash_callout_popup_menu_dismiss_subscription = None;
    }

    pub(super) fn slash_menu_anchor(&self, block_id: BlockId, caret: usize) -> (f32, f32) {
        self.resolved_slash_menu_anchor(block_id, caret)
            .unwrap_or((120.0, 120.0))
    }

    pub(super) fn ai_prompt_line_anchor(&self, block_id: BlockId, caret: usize) -> (f32, f32) {
        self.resolved_ai_prompt_line_anchor(block_id, caret)
            .unwrap_or((120.0, 120.0))
    }

    fn resolved_slash_menu_anchor(&self, block_id: BlockId, caret: usize) -> Option<(f32, f32)> {
        let anchor = slash_menu_window_anchor(self.text_caret_bounds_for_block(block_id, caret)?);
        Some(self.window_anchor_to_editor_local(anchor.0, anchor.1))
    }

    fn resolved_ai_prompt_line_anchor(
        &self,
        block_id: BlockId,
        caret: usize,
    ) -> Option<(f32, f32)> {
        let anchor = ai_prompt_window_anchor(self.text_caret_bounds_for_block(block_id, caret)?);
        Some(self.window_anchor_to_editor_local(anchor.0, anchor.1))
    }

    fn window_anchor_to_editor_local(&self, x: f32, y: f32) -> (f32, f32) {
        let bounds = self.interaction.editor_viewport_handle.bounds();
        EditorViewport::from_measurement(bounds, bounds.size).window_point_to_local(x, y)
    }
}

fn slash_ai_presentation() -> AiRequestPresentation {
    AiRequestPresentation::AssistantPanel
}

fn slash_menu_window_anchor(bounds: gpui::Bounds<gpui::Pixels>) -> (f32, f32) {
    (f32::from(bounds.left()), f32::from(bounds.bottom()) + 4.0)
}

fn ai_prompt_window_anchor(bounds: gpui::Bounds<gpui::Pixels>) -> (f32, f32) {
    (f32::from(bounds.left()), f32::from(bounds.top()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_ask_ai_always_uses_a_visible_assistant_panel() {
        assert_eq!(
            slash_ai_presentation(),
            AiRequestPresentation::AssistantPanel
        );
    }

    #[test]
    fn slash_menu_opens_below_the_projected_caret() {
        let bounds = gpui::Bounds::new(
            gpui::point(gpui::px(142.0), gpui::px(154.0)),
            gpui::size(gpui::px(1.0), gpui::px(20.0)),
        );
        assert_eq!(slash_menu_window_anchor(bounds), (142.0, 178.0),);
    }

    #[test]
    fn ai_prompt_uses_the_projected_caret_line_top() {
        let bounds = gpui::Bounds::new(
            gpui::point(gpui::px(142.0), gpui::px(326.0)),
            gpui::size(gpui::px(1.0), gpui::px(20.0)),
        );
        assert_eq!(ai_prompt_window_anchor(bounds), (142.0, 326.0),);
    }
}
