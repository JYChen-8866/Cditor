use gpui::{Context, Pixels, Point, Window};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::block::table::TableAxis;
use crate::gui::input::BlockDragSelectionController;
use crate::gui::persistence::EditorSaveStatus;
use cditor_core::ids::BlockId;

const TABLE_REORDER_MIN_DRAG_DELTA_PX: f32 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::gui::app) struct GuiTableReorderDrag {
    pub(in crate::gui::app) block_id: BlockId,
    pub(in crate::gui::app) axis: TableAxis,
    pub(in crate::gui::app) from_index: usize,
    pub(in crate::gui::app) target_index: usize,
    start_pointer: f32,
    track_sizes_px: Vec<f32>,
    exceeded_threshold: bool,
}

impl CditorV2View {
    pub(crate) fn start_table_reorder_from_gui(
        &mut self,
        block_id: BlockId,
        axis: TableAxis,
        index: usize,
        track_sizes_px: Vec<f32>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly || track_sizes_px.get(index).is_none() {
            return;
        }
        window.focus(&self.focus, cx);
        self.text_drag_selection = None;
        self.block_drag_selection = BlockDragSelectionController::default();
        self.clear_gutter_action();
        self.scrollbar_drag = None;
        self.image_resize_drag = None;
        self.table_resize_drag = None;
        self.hovered_block_id = Some(block_id);
        self.action_block_id = Some(block_id);
        self.table_reorder_drag = Some(GuiTableReorderDrag {
            block_id,
            axis,
            from_index: index,
            target_index: index,
            start_pointer: table_reorder_pointer(axis, position),
            track_sizes_px,
            exceeded_threshold: false,
        });
        if let CditorViewState::Ready(runtime) = &mut self.state {
            runtime.focus_block(block_id);
        }
        cx.notify();
    }

    pub(in crate::gui::app) fn table_reorder_preview(
        &self,
    ) -> Option<(BlockId, TableAxis, usize, usize)> {
        let drag = self.table_reorder_drag.as_ref()?;
        drag.exceeded_threshold.then_some((
            drag.block_id,
            drag.axis,
            drag.from_index,
            drag.target_index,
        ))
    }

    pub(in crate::gui::app) fn update_table_reorder_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.table_reorder_drag.take() else {
            return false;
        };
        let pointer = table_reorder_pointer(drag.axis, position);
        let delta = pointer - drag.start_pointer;
        drag.exceeded_threshold |= delta.abs() >= TABLE_REORDER_MIN_DRAG_DELTA_PX;
        drag.target_index =
            table_reorder_target_index(drag.from_index, delta, &drag.track_sizes_px);
        self.table_reorder_drag = Some(drag);
        cx.notify();
        true
    }

    pub(in crate::gui::app) fn commit_table_reorder_drag(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.table_reorder_drag.take() else {
            return false;
        };
        self.action_block_id = self
            .action_block_id
            .filter(|action_block_id| *action_block_id != drag.block_id);
        if !drag.exceeded_threshold || drag.from_index == drag.target_index {
            cx.notify();
            return true;
        }
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let result = match drag.axis {
                TableAxis::Row => {
                    runtime.move_table_row(drag.block_id, drag.from_index, drag.target_index)
                }
                TableAxis::Column => {
                    runtime.move_table_column(drag.block_id, drag.from_index, drag.target_index)
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

fn table_reorder_pointer(axis: TableAxis, position: Point<Pixels>) -> f32 {
    match axis {
        TableAxis::Row => f32::from(position.y),
        TableAxis::Column => f32::from(position.x),
    }
}

fn table_reorder_target_index(from_index: usize, delta_px: f32, track_sizes_px: &[f32]) -> usize {
    let Some(from_size) = track_sizes_px.get(from_index).copied() else {
        return from_index;
    };
    let moving_center_px =
        track_sizes_px.iter().take(from_index).sum::<f32>() + from_size / 2.0 + delta_px;
    let mut target = 0;
    for (index, size) in track_sizes_px.iter().copied().enumerate() {
        let center = track_sizes_px.iter().take(index).sum::<f32>() + size / 2.0;
        if moving_center_px >= center {
            target = index;
        }
    }
    target.min(track_sizes_px.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use gpui::{point, px};

    use super::*;

    #[test]
    fn table_reorder_pointer_uses_axis_direction() {
        let position = point(px(40.0), px(96.0));

        assert_eq!(table_reorder_pointer(TableAxis::Column, position), 40.0);
        assert_eq!(table_reorder_pointer(TableAxis::Row, position), 96.0);
    }

    #[test]
    fn table_reorder_target_uses_track_centers() {
        let sizes = [80.0, 120.0, 100.0, 140.0];

        assert_eq!(table_reorder_target_index(1, -100.0, &sizes), 0);
        assert_eq!(table_reorder_target_index(1, 0.0, &sizes), 1);
        assert_eq!(table_reorder_target_index(1, 130.0, &sizes), 2);
        assert_eq!(table_reorder_target_index(1, 260.0, &sizes), 3);
    }

    #[test]
    fn table_reorder_target_keeps_invalid_origin_stable() {
        assert_eq!(table_reorder_target_index(4, 100.0, &[80.0, 80.0]), 4);
    }
}
