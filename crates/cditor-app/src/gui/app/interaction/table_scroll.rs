use std::collections::HashMap;

use gpui::{Context, Pixels, Point, ScrollHandle, Window, point, px};

use crate::gui::app::cditor_v2_view::CditorV2View;
use crate::gui::app::interaction::table_mode::GuiTableInteractionMode;
use crate::gui::overlay::table::{
    TableViewportMeasurement, table_viewport_measurement_from_handle,
};
use cditor_core::ids::BlockId;

#[derive(Debug, Default)]
pub(in crate::gui::app) struct GuiTableScrollState {
    handles: HashMap<BlockId, ScrollHandle>,
    viewport_measurements: HashMap<BlockId, TableViewportMeasurement>,
}

impl GuiTableScrollState {
    pub(in crate::gui::app) fn clear(&mut self) {
        self.handles.clear();
        self.viewport_measurements.clear();
    }

    pub(in crate::gui::app) fn handle(&mut self, block_id: BlockId, offset_x: f32) -> ScrollHandle {
        let handle = self.handles.entry(block_id).or_default().clone();
        handle.set_offset(point(px(offset_x), handle.offset().y));
        handle
    }

    pub(in crate::gui::app) fn stable_viewport_measurement(
        &mut self,
        block_id: BlockId,
        handle: &ScrollHandle,
    ) -> Option<TableViewportMeasurement> {
        if let Some(measurement) = table_viewport_measurement_from_handle(handle) {
            self.viewport_measurements.insert(block_id, measurement);
            return Some(measurement);
        }
        self.viewport_measurements.get(&block_id).copied()
    }

    pub(in crate::gui::app) fn sync_handle_offset_x(&self, block_id: BlockId, offset_x: f32) {
        if let Some(handle) = self.handles.get(&block_id) {
            handle.set_offset(point(px(offset_x), handle.offset().y));
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TableScrollSnapshot {
    pub handle: ScrollHandle,
    pub viewport_measurement: Option<TableViewportMeasurement>,
    pub offset_x: f32,
}

#[derive(Clone)]
pub(in crate::gui::app) struct GuiTableHScrollDrag {
    pub(in crate::gui::app) block_id: BlockId,
    start_pointer_x: f32,
    start_offset_x: f32,
    max_offset_x: f32,
    thumb_travel_px: f32,
}

impl std::fmt::Debug for GuiTableHScrollDrag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuiTableHScrollDrag")
            .field("block_id", &self.block_id)
            .field("start_pointer_x", &self.start_pointer_x)
            .field("start_offset_x", &self.start_offset_x)
            .field("max_offset_x", &self.max_offset_x)
            .field("thumb_travel_px", &self.thumb_travel_px)
            .finish()
    }
}

impl PartialEq for GuiTableHScrollDrag {
    fn eq(&self, other: &Self) -> bool {
        self.block_id == other.block_id
            && self.start_pointer_x == other.start_pointer_x
            && self.start_offset_x == other.start_offset_x
            && self.max_offset_x == other.max_offset_x
            && self.thumb_travel_px == other.thumb_travel_px
    }
}

impl CditorV2View {
    pub(crate) fn start_table_hscroll_drag_from_gui(
        &mut self,
        block_id: BlockId,
        pointer: Point<Pixels>,
        max_offset_x: f32,
        thumb_travel_px: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if thumb_travel_px <= 0.0 || max_offset_x <= 0.0 {
            return;
        }
        window.focus(&self.focus, cx);
        self.scrollbar_drag = None;
        self.image_resize_drag = None;
        self.table_resize_drag = None;
        self.table_reorder_drag = None;
        self.gutter_block_drag = None;
        self.table_interaction_mode = GuiTableInteractionMode::HScrolling { block_id };
        self.table_hscroll_drag = Some(GuiTableHScrollDrag {
            block_id,
            start_pointer_x: f32::from(pointer.x),
            start_offset_x: self
                .ready_runtime_ref()
                .map(|runtime| runtime.table_horizontal_scroll_offset_px(block_id))
                .unwrap_or(0.0),
            max_offset_x,
            thumb_travel_px,
        });
        cx.notify();
    }

    pub(in crate::gui::app) fn update_table_hscroll_drag(
        &mut self,
        pointer: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.table_hscroll_drag.as_ref() else {
            return false;
        };
        let block_id = drag.block_id;
        let delta_px = f32::from(pointer.x) - drag.start_pointer_x;
        let next_offset_x = table_hscroll_drag_offset(
            drag.start_offset_x,
            delta_px,
            drag.max_offset_x,
            drag.thumb_travel_px,
        );
        let Some(runtime) = self.ready_runtime() else {
            return false;
        };
        let _ = runtime.set_table_horizontal_scroll_offset_px(block_id, next_offset_x);
        self.table_scroll_state
            .sync_handle_offset_x(block_id, next_offset_x);
        cx.notify();
        true
    }

    pub(in crate::gui::app) fn finish_table_hscroll_drag(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.table_hscroll_drag.take() else {
            return false;
        };
        if self.table_interaction_mode
            == (GuiTableInteractionMode::HScrolling {
                block_id: drag.block_id,
            })
        {
            self.table_interaction_mode = GuiTableInteractionMode::Idle;
        }
        cx.notify();
        true
    }
}

pub(super) fn table_hscroll_drag_offset(
    start_offset_x: f32,
    pointer_delta_px: f32,
    max_offset_x: f32,
    thumb_travel_px: f32,
) -> f32 {
    if max_offset_x <= 0.0 || thumb_travel_px <= 0.0 {
        return 0.0;
    }
    let scroll_per_thumb_px = max_offset_x / thumb_travel_px;
    (start_offset_x - pointer_delta_px * scroll_per_thumb_px).clamp(-max_offset_x, 0.0)
}

pub(in crate::gui::app) fn clamped_table_scroll_offset_x(offset_x: f32, max_offset_x: f32) -> f32 {
    if max_offset_x <= 0.0 {
        0.0
    } else {
        offset_x.clamp(-max_offset_x, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_hscroll_drag_maps_thumb_delta_to_negative_scroll_offset() {
        assert_eq!(table_hscroll_drag_offset(0.0, 100.0, 600.0, 300.0), -200.0);
        assert_eq!(table_hscroll_drag_offset(-200.0, -100.0, 600.0, 300.0), 0.0);
        assert_eq!(
            table_hscroll_drag_offset(-500.0, 200.0, 600.0, 300.0),
            -600.0
        );
    }

    #[test]
    fn table_scroll_offset_is_clamped_to_negative_scroll_range() {
        assert_eq!(clamped_table_scroll_offset_x(-200.0, 600.0), -200.0);
        assert_eq!(clamped_table_scroll_offset_x(40.0, 600.0), 0.0);
        assert_eq!(clamped_table_scroll_offset_x(-900.0, 600.0), -600.0);
        assert_eq!(clamped_table_scroll_offset_x(-200.0, 0.0), 0.0);
    }
}
