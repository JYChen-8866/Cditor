use gpui::{Context, Pixels, Point, Window};

use crate::editor_view::{CditorV2View, CditorViewState};
use crate::features::media::image_width_ratio_milli_for_width;
use crate::input::BlockDragSelectionController;
use crate::persistence::EditorSaveStatus;
use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{CommandSource, EditorCommand};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GuiImageResizeDrag {
    pub(crate) block_id: BlockId,
    start_pointer_x: f32,
    start_width_px: f32,
    pub(crate) current_width_px: f32,
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
        if self.status.readonly {
            return;
        }
        window.focus(&self.focus.editor, cx);
        self.interaction.text_drag_selection = None;
        self.interaction.block_drag_selection = BlockDragSelectionController::default();
        self.clear_gutter_action();
        self.interaction.scrollbar_drag = None;
        self.interaction.hovered_block_id = Some(block_id);
        self.interaction.action_block_id = Some(block_id);
        self.interaction.image_resize_drag = Some(GuiImageResizeDrag {
            block_id,
            start_pointer_x: f32::from(position.x),
            start_width_px: current_width_px,
            current_width_px: current_width_px.clamp(max_width_px * 0.2, max_width_px),
            max_width_px,
        });
        if let CditorViewState::Ready(session) = &self.state {
            let _ = session.dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                cditor_editor_protocol::command::CditorCommand::FocusBlock { block_id },
                cditor_editor_protocol::command::CommandSource::Toolbar,
            ));
        }
        cx.notify();
    }

    pub(crate) fn image_resize_preview(&self) -> Option<(BlockId, f32)> {
        self.interaction
            .image_resize_drag
            .map(|drag| (drag.block_id, drag.current_width_px))
    }

    pub(crate) fn update_image_resize_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.interaction.image_resize_drag else {
            return false;
        };
        let dx = f32::from(position.x) - drag.start_pointer_x;
        let next_width =
            (drag.start_width_px + dx).clamp(drag.max_width_px * 0.2, drag.max_width_px);
        if (next_width - drag.current_width_px).abs() < 0.5 {
            return true;
        }
        drag.current_width_px = next_width;
        self.interaction.image_resize_drag = Some(drag);
        cx.notify();
        true
    }

    pub(crate) fn commit_image_resize_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = super::take_drag(&mut self.interaction.image_resize_drag) else {
            return false;
        };
        clear_committed_image_resize_action(&mut self.interaction.action_block_id, drag.block_id);
        let ratio = image_width_ratio_milli_for_width(drag.current_width_px, drag.max_width_px);
        if let Err(error) = self.dispatch_command(
            EditorCommand::SetMediaWidthRatio {
                block_id: drag.block_id,
                ratio_milli: ratio,
            },
            CommandSource::Toolbar,
            cx,
        ) {
            self.status.save_status = EditorSaveStatus::Failed(error.to_string());
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
