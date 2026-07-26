use std::collections::HashMap;

use gpui::{Context, ScrollHandle, point, px};

use crate::editor_view::CditorV2View;
use crate::overlays::table::{TableViewportMeasurement, table_viewport_measurement_from_handle};
use cditor_core::ids::BlockId;

#[derive(Debug, Default)]
pub(crate) struct GuiTableScrollState {
    handles: HashMap<BlockId, ScrollHandle>,
    viewport_measurements: HashMap<BlockId, TableViewportMeasurement>,
}

impl GuiTableScrollState {
    pub(crate) fn handle(&mut self, block_id: BlockId, offset_x: f32) -> ScrollHandle {
        let handle = self.handles.entry(block_id).or_default().clone();
        handle.set_offset(point(px(offset_x), handle.offset().y));
        handle
    }

    pub(crate) fn stable_viewport_measurement(
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

    pub(crate) fn sync_handle_offset_x(&self, block_id: BlockId, offset_x: f32) {
        if let Some(handle) = self.handles.get(&block_id) {
            handle.set_offset(point(px(offset_x), handle.offset().y));
        }
    }

    pub(crate) fn live_offset(&self, block_id: BlockId) -> Option<(f64, f64)> {
        let offset = self.handles.get(&block_id)?.offset();
        Some((f64::from(offset.x), f64::from(offset.y)))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TableScrollSnapshot {
    pub handle: ScrollHandle,
    pub viewport_measurement: Option<TableViewportMeasurement>,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl CditorV2View {
    pub(crate) fn set_table_hscroll_offset_from_component(
        &mut self,
        block_id: BlockId,
        offset_px: f32,
        cx: &mut Context<Self>,
    ) {
        let next_offset_x = -offset_px.max(0.0);
        let Some(session) = self.ready_session() else {
            return;
        };
        let _ = session.set_table_horizontal_scroll_offset(block_id, next_offset_x);
        self.interaction
            .table_scroll_state
            .sync_handle_offset_x(block_id, next_offset_x);
        cx.notify();
    }
}

pub(crate) fn clamped_table_scroll_offset_x(offset_x: f32, max_offset_x: f32) -> f32 {
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
    fn table_scroll_offset_is_clamped_to_negative_scroll_range() {
        assert_eq!(clamped_table_scroll_offset_x(-200.0, 600.0), -200.0);
        assert_eq!(clamped_table_scroll_offset_x(40.0, 600.0), 0.0);
        assert_eq!(clamped_table_scroll_offset_x(-900.0, 600.0), -600.0);
        assert_eq!(clamped_table_scroll_offset_x(-200.0, 0.0), 0.0);
    }
}
