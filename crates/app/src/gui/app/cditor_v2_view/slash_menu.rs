use cditor_core::ids::BlockId;
use gpui::Context;

use crate::gui::app::cditor_v2_view::CditorV2View;
use crate::gui::app::interaction::geometry::{FallbackViewportOrigin, ProjectedBlockRect};
use crate::gui::overlay::{SlashMenuCommand, SlashMenuItem, SlashMenuState};
use crate::gui::persistence::EditorSaveStatus;
use crate::gui::text::platform_range_bounds;

impl CditorV2View {
    pub(crate) fn sync_slash_menu_from_runtime(&mut self, cx: &mut Context<Self>) {
        let Some((block_id, text, caret)) = self.ready_runtime_ref().and_then(|runtime| {
            let block_id = runtime.focused_block_id()?;
            let text = runtime.focused_text()?.to_owned();
            let caret = runtime.caret_offset_for_block(block_id)?;
            Some((block_id, text, caret))
        }) else {
            self.slash_menu = None;
            return;
        };
        let Some((trigger_start, query)) =
            crate::gui::overlay::slash_query_before_caret(&text, caret)
        else {
            self.slash_menu = None;
            return;
        };
        let (x, y) = self.slash_menu_anchor(block_id, caret);
        let mut next = SlashMenuState::new(block_id, trigger_start, query, x, y);
        if let Some(previous) = self
            .slash_menu
            .as_ref()
            .filter(|menu| menu.block_id == block_id && menu.trigger_start == trigger_start)
        {
            next.selected_index = previous
                .selected_index
                .min(next.visible_items().len().saturating_sub(1));
            next.scroll_start = previous.scroll_start;
        }
        self.slash_menu = Some(next);
        cx.notify();
    }

    pub(crate) fn apply_slash_menu_index_from_gui(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut menu) = self.slash_menu.clone() else {
            return false;
        };
        let Some(item) = menu.visible_items().get(index).cloned() else {
            return false;
        };
        menu.selected_index = index;
        self.apply_slash_menu_item(menu, item, cx)
    }

    pub(in crate::gui::app) fn apply_selected_slash_menu_item(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.slash_menu.clone() else {
            return false;
        };
        let Some(item) = menu.selected_item() else {
            return false;
        };
        self.apply_slash_menu_item(menu, item, cx)
    }

    pub(in crate::gui::app) fn move_slash_menu_selection(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.slash_menu.as_mut() else {
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
        let Some(menu) = self.slash_menu.as_mut() else {
            return false;
        };
        let changed = menu.scroll(delta_rows);
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
        let Some(menu) = self.slash_menu.as_mut() else {
            return false;
        };
        if index >= menu.visible_items().len() || menu.selected_index == index {
            return false;
        }
        menu.selected_index = index;
        cx.notify();
        true
    }

    pub(crate) fn cancel_slash_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let had_menu = self.slash_menu.take().is_some();
        if had_menu {
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
        if self.readonly {
            return false;
        }
        if item.command == Some(SlashMenuCommand::AskAi) {
            let changed = self
                .ready_runtime()
                .and_then(|runtime| {
                    let caret = runtime.caret_offset_for_block(menu.block_id)?;
                    runtime
                        .replace_text_in_focused_range(Some(menu.trigger_start..caret), "")
                        .ok()
                })
                .unwrap_or(false);
            self.slash_menu = None;
            if changed {
                self.mark_dirty(cx);
            }
            return self.open_ai_prompt_from_gui(menu.x, menu.y, cx);
        }
        let kind = item.kind;
        let opens_whiteboard = matches!(kind, cditor_core::rich_text::RichBlockKind::Whiteboard);
        let result: Result<(bool, bool), String> = (|| {
            let runtime = self
                .ready_runtime()
                .ok_or_else(|| "runtime is not ready".to_owned())?;
            if runtime.focused_block_id() != Some(menu.block_id) {
                return Ok((false, false));
            }
            let caret = runtime
                .caret_offset_for_block(menu.block_id)
                .unwrap_or(menu.trigger_start);
            let deleted_trigger =
                runtime.replace_text_in_focused_range(Some(menu.trigger_start..caret), "")?;
            let converted = runtime.convert_focused_block_kind(kind)?;
            Ok((deleted_trigger || converted, converted))
        })();
        match result {
            Ok((changed, converted)) => {
                self.slash_menu = None;
                if changed {
                    self.mark_dirty(cx);
                }
                if converted && opens_whiteboard {
                    self.open_whiteboard_editor_from_gui(menu.block_id, cx);
                }
                cx.notify();
                true
            }
            Err(error) => {
                self.save_status = EditorSaveStatus::Failed(error);
                self.slash_menu = None;
                cx.notify();
                false
            }
        }
    }

    pub(super) fn slash_menu_anchor(&self, block_id: BlockId, caret: usize) -> (f32, f32) {
        if let Some(cache) = self.text_layouts.get(&block_id)
            && let Some(bounds) = platform_range_bounds(cache, caret..caret)
        {
            return (f32::from(bounds.left()), f32::from(bounds.bottom()) + 4.0);
        }
        let Some(viewport_origin) = self.infer_document_viewport_origin() else {
            return (120.0, 120.0);
        };
        let Some(scroll_top) = self
            .ready_runtime_ref()
            .map(|runtime| runtime.scroll.global_scroll_top)
        else {
            return (120.0, 120.0);
        };
        self.projected_block_rects
            .iter()
            .find(|rect| rect.block_id == block_id)
            .map(|rect| slash_menu_fallback_anchor(rect, viewport_origin, scroll_top))
            .unwrap_or((120.0, 120.0))
    }
}

fn slash_menu_fallback_anchor(
    rect: &ProjectedBlockRect,
    viewport_origin: FallbackViewportOrigin,
    scroll_top: f64,
) -> (f32, f32) {
    (
        (viewport_origin.x + rect.text_origin_x_in_block_px) as f32,
        (viewport_origin.y + rect.document_top - scroll_top + rect.text_origin_y_in_block_px + 24.0)
            as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_anchor_projects_document_coordinates_into_viewport() {
        let rect = ProjectedBlockRect {
            block_id: 1,
            visible_index: 0,
            depth: 0,
            document_top: 620.0,
            document_bottom: 652.0,
            indent_px: 0.0,
            text_origin_x_in_block_px: 42.0,
            text_origin_y_in_block_px: 4.0,
            text_width_px: 720.0,
            supports_children: false,
        };

        assert_eq!(
            slash_menu_fallback_anchor(&rect, FallbackViewportOrigin { x: 100.0, y: 30.0 }, 500.0,),
            (142.0, 178.0),
        );
    }
}
