use gpui::{Context, MouseMoveEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent, Window};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::app::interaction::scrollbar::scrollbar_policy;
use cditor_editor::scroll::{ScrollDeltaMode, ScrollDevice, ScrollInput, ScrollPhase};

impl CditorV2View {
    pub(in crate::gui::app) fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.last_wheel_delta_y = scroll_delta_y(event);
        if let CditorViewState::Ready(runtime) = &mut self.state {
            self.scroll_accumulator.push_input(
                ScrollInput {
                    delta_y: self.last_wheel_delta_y,
                    mode: ScrollDeltaMode::Pixel,
                    phase: scroll_phase_from_touch(event.touch_phase),
                    device: ScrollDevice::Trackpad,
                    timestamp: std::time::Instant::now(),
                },
                runtime.scroll.viewport_height,
            );
            let _ = self.scroll_accumulator.apply_frame(&mut runtime.scroll);
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(in crate::gui::app) fn on_scrollbar_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.dragging() && self.image_resize_drag.is_some() {
            if self.update_image_resize_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if event.dragging() && self.table_resize_drag.is_some() {
            if self.update_table_resize_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if event.dragging() && self.table_reorder_drag.is_some() {
            if self.update_table_reorder_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if event.dragging() && self.table_hscroll_drag.is_some() {
            if self.update_table_hscroll_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if event.dragging() && self.gutter_block_drag.is_some() {
            if self.update_gutter_block_drag(event.position, cx) {
                cx.stop_propagation();
            }
            return;
        }
        let Some(drag) = self.scrollbar_drag else {
            if event.dragging() {
                if !self.block_drag_selection.is_dragging() {
                    self.update_text_drag_selection(event.position, cx);
                }
            } else {
                self.finish_text_drag_selection();
                self.finish_block_drag_selection();
            }
            return;
        };
        if !event.dragging() {
            self.finish_gui_scrollbar_drag(cx);
            self.finish_text_drag_selection();
            self.finish_block_drag_selection();
            return;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            self.scrollbar_drag = None;
            return;
        };
        let policy = scrollbar_policy(runtime);
        let thumb_top = f64::from(event.position.y) - drag.pointer_y_offset_in_thumb;
        let _ = runtime.drag_scrollbar_to_thumb_top(policy, thumb_top);
        cx.stop_propagation();
        cx.notify();
    }

    pub(in crate::gui::app) fn on_scrollbar_mouse_up(
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
        if self.finish_table_hscroll_drag(cx) {
            cx.stop_propagation();
        }
        if self.commit_gutter_block_drag(cx) {
            cx.stop_propagation();
        }
        self.finish_table_cell_range_selection_drag();
        self.finish_gui_scrollbar_drag(cx);
        self.finish_text_drag_selection();
        self.finish_block_drag_selection();
    }
}

pub(in crate::gui::app) fn scroll_delta_y(event: &ScrollWheelEvent) -> f64 {
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
