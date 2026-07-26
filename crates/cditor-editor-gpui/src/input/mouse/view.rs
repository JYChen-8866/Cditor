use gpui::{Context, MouseMoveEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent, Window};

use crate::editor_view::{CditorV2View, CditorViewState};
use crate::scroll::{ScrollDeltaMode, ScrollDevice, ScrollInput, ScrollPhase};

impl CditorV2View {
    pub(crate) fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pause_caret_blink(cx);
        self.interaction.last_wheel_delta_y = scroll_delta_y(event);
        if let CditorViewState::Ready(session) = &self.state {
            let _ = session.apply_scroll_input_frame(
                &mut self.interaction.scroll_accumulator,
                ScrollInput {
                    delta_y: self.interaction.last_wheel_delta_y,
                    mode: ScrollDeltaMode::Pixel,
                    phase: scroll_phase_from_touch(event.touch_phase),
                    device: ScrollDevice::Trackpad,
                    timestamp: std::time::Instant::now(),
                },
            );
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn on_scrollbar_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.dragging() && self.interaction.image_resize_drag.is_some() {
            if self.update_image_resize_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if event.dragging() && self.interaction.table_resize_drag.is_some() {
            if self.update_table_resize_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if event.dragging() && self.interaction.table_reorder_drag.is_some() {
            if self.update_table_reorder_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if event.dragging() && self.interaction.gutter_block_drag.is_some() {
            if self.update_gutter_block_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if self.interaction.scrollbar_drag.is_some() {
            return;
        }
        if event.dragging() {
            if !self.interaction.block_drag_selection.is_dragging() {
                self.update_text_drag_selection(event.position, cx);
            }
        } else {
            self.finish_text_drag_selection();
            self.finish_block_drag_selection();
        }
    }

    pub(crate) fn on_scrollbar_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.commit_image_resize_drag(cx) {
            cx.stop_propagation();
        }
        if self.commit_table_resize_drag(cx) {
            cx.stop_propagation();
        }
        if self.commit_table_reorder_drag(cx) {
            cx.stop_propagation();
        }
        if self.commit_gutter_block_drag(cx) {
            cx.stop_propagation();
        }
        self.finish_table_cell_text_selection_drag();
        self.finish_text_drag_selection();
        self.finish_block_drag_selection();
    }
}

pub(crate) fn scroll_delta_y(event: &ScrollWheelEvent) -> f64 {
    match event.delta {
        ScrollDelta::Pixels(delta) => -(f32::from(delta.y) as f64),
        ScrollDelta::Lines(delta) => -(delta.y as f64 * 16.0),
    }
}

fn scroll_phase_from_touch(phase: gpui::TouchPhase) -> ScrollPhase {
    match phase {
        gpui::TouchPhase::Started => ScrollPhase::Began,
        gpui::TouchPhase::Moved => ScrollPhase::Changed,
        gpui::TouchPhase::Ended => ScrollPhase::Ended,
    }
}
