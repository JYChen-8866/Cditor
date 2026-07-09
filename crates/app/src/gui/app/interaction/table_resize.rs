use gpui::{Context, Pixels, Point, Window};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::block::table::TableAxis;
use crate::gui::input::BlockDragSelectionController;
use crate::gui::persistence::EditorSaveStatus;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::TableTrackSize;

const TABLE_RESIZE_MIN_SIZE_PX: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::gui::app) struct GuiTableResizeDrag {
    pub(in crate::gui::app) block_id: BlockId,
    pub(in crate::gui::app) axis: TableAxis,
    pub(in crate::gui::app) index: usize,
    start_pointer: f32,
    start_size_px: f32,
    pub(in crate::gui::app) current_size_px: f32,
}

impl CditorV2View {
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
        if self.readonly {
            return;
        }
        window.focus(&self.focus, cx);
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.clear_gutter_action();
        self.scrollbar_drag = None;
        self.image_resize_drag = None;
        self.hovered_block_id = Some(block_id);
        self.action_block_id = Some(block_id);
        self.table_resize_drag = Some(GuiTableResizeDrag {
            block_id,
            axis,
            index,
            start_pointer: table_resize_pointer(axis, position),
            start_size_px: current_size_px.max(TABLE_RESIZE_MIN_SIZE_PX),
            current_size_px: current_size_px.max(TABLE_RESIZE_MIN_SIZE_PX),
        });
        if let CditorViewState::Ready(runtime) = &mut self.state {
            runtime.focus_block(block_id);
        }
        cx.notify();
    }

    pub(in crate::gui::app) fn table_resize_preview(
        &self,
    ) -> Option<(BlockId, TableAxis, usize, f32)> {
        self.table_resize_drag
            .map(|drag| (drag.block_id, drag.axis, drag.index, drag.current_size_px))
    }

    pub(in crate::gui::app) fn update_table_resize_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.table_resize_drag else {
            return false;
        };
        let next_size =
            table_resize_preview_size(drag.axis, drag.start_pointer, drag.start_size_px, position);
        if (next_size - drag.current_size_px).abs() < 0.5 {
            return true;
        }
        drag.current_size_px = next_size;
        self.table_resize_drag = Some(drag);
        cx.notify();
        true
    }

    pub(in crate::gui::app) fn commit_table_resize_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.table_resize_drag.take() else {
            return false;
        };
        clear_committed_table_resize_action(&mut self.action_block_id, drag.block_id);
        let size =
            TableTrackSize::Px(drag.current_size_px.round().clamp(1.0, u16::MAX as f32) as u16);
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let result = match drag.axis {
                TableAxis::Row => runtime.set_table_row_height(drag.block_id, drag.index, size),
                TableAxis::Column => {
                    runtime.set_table_column_width(drag.block_id, drag.index, size)
                }
            };
            match result {
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
