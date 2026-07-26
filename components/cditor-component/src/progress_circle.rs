// Adapted from gpui-component's ProgressCircle and plot Arc implementations.
// Copyright 2024-2025 Longbridge. Licensed under Apache-2.0.

use std::{cell::Cell, f32::consts::TAU, time::Duration};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, ElementId, Hsla, InteractiveElement,
    IntoElement, ParentElement, Path, PathBuilder, Pixels, RenderOnce, Styled, Window, canvas, div,
    ease_in_out, hsla, point, px,
};

const DEFAULT_SIZE_PX: f32 = 16.0;
const MAX_STROKE_WIDTH_PX: f32 = 5.0;
const STROKE_WIDTH_RATIO: f32 = 0.15;
const VALUE_ANIMATION_DURATION: Duration = Duration::from_millis(150);
const LOADING_ANIMATION_DURATION: Duration = Duration::from_secs(1);
const FULL_CIRCLE_PAD_RADIANS: f32 = 0.0001;
const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;

struct ProgressState {
    value: f32,
    target: Cell<f32>,
}

impl ProgressState {
    fn new(value: f32) -> Self {
        Self {
            value,
            target: Cell::new(value),
        }
    }

    fn target(&self) -> f32 {
        self.target.get()
    }

    fn set_target(&self, value: f32) {
        self.target.set(value);
    }
}

/// Circular determinate or indeterminate progress indicator.
#[derive(IntoElement)]
pub struct ProgressCircle {
    id: ElementId,
    color: Hsla,
    track_color: Option<Hsla>,
    value: f32,
    size: Pixels,
    children: Vec<AnyElement>,
    loading: bool,
}

impl ProgressCircle {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            color: hsla(0.0, 0.0, 0.45, 1.0),
            track_color: None,
            value: 0.0,
            size: px(DEFAULT_SIZE_PX),
            children: Vec::new(),
            loading: false,
        }
    }

    /// Shows an infinite animated arc and ignores the determinate value.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = color.into();
        self
    }

    pub fn track_color(mut self, color: impl Into<Hsla>) -> Self {
        self.track_color = Some(color.into());
        self
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = clamp_progress(value);
        self
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = px(f32::from(size).max(0.0));
        self
    }

    fn render_circle(
        start_value: f32,
        end_value: f32,
        color: Hsla,
        track_color: Hsla,
    ) -> impl IntoElement {
        struct PrepaintState {
            start_value: f32,
            end_value: f32,
            inner_radius: f32,
            outer_radius: f32,
            bounds: Bounds<Pixels>,
        }

        canvas(
            move |bounds: Bounds<Pixels>, _window: &mut Window, _cx: &mut App| {
                let actual_size = f32::from(bounds.size.width.min(bounds.size.height));
                let stroke_width = (actual_size * STROKE_WIDTH_RATIO).min(MAX_STROKE_WIDTH_PX);
                let radius = (actual_size - stroke_width).max(0.0) / 2.0;
                PrepaintState {
                    start_value,
                    end_value,
                    inner_radius: (radius - stroke_width / 2.0).max(0.0),
                    outer_radius: radius + stroke_width / 2.0,
                    bounds,
                }
            },
            move |_bounds, state, window: &mut Window, _cx: &mut App| {
                paint_ring_segment(
                    0.0,
                    TAU,
                    state.inner_radius,
                    state.outer_radius,
                    state.bounds,
                    track_color,
                    window,
                );
                if state.end_value > state.start_value {
                    paint_ring_segment(
                        state.start_value / 100.0 * TAU,
                        state.end_value / 100.0 * TAU,
                        state.inner_radius,
                        state.outer_radius,
                        state.bounds,
                        color,
                        window,
                    );
                }
            },
        )
        .absolute()
        .size_full()
    }
}

impl ParentElement for ProgressCircle {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ProgressCircle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let value = self.value;
        let loading = self.loading;
        let color = self.color;
        let track_color = self.track_color.unwrap_or_else(|| color.opacity(0.2));
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ProgressState::new(value));
        let previous_target = state.read(cx).target();
        let value_changed = previous_target != value;

        div()
            .id(self.id.clone())
            .relative()
            .size(self.size)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .children(self.children)
            .map(|container| {
                if loading {
                    container
                        .with_animation(
                            "progress-circle-loading",
                            Animation::new(LOADING_ANIMATION_DURATION).repeat(),
                            move |container, delta| {
                                let (start, end) = loading_arc_range(delta);
                                container.child(Self::render_circle(start, end, color, track_color))
                            },
                        )
                        .into_any_element()
                } else if value_changed {
                    state.read(cx).set_target(value);
                    cx.spawn({
                        let state = state.clone();
                        async move |cx| {
                            cx.background_executor()
                                .timer(VALUE_ANIMATION_DURATION)
                                .await;
                            _ = state.update(cx, |state, _| state.value = state.target());
                        }
                    })
                    .detach();

                    container
                        .with_animation(
                            format!("progress-circle-{previous_target}"),
                            Animation::new(VALUE_ANIMATION_DURATION),
                            move |container, delta| {
                                let animated = previous_target + (value - previous_target) * delta;
                                container.child(Self::render_circle(
                                    0.0,
                                    animated,
                                    color,
                                    track_color,
                                ))
                            },
                        )
                        .into_any_element()
                } else {
                    container
                        .child(Self::render_circle(0.0, value, color, track_color))
                        .into_any_element()
                }
            })
    }
}

fn clamp_progress(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        0.0
    }
}

fn loading_arc_range(delta: f32) -> (f32, f32) {
    let delta = delta.clamp(0.0, 1.0);
    let end = ease_in_out(delta) * 100.0;
    let start = ease_in_out(((delta - 0.5) / 0.5).clamp(0.0, 1.0)) * 100.0;
    (start, end)
}

fn paint_ring_segment(
    start_angle: f32,
    end_angle: f32,
    inner_radius: f32,
    outer_radius: f32,
    bounds: Bounds<Pixels>,
    color: Hsla,
    window: &mut Window,
) {
    if let Some(path) =
        ring_segment_path(start_angle, end_angle, inner_radius, outer_radius, bounds)
    {
        window.paint_path(path, color);
    }
}

fn ring_segment_path(
    start_angle: f32,
    end_angle: f32,
    inner_radius: f32,
    outer_radius: f32,
    bounds: Bounds<Pixels>,
) -> Option<Path<Pixels>> {
    let arc_length = end_angle - start_angle;
    if outer_radius <= f32::EPSILON || arc_length <= f32::EPSILON {
        return None;
    }
    let pad = if arc_length >= std::f32::consts::PI {
        FULL_CIRCLE_PAD_RADIANS
    } else {
        0.0
    };
    let start = start_angle - HALF_PI + pad / 2.0;
    let end = end_angle - HALF_PI - pad / 2.0;
    if end <= start {
        return None;
    }

    let center_x = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
    let center_y = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
    let outer_start = point(
        px(center_x + outer_radius * start.cos()),
        px(center_y + outer_radius * start.sin()),
    );
    let outer_end = point(
        px(center_x + outer_radius * end.cos()),
        px(center_y + outer_radius * end.sin()),
    );
    let mut path = PathBuilder::fill();
    path.move_to(outer_start);
    path.arc_to(
        point(px(outer_radius), px(outer_radius)),
        px(0.0),
        end - start > std::f32::consts::PI,
        true,
        outer_end,
    );

    if inner_radius > f32::EPSILON {
        let inner_end = point(
            px(center_x + inner_radius * end.cos()),
            px(center_y + inner_radius * end.sin()),
        );
        let inner_start = point(
            px(center_x + inner_radius * start.cos()),
            px(center_y + inner_radius * start.sin()),
        );
        path.line_to(inner_end);
        path.arc_to(
            point(px(inner_radius), px(inner_radius)),
            px(0.0),
            end - start > std::f32::consts::PI,
            false,
            inner_start,
        );
    } else {
        path.line_to(point(px(center_x), px(center_y)));
    }
    path.close();
    path.build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_values_are_clamped_and_non_finite_values_are_safe() {
        assert_eq!(clamp_progress(-1.0), 0.0);
        assert_eq!(clamp_progress(42.0), 42.0);
        assert_eq!(clamp_progress(101.0), 100.0);
        assert_eq!(clamp_progress(f32::NAN), 0.0);
    }

    #[test]
    fn loading_arc_grows_then_advances_its_start() {
        assert_eq!(loading_arc_range(0.0), (0.0, 0.0));
        let (middle_start, middle_end) = loading_arc_range(0.5);
        assert_eq!(middle_start, 0.0);
        assert!(middle_end > 0.0);
        let (late_start, late_end) = loading_arc_range(0.75);
        assert!(late_start > 0.0);
        assert!(late_end > late_start);
    }
}
