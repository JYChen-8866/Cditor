use gpui::{Context, Pixels, Point, Window};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::block::media::image_width_ratio_milli_for_width;
use crate::gui::input::BlockDragSelectionController;
use crate::gui::persistence::EditorSaveStatus;
use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::gui::app) struct GuiImageResizeDrag {
    pub(in crate::gui::app) block_id: BlockId,
    start_pointer_x: f32,
    start_width_px: f32,
    pub(in crate::gui::app) current_width_px: f32,
    max_width_px: f32,
}

impl CditorV2View {
    pub(crate) fn start_image_resize_from_gui(
        &mut self,
        block_id: BlockId,
        current_width_px: f32,
        max_width_px: f32,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        window.focus(&self.focus, cx);
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.clear_gutter_action();
        self.scrollbar_drag = None;
        self.hovered_block_id = Some(block_id);
        self.action_block_id = Some(block_id);
        self.image_resize_drag = Some(GuiImageResizeDrag {
            block_id,
            start_pointer_x: f32::from(position.x),
            start_width_px: current_width_px,
            current_width_px: current_width_px.clamp(max_width_px * 0.2, max_width_px),
            max_width_px,
        });
        if let CditorViewState::Ready(runtime) = &mut self.state {
            runtime.focus_block(block_id);
        }
        cx.notify();
    }

    pub(in crate::gui::app) fn image_resize_preview(&self) -> Option<(BlockId, f32)> {
        self.image_resize_drag
            .map(|drag| (drag.block_id, drag.current_width_px))
    }

    pub(in crate::gui::app) fn update_image_resize_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.image_resize_drag else {
            return false;
        };
        let dx = f32::from(position.x) - drag.start_pointer_x;
        let next_width =
            (drag.start_width_px + dx).clamp(drag.max_width_px * 0.2, drag.max_width_px);
        if (next_width - drag.current_width_px).abs() < 0.5 {
            return true;
        }
        drag.current_width_px = next_width;
        self.image_resize_drag = Some(drag);
        cx.notify();
        true
    }

    pub(in crate::gui::app) fn commit_image_resize_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.image_resize_drag.take() else {
            return false;
        };
        clear_committed_image_resize_action(&mut self.action_block_id, drag.block_id);
        let ratio = image_width_ratio_milli_for_width(drag.current_width_px, drag.max_width_px);
        if let CditorViewState::Ready(runtime) = &mut self.state {
            match runtime.update_image_display_width_ratio(drag.block_id, ratio) {
                Ok(true) => self.mark_dirty(cx),
                Ok(false) => {}
                Err(error) => {
                    self.save_status = EditorSaveStatus::Failed(error);
                }
            }
        }
        cx.notify();
        true
    }
}

fn clear_committed_image_resize_action(
    action_block_id: &mut Option<BlockId>,
    image_block_id: BlockId,
) {
    if *action_block_id == Some(image_block_id) {
        *action_block_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committing_image_resize_clears_matching_action_root() {
        let mut action_block_id = Some(7);

        clear_committed_image_resize_action(&mut action_block_id, 7);

        assert_eq!(action_block_id, None);
    }

    #[test]
    fn committing_image_resize_preserves_newer_action_root() {
        let mut action_block_id = Some(8);

        clear_committed_image_resize_action(&mut action_block_id, 7);

        assert_eq!(action_block_id, Some(8));
    }
}
