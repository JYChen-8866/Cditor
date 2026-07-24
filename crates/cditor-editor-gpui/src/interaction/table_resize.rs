use gpui::{Context, Pixels, Point, Window};

use crate::block::table::TableAxis;
use crate::editor_view::{CditorV2View, CditorViewState};
use crate::input::BlockDragSelectionController;
use crate::interaction::table_mode::GuiTableInteractionMode;
use crate::persistence::EditorSaveStatus;
use cditor_core::ids::BlockId;
use cditor_editor_protocol::command::{
    CommandSource, EditorCommand, TableAxis as CommandTableAxis,
};

const TABLE_RESIZE_MIN_SIZE_PX: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GuiTableResizeDrag {
    pub(crate) block_id: BlockId,
    pub(crate) axis: TableAxis,
    pub(crate) index: usize,
    start_pointer: f32,
    start_size_px: f32,
    pub(crate) current_size_px: f32,
}

impl CditorV2View {
    #[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
    pub(crate) fn start_table_resize_from_gui(
        &mut self,
        block_id: BlockId,
        axis: TableAxis,
        index: usize,
        current_size_px: f32,
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
        self.interaction.image_resize_drag = None;
        self.interaction.table_hscroll_drag = None;
        self.interaction.table_interaction_mode = GuiTableInteractionMode::Resizing {
            block_id,
            axis,
            index,
        };
        self.interaction.hovered_block_id = Some(block_id);
        self.interaction.action_block_id = Some(block_id);
        self.interaction.table_resize_drag = Some(GuiTableResizeDrag {
            block_id,
            axis,
            index,
            start_pointer: table_resize_pointer(axis, position),
            start_size_px: current_size_px.max(TABLE_RESIZE_MIN_SIZE_PX),
            current_size_px: current_size_px.max(TABLE_RESIZE_MIN_SIZE_PX),
        });
        if let CditorViewState::Ready(session) = &self.state {
            let _ = session.dispatch_with_snapshot(
                cditor_editor_protocol::command::CommandEnvelope::new(
                    cditor_editor_protocol::command::CditorCommand::FocusBlock { block_id },
                    cditor_editor_protocol::command::CommandSource::Toolbar,
                ),
            );
        }
        cx.notify();
    }

    pub(crate) fn table_resize_preview(&self) -> Option<(BlockId, TableAxis, usize, f32)> {
        self.interaction
            .table_resize_drag
            .map(|drag| (drag.block_id, drag.axis, drag.index, drag.current_size_px))
    }

    pub(crate) fn update_table_resize_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.interaction.table_resize_drag else {
            return false;
        };
        let next_size =
            table_resize_preview_size(drag.axis, drag.start_pointer, drag.start_size_px, position);
        if (next_size - drag.current_size_px).abs() < 0.5 {
            return true;
        }
        drag.current_size_px = next_size;
        self.interaction.table_resize_drag = Some(drag);
        // Preview remains UI-transient. Mouse-up commits exactly one Runtime
        // transaction; rendering consumes `table_resize_preview` meanwhile.
        cx.notify();
        true
    }

    pub(crate) fn commit_table_resize_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.interaction.table_resize_drag.take() else {
            return false;
        };
        clear_committed_table_resize_action(&mut self.interaction.action_block_id, drag.block_id);
        if matches!(self.interaction.table_interaction_mode, GuiTableInteractionMode::Resizing { block_id, axis, index } if block_id == drag.block_id && axis == drag.axis && index == drag.index)
        {
            self.interaction.table_interaction_mode = GuiTableInteractionMode::Idle;
        }
        let size_px = drag.current_size_px.round().clamp(1.0, u16::MAX as f32) as u16;
        let axis = match drag.axis {
            TableAxis::Row => CommandTableAxis::Row,
            TableAxis::Column => CommandTableAxis::Column,
        };
        if let Err(error) = self.dispatch_command(
            EditorCommand::TableResizeAxis {
                block_id: drag.block_id,
                axis,
                index: drag.index,
                size_px,
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

fn table_resize_pointer(axis: TableAxis, position: Point<Pixels>) -> f32 {
    match axis {
        TableAxis::Row => f32::from(position.y),
        TableAxis::Column => f32::from(position.x),
    }
}

fn table_resize_preview_size(
    axis: TableAxis,
    start_pointer: f32,
    start_size_px: f32,
    position: Point<Pixels>,
) -> f32 {
    let delta = table_resize_pointer(axis, position) - start_pointer;
    (start_size_px + delta).max(TABLE_RESIZE_MIN_SIZE_PX)
}

fn clear_committed_table_resize_action(action_block_id: &mut Option<BlockId>, block_id: BlockId) {
    if *action_block_id == Some(block_id) {
        *action_block_id = None;
    }
}

#[cfg(test)]
mod tests {
    use gpui::{point, px};

    use super::*;

    #[test]
    fn table_resize_pointer_uses_axis_direction() {
        let position = point(px(80.0), px(140.0));

        assert_eq!(table_resize_pointer(TableAxis::Column, position), 80.0);
        assert_eq!(table_resize_pointer(TableAxis::Row, position), 140.0);
    }

    #[test]
    fn committing_table_resize_clears_matching_action_root() {
        let mut action_block_id = Some(7);

        clear_committed_table_resize_action(&mut action_block_id, 7);

        assert_eq!(action_block_id, None);
    }

    #[test]
    fn table_resize_preview_size_clamps_without_runtime_commit() {
        assert_eq!(
            table_resize_preview_size(TableAxis::Column, 100.0, 120.0, point(px(160.0), px(0.0))),
            180.0
        );
        assert_eq!(
            table_resize_preview_size(TableAxis::Row, 100.0, 36.0, point(px(0.0), px(40.0))),
            TABLE_RESIZE_MIN_SIZE_PX
        );
    }
}
