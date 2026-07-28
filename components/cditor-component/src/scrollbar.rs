use std::{cell::Cell, rc::Rc, time::Duration};
use web_time::Instant;

use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, Element, ElementId, EntityId, GlobalElementId, Hitbox,
    HitboxBehavior, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ScrollHandle, Style, Window, point, px,
    relative, rgb, size,
};

const SCROLLBAR_HOVER_ANIMATION_DURATION: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InteractiveScrollbarStyle {
    pub idle_thickness_px: f32,
    pub active_thickness_px: f32,
    pub hit_thickness_px: f32,
    pub min_thumb_extent_px: f32,
    pub track_inset_px: f32,
    pub thumb: u32,
    pub thumb_hover: u32,
}

impl InteractiveScrollbarStyle {
    pub fn notion(thumb: u32, thumb_hover: u32) -> Self {
        Self {
            idle_thickness_px: 4.0,
            active_thickness_px: 10.0,
            hit_thickness_px: 16.0,
            min_thumb_extent_px: 24.0,
            track_inset_px: 3.0,
            thumb,
            thumb_hover,
        }
    }
}

type ScrollbarChangeHandler = Rc<dyn Fn(f32, &mut Window, &mut App) + 'static>;
type ScrollbarLifecycleHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

#[derive(Clone)]
enum ScrollbarTarget {
    Handle {
        handle: ScrollHandle,
        estimated_max_offset_px: f32,
    },
    Callback {
        offset_px: f32,
        max_offset_px: f32,
        visible_fraction: f32,
        on_change: ScrollbarChangeHandler,
    },
}

impl ScrollbarTarget {
    fn model(&self, axis: ScrollbarAxis) -> ScrollbarModel {
        match self {
            Self::Handle {
                handle,
                estimated_max_offset_px,
            } => {
                let max_offset_px = axis
                    .coordinate(handle.max_offset())
                    .max(*estimated_max_offset_px)
                    .max(0.0);
                let offset_px = (-axis.coordinate(handle.offset())).clamp(0.0, max_offset_px);
                ScrollbarModel {
                    offset_px,
                    max_offset_px,
                    visible_fraction: 1.0,
                }
            }
            Self::Callback {
                offset_px,
                max_offset_px,
                visible_fraction,
                ..
            } => ScrollbarModel {
                offset_px: offset_px.clamp(0.0, *max_offset_px),
                max_offset_px: max_offset_px.max(0.0),
                visible_fraction: visible_fraction.clamp(0.0, 1.0),
            },
        }
    }

    fn set_offset(&self, axis: ScrollbarAxis, offset_px: f32, window: &mut Window, cx: &mut App) {
        match self {
            Self::Handle { handle, .. } => {
                let mut offset = handle.offset();
                axis.set_coordinate(&mut offset, -offset_px);
                handle.set_offset(offset);
            }
            Self::Callback { on_change, .. } => on_change(offset_px, window, cx),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScrollbarModel {
    offset_px: f32,
    max_offset_px: f32,
    visible_fraction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScrollbarMetrics {
    thumb_bounds: Bounds<Pixels>,
    track_start_px: f32,
    thumb_extent_px: f32,
    travel_px: f32,
    max_offset_px: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScrollbarDragState {
    owner: u64,
    grab_offset_px: f32,
}

thread_local! {
    static SCROLLBAR_DRAG_STATE: Cell<Option<ScrollbarDragState>> = const { Cell::new(None) };
}

pub struct InteractiveScrollbar {
    id: Option<ElementId>,
    axis: ScrollbarAxis,
    target: ScrollbarTarget,
    viewport_extent_px: f32,
    style: InteractiveScrollbarStyle,
    on_drag_start: Option<ScrollbarLifecycleHandler>,
    on_drag_end: Option<ScrollbarLifecycleHandler>,
}

impl InteractiveScrollbar {
    pub fn for_scroll_handle(
        axis: ScrollbarAxis,
        handle: ScrollHandle,
        viewport_extent_px: f32,
        estimated_content_extent_px: f32,
        style: InteractiveScrollbarStyle,
    ) -> Self {
        Self {
            id: None,
            axis,
            target: ScrollbarTarget::Handle {
                handle,
                estimated_max_offset_px: (estimated_content_extent_px - viewport_extent_px)
                    .max(0.0),
            },
            viewport_extent_px,
            style,
            on_drag_start: None,
            on_drag_end: None,
        }
    }

    pub fn for_callback(
        axis: ScrollbarAxis,
        offset_px: f32,
        max_offset_px: f32,
        visible_fraction: f32,
        style: InteractiveScrollbarStyle,
        on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: None,
            axis,
            target: ScrollbarTarget::Callback {
                offset_px,
                max_offset_px,
                visible_fraction,
                on_change: Rc::new(on_change),
            },
            viewport_extent_px: 0.0,
            style,
            on_drag_start: None,
            on_drag_end: None,
        }
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn on_drag_start(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_drag_start = Some(Rc::new(handler));
        self
    }

    pub fn on_drag_end(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_drag_end = Some(Rc::new(handler));
        self
    }
}

impl IntoElement for InteractiveScrollbar {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[doc(hidden)]
pub struct InteractiveScrollbarPrepaint {
    metrics: Option<ScrollbarMetrics>,
    thumb_bounds: Option<Bounds<Pixels>>,
    hitbox: Hitbox,
    owner: u64,
    active: bool,
    dragging: bool,
}

#[derive(Debug, Clone, Copy)]
struct ScrollbarAnimationState {
    active_progress: f32,
    target_active: bool,
    last_frame: Instant,
}

impl ScrollbarAnimationState {
    fn new(active: bool, now: Instant) -> Self {
        Self {
            active_progress: if active { 1.0 } else { 0.0 },
            target_active: active,
            last_frame: now,
        }
    }

    fn update(&mut self, active: bool, now: Instant) -> f32 {
        if self.target_active != active {
            self.target_active = active;
            self.last_frame = now;
            return self.active_progress;
        }
        let elapsed = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        let step = elapsed.as_secs_f32() / SCROLLBAR_HOVER_ANIMATION_DURATION.as_secs_f32();
        if self.target_active {
            self.active_progress = (self.active_progress + step).min(1.0);
        } else {
            self.active_progress = (self.active_progress - step).max(0.0);
        }
        self.active_progress
    }

    fn animating(self) -> bool {
        if self.target_active {
            self.active_progress < 1.0
        } else {
            self.active_progress > 0.0
        }
    }
}

impl Element for InteractiveScrollbar {
    type RequestLayoutState = ();
    type PrepaintState = InteractiveScrollbarPrepaint;

    fn id(&self) -> Option<gpui::ElementId> {
        self.id.clone()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style {
            position: gpui::Position::Absolute,
            ..Style::default()
        };
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let model = self.target.model(self.axis);
        let visible_fraction = match self.target {
            ScrollbarTarget::Handle { .. } => {
                let content_extent_px = self.viewport_extent_px + model.max_offset_px;
                if content_extent_px <= 0.5 {
                    1.0
                } else {
                    self.viewport_extent_px / content_extent_px
                }
            }
            ScrollbarTarget::Callback { .. } => model.visible_fraction,
        };
        let metrics = scrollbar_metrics(
            bounds,
            self.axis,
            ScrollbarModel {
                visible_fraction,
                ..model
            },
            self.style,
        );
        let hitbox_bounds = scrollbar_hitbox_bounds(bounds, self.axis, self.style.hit_thickness_px);
        let hitbox = window.insert_hitbox(hitbox_bounds, HitboxBehavior::Normal);
        let owner = scrollbar_owner(window.current_view(), hitbox_bounds, self.axis);
        let dragging = drag_state().is_some_and(|state| state.owner == owner);
        let active = hitbox.is_hovered(window)
            || dragging
            || hitbox_bounds.contains(&window.mouse_position());
        let active_progress = window.with_optional_element_state(id, |state, window| match state {
            Some(previous) => {
                let now = Instant::now();
                let mut animation =
                    previous.unwrap_or_else(|| ScrollbarAnimationState::new(active, now));
                let progress = animation.update(active, now);
                if animation.animating() {
                    window.request_animation_frame();
                }
                (progress, Some(animation))
            }
            None => (if active { 1.0 } else { 0.0 }, None),
        });
        let thickness_px = animated_scrollbar_thickness(self.style, active_progress);
        let thumb_bounds = metrics.map(|metrics| {
            scrollbar_thumb_bounds_for_thickness(metrics.thumb_bounds, self.axis, thickness_px)
        });
        InteractiveScrollbarPrepaint {
            metrics,
            thumb_bounds,
            hitbox,
            owner,
            active,
            dragging,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let Some(metrics) = prepaint.metrics else {
            return;
        };
        let Some(thumb_bounds) = prepaint.thumb_bounds else {
            return;
        };
        let owner = prepaint.owner;
        let current_view = window.current_view();
        let was_active = prepaint.active;
        let hitbox_bounds = prepaint.hitbox.bounds;
        window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture {
                let active = drag_state().is_some_and(|state| state.owner == owner)
                    || hitbox_bounds.contains(&window.mouse_position());
                if active != was_active {
                    cx.notify(current_view);
                    window.refresh();
                }
            }
        });

        let target = self.target.clone();
        let on_drag_start = self.on_drag_start.clone();
        let axis = self.axis;
        let hitbox = prepaint.hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture
                && event.button == MouseButton::Left
                && hitbox.is_hovered(window)
            {
                if let Some(on_drag_start) = on_drag_start.as_ref() {
                    on_drag_start(window, cx);
                }
                let pointer_px = axis.coordinate(event.position);
                let thumb_start_px = axis.bounds_start(thumb_bounds);
                let grab_offset_px = if thumb_bounds.contains(&event.position) {
                    pointer_px - thumb_start_px
                } else {
                    metrics.thumb_extent_px / 2.0
                };
                set_drag_state(Some(ScrollbarDragState {
                    owner,
                    grab_offset_px,
                }));
                set_scrollbar_position(
                    &target,
                    axis,
                    metrics,
                    pointer_px,
                    grab_offset_px,
                    window,
                    cx,
                );
                cx.stop_propagation();
                cx.notify(current_view);
                window.refresh();
            }
        });

        let target = self.target.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture {
                return;
            }
            let Some(drag) = drag_state().filter(|state| state.owner == owner) else {
                return;
            };
            set_scrollbar_position(
                &target,
                axis,
                metrics,
                axis.coordinate(event.position),
                drag.grab_offset_px,
                window,
                cx,
            );
            cx.stop_propagation();
            cx.notify(current_view);
            window.refresh();
        });

        let on_drag_end = self.on_drag_end.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture
                && event.button == MouseButton::Left
                && drag_state().is_some_and(|state| state.owner == owner)
            {
                set_drag_state(None);
                if let Some(on_drag_end) = on_drag_end.as_ref() {
                    on_drag_end(window, cx);
                }
                cx.stop_propagation();
                cx.notify(current_view);
                window.refresh();
            }
        });

        if prepaint.dragging {
            window.set_window_cursor_style(self.axis.resize_cursor());
        } else if prepaint.active {
            window.set_cursor_style(CursorStyle::PointingHand, &prepaint.hitbox);
        }

        let color = if prepaint.active {
            self.style.thumb_hover
        } else {
            self.style.thumb
        };
        window.paint_quad(PaintQuad {
            bounds: thumb_bounds,
            corner_radii: gpui::Corners::all(self.axis.cross_extent(thumb_bounds) / 2.0),
            background: rgb(color).into(),
            border_widths: gpui::Edges::all(px(0.0)),
            border_color: gpui::transparent_black(),
            border_style: gpui::BorderStyle::Solid,
        });
    }
}

fn smoothstep(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

fn animated_scrollbar_thickness(style: InteractiveScrollbarStyle, progress: f32) -> f32 {
    style.idle_thickness_px
        + (style.active_thickness_px - style.idle_thickness_px) * smoothstep(progress)
}

fn scrollbar_metrics(
    bounds: Bounds<Pixels>,
    axis: ScrollbarAxis,
    model: ScrollbarModel,
    style: InteractiveScrollbarStyle,
) -> Option<ScrollbarMetrics> {
    let track_start_px = axis.bounds_start(bounds) + style.track_inset_px;
    let track_extent_px = (axis.bounds_extent(bounds) - style.track_inset_px * 2.0).max(0.0);
    if model.max_offset_px <= 0.5 || track_extent_px <= 0.5 {
        return None;
    }
    let thumb_extent_px = (track_extent_px * model.visible_fraction)
        .max(style.min_thumb_extent_px)
        .min(track_extent_px);
    let travel_px = (track_extent_px - thumb_extent_px).max(0.0);
    let progress = (model.offset_px / model.max_offset_px).clamp(0.0, 1.0);
    let thumb_start_px = track_start_px + travel_px * progress;
    let thumb_bounds = axis.make_thumb_bounds(
        bounds,
        thumb_start_px,
        thumb_extent_px,
        style.idle_thickness_px,
    );
    Some(ScrollbarMetrics {
        thumb_bounds,
        track_start_px,
        thumb_extent_px,
        travel_px,
        max_offset_px: model.max_offset_px,
    })
}

fn set_scrollbar_position(
    target: &ScrollbarTarget,
    axis: ScrollbarAxis,
    metrics: ScrollbarMetrics,
    pointer_px: f32,
    grab_offset_px: f32,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(offset_px) = scrollbar_offset_for_pointer(metrics, pointer_px, grab_offset_px) {
        target.set_offset(axis, offset_px, window, cx);
    }
}

fn scrollbar_offset_for_pointer(
    metrics: ScrollbarMetrics,
    pointer_px: f32,
    grab_offset_px: f32,
) -> Option<f32> {
    if metrics.travel_px <= 0.5 || metrics.max_offset_px <= 0.5 {
        return None;
    }
    let thumb_start_px =
        (pointer_px - metrics.track_start_px - grab_offset_px).clamp(0.0, metrics.travel_px);
    Some(thumb_start_px / metrics.travel_px * metrics.max_offset_px)
}

fn scrollbar_hitbox_bounds(
    bounds: Bounds<Pixels>,
    axis: ScrollbarAxis,
    hit_thickness_px: f32,
) -> Bounds<Pixels> {
    match axis {
        ScrollbarAxis::Vertical => Bounds {
            origin: point(
                bounds.left() + (bounds.size.width - px(hit_thickness_px)) / 2.0,
                bounds.top(),
            ),
            size: size(px(hit_thickness_px), bounds.size.height),
        },
        ScrollbarAxis::Horizontal => Bounds {
            origin: point(
                bounds.left(),
                bounds.top() + (bounds.size.height - px(hit_thickness_px)) / 2.0,
            ),
            size: size(bounds.size.width, px(hit_thickness_px)),
        },
    }
}

fn scrollbar_thumb_bounds_for_thickness(
    bounds: Bounds<Pixels>,
    axis: ScrollbarAxis,
    thickness_px: f32,
) -> Bounds<Pixels> {
    match axis {
        ScrollbarAxis::Vertical => Bounds {
            origin: point(
                bounds.left() + (bounds.size.width - px(thickness_px)) / 2.0,
                bounds.top(),
            ),
            size: size(px(thickness_px), bounds.size.height),
        },
        ScrollbarAxis::Horizontal => Bounds {
            origin: point(
                bounds.left(),
                bounds.top() + (bounds.size.height - px(thickness_px)) / 2.0,
            ),
            size: size(bounds.size.width, px(thickness_px)),
        },
    }
}

fn scrollbar_owner(view_id: EntityId, bounds: Bounds<Pixels>, axis: ScrollbarAxis) -> u64 {
    let mut hash = view_id.as_u64() ^ axis.owner_salt();
    for value in [
        bounds.origin.x.as_f32(),
        bounds.origin.y.as_f32(),
        bounds.size.width.as_f32(),
        bounds.size.height.as_f32(),
    ] {
        hash = hash.wrapping_mul(16_777_619) ^ u64::from(value.to_bits());
    }
    hash
}

fn drag_state() -> Option<ScrollbarDragState> {
    SCROLLBAR_DRAG_STATE.with(Cell::get)
}

fn set_drag_state(state: Option<ScrollbarDragState>) {
    SCROLLBAR_DRAG_STATE.with(|slot| slot.set(state));
}

impl ScrollbarAxis {
    fn coordinate(self, point: Point<Pixels>) -> f32 {
        f32::from(match self {
            Self::Horizontal => point.x,
            Self::Vertical => point.y,
        })
    }

    fn set_coordinate(self, point: &mut Point<Pixels>, value: f32) {
        match self {
            Self::Horizontal => point.x = px(value),
            Self::Vertical => point.y = px(value),
        }
    }

    fn bounds_start(self, bounds: Bounds<Pixels>) -> f32 {
        f32::from(match self {
            Self::Horizontal => bounds.left(),
            Self::Vertical => bounds.top(),
        })
    }

    fn bounds_extent(self, bounds: Bounds<Pixels>) -> f32 {
        f32::from(match self {
            Self::Horizontal => bounds.size.width,
            Self::Vertical => bounds.size.height,
        })
    }

    fn cross_extent(self, bounds: Bounds<Pixels>) -> Pixels {
        match self {
            Self::Horizontal => bounds.size.height,
            Self::Vertical => bounds.size.width,
        }
    }

    fn make_thumb_bounds(
        self,
        track_bounds: Bounds<Pixels>,
        start_px: f32,
        extent_px: f32,
        thickness_px: f32,
    ) -> Bounds<Pixels> {
        match self {
            Self::Vertical => Bounds {
                origin: point(
                    track_bounds.left() + (track_bounds.size.width - px(thickness_px)) / 2.0,
                    px(start_px),
                ),
                size: size(px(thickness_px), px(extent_px)),
            },
            Self::Horizontal => Bounds {
                origin: point(
                    px(start_px),
                    track_bounds.top() + (track_bounds.size.height - px(thickness_px)) / 2.0,
                ),
                size: size(px(extent_px), px(thickness_px)),
            },
        }
    }

    fn resize_cursor(self) -> CursorStyle {
        match self {
            Self::Horizontal => CursorStyle::ResizeLeftRight,
            Self::Vertical => CursorStyle::ResizeUpDown,
        }
    }

    const fn owner_salt(self) -> u64 {
        match self {
            Self::Horizontal => 0x484f5249,
            Self::Vertical => 0x56455254,
        }
    }
}

#[cfg(test)]
mod tests;
