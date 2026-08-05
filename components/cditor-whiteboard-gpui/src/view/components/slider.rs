use gpui::{
    App, AppContext, Bounds, Context, DragMoveEvent, Empty, Entity, EntityId, EventEmitter,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Pixels, Point,
    Render, RenderOnce, StatefulInteractiveElement, Styled, Window, canvas, div, px, relative, rgb,
};

use crate::theme::chrome;

#[derive(Clone)]
struct DragSlider(EntityId);

impl Render for DragSlider {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::view) enum SliderEvent {
    Change(f32),
    Release(f32),
}

pub(in crate::view) struct SliderState {
    min: f32,
    max: f32,
    step: f32,
    value: f32,
    percentage: f32,
    bounds: Bounds<Pixels>,
    dragging: bool,
}

impl SliderState {
    pub(in crate::view) fn new() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            value: 0.0,
            percentage: 0.0,
            bounds: Bounds::default(),
            dragging: false,
        }
    }

    pub(in crate::view) fn min(mut self, min: f32) -> Self {
        self.min = min;
        self.update_thumb_position();
        self
    }

    pub(in crate::view) fn max(mut self, max: f32) -> Self {
        self.max = max;
        self.update_thumb_position();
        self
    }

    pub(in crate::view) fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    pub(in crate::view) fn default_value(mut self, value: f32) -> Self {
        self.value = value.clamp(self.min, self.max);
        self.update_thumb_position();
        self
    }

    pub(in crate::view) fn set_value(&mut self, value: f32, cx: &mut Context<Self>) {
        self.value = value.clamp(self.min, self.max);
        self.update_thumb_position();
        cx.notify();
    }

    fn update_thumb_position(&mut self) {
        let range = self.max - self.min;
        self.percentage = if range <= f32::EPSILON {
            0.0
        } else {
            ((self.value - self.min) / range).clamp(0.0, 1.0)
        };
    }

    fn update_value_by_position(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dragging = true;
        let width = self.bounds.size.width;
        if width <= px(0.0) {
            return;
        }
        let percentage =
            ((position.x - self.bounds.left()).clamp(px(0.0), width) / width).clamp(0.0, 1.0);
        let value = self.min + (self.max - self.min) * percentage;
        self.value = ((value / self.step).round() * self.step).clamp(self.min, self.max);
        self.update_thumb_position();
        cx.emit(SliderEvent::Change(self.value));
        cx.notify();
    }

    fn handle_release(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.dragging {
            return false;
        }
        self.dragging = false;
        cx.emit(SliderEvent::Release(self.value));
        true
    }
}

impl EventEmitter<SliderEvent> for SliderState {}

#[derive(IntoElement)]
pub(in crate::view) struct Slider {
    state: Entity<SliderState>,
}

impl Slider {
    pub(in crate::view) fn new(state: &Entity<SliderState>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let entity_id = self.state.entity_id();
        let percentage = self.state.read(cx).percentage;
        let c = chrome(cx);

        div()
            .id(("slider", entity_id))
            .h(px(24.0))
            .w_full()
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                window.listener_for(&self.state, |state, _, _, cx| {
                    if state.handle_release(cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                window.listener_for(&self.state, |state, _, _, cx| {
                    if state.handle_release(cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .child(
                div()
                    .id("slider-bar-container")
                    .relative()
                    .h(px(24.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .on_mouse_down(
                        MouseButton::Left,
                        window.listener_for(
                            &self.state,
                            |state, event: &MouseDownEvent, window, cx| {
                                state.update_value_by_position(event.position, window, cx);
                                cx.stop_propagation();
                            },
                        ),
                    )
                    .on_drag(DragSlider(entity_id), |drag, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| drag.clone())
                    })
                    .on_drag_move(window.listener_for(
                        &self.state,
                        move |state, event: &DragMoveEvent<DragSlider>, window, cx| {
                            let DragSlider(id) = event.drag(cx);
                            if *id != entity_id {
                                return;
                            }
                            state.update_value_by_position(event.event.position, window, cx);
                        },
                    ))
                    .child(
                        div()
                            .id("slider-bar")
                            .relative()
                            .h(px(6.0))
                            .w_full()
                            .rounded(px(999.0))
                            .bg(rgb(c.accent).opacity(0.2))
                            .active(move |style| style.bg(rgb(c.accent).opacity(0.4)))
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .bottom_0()
                                    .right(relative(1.0 - percentage))
                                    .rounded(px(999.0))
                                    .bg(rgb(c.accent)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(-5.0))
                                    .left(relative(percentage))
                                    .ml(px(-8.0))
                                    .size(px(16.0))
                                    .p(px(1.0))
                                    .rounded(px(999.0))
                                    .bg(rgb(c.accent).opacity(0.5))
                                    .shadow_md()
                                    .child(div().size_full().rounded(px(999.0)).bg(rgb(c.bg))),
                            )
                            .child({
                                let state = self.state.clone();
                                canvas(
                                    move |bounds, _, cx| {
                                        state.update(cx, |state, _| state.bounds = bounds)
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full()
                            }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_and_maps_single_value() {
        let state = SliderState::new()
            .min(0.0)
            .max(1.0)
            .step(0.01)
            .default_value(2.0);
        assert_eq!(state.value, 1.0);
        assert_eq!(state.percentage, 1.0);
    }
}
