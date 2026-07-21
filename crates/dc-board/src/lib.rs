//! dc-board — OneNote-style infinite canvas with Cditor containers.
//!
//! A 2D pannable/zoomable canvas where each "Note Container" wraps a full
//! Cditor rich-text editor. Click anywhere to create a new container;
//! drag containers to reposition them.

use gpui::{
    App, AppContext, Context, CursorStyle, Entity, FocusHandle, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Point, Render,
    ScrollDelta, ScrollWheelEvent, Styled, Window, canvas, div, hsla, point, px, rgb,
};

// ── Camera (pan + zoom, adapted from ding-board) ──

const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 4.0;

#[derive(Debug, Clone, Copy)]
struct Camera {
    /// World-space offset of the viewport origin (top-left corner).
    pan: Point<f32>,
    /// Scale factor: screen_px = world_px * zoom.
    zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pan: point(0.0, 0.0),
            zoom: 1.0,
        }
    }
}

impl Camera {
    fn world_to_screen(&self, world: Point<f32>) -> Point<f32> {
        point(
            (world.x - self.pan.x) * self.zoom,
            (world.y - self.pan.y) * self.zoom,
        )
    }

    fn screen_to_world(&self, screen: Point<f32>) -> Point<f32> {
        point(
            screen.x / self.zoom + self.pan.x,
            screen.y / self.zoom + self.pan.y,
        )
    }
}

// ── Note Container ──

struct NoteContainer {
    /// World-space position (top-left corner of the container).
    world_pos: Point<f32>,
    /// World-space width in pixels.
    width: f32,
    /// The embedded Cditor editor.
    editor: Entity<cditor_editor::app::CditorV2View>,
}

impl NoteContainer {
    fn new(world_pos: Point<f32>, width: f32, cx: &mut App) -> Self {
        let runtime = cditor_runtime::DocumentRuntime::demo();
        let editor = cx.new(|cx| {
            cditor_editor::app::CditorV2View::from_runtime_with_options(runtime, false, false, cx)
        });
        Self {
            world_pos,
            width,
            editor,
        }
    }
}

// ── Board View ──

pub struct BoardView {
    focus: FocusHandle,
    camera: Camera,
    containers: Vec<NoteContainer>,

    // Drag state
    dragging: Option<Dragging>,
    drag_origin: Point<f32>,
}

enum Dragging {
    Canvas,
    Container(usize),
}

impl BoardView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            camera: Camera::default(),
            containers: Vec::new(),
            dragging: None,
            drag_origin: point(0.0, 0.0),
        }
    }

    fn on_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        match event.delta {
            ScrollDelta::Pixels(pixels) => {
                // Pan: scroll wheel pans the canvas
                self.camera.pan.x -= f32::from(pixels.x) / self.camera.zoom;
                self.camera.pan.y -= f32::from(pixels.y) / self.camera.zoom;
            }
            ScrollDelta::Lines(lines) => {
                self.camera.pan.x -= lines.x * 20.0 / self.camera.zoom;
                self.camera.pan.y -= lines.y * 20.0 / self.camera.zoom;
            }
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let world_pos = self.camera.screen_to_world(event.position.map(f32::from));

        // Check if clicking on a container (manual bounds check to avoid Pixels/f32 mismatch)
        let container_height: f32 = 200.0;
        for (i, container) in self.containers.iter().enumerate().rev() {
            let in_bounds = world_pos.x >= container.world_pos.x
                && world_pos.x <= container.world_pos.x + container.width
                && world_pos.y >= container.world_pos.y
                && world_pos.y <= container.world_pos.y + container_height;
            if in_bounds {
                self.dragging = Some(Dragging::Container(i));
                self.drag_origin = point(
                    world_pos.x - container.world_pos.x,
                    world_pos.y - container.world_pos.y,
                );
                return;
            }
        }

        // Click on empty space → create new container
        if event.button == MouseButton::Left {
            self.dragging = Some(Dragging::Canvas);
            self.drag_origin = world_pos;
        }
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let world_pos = self.camera.screen_to_world(event.position.map(f32::from));

        match self.dragging.take() {
            Some(Dragging::Canvas) => {
                let dx = world_pos.x - self.drag_origin.x;
                let dy = world_pos.y - self.drag_origin.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < 5.0 {
                    // Was a click, not a drag → create container
                    let container = NoteContainer::new(world_pos, 420.0, cx);
                    self.containers.push(container);
                    cx.notify();
                }
            }
            Some(Dragging::Container(_)) => {
                cx.notify();
            }
            None => {}
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let world_pos = self.camera.screen_to_world(event.position.map(f32::from));

        match self.dragging {
            Some(Dragging::Container(i)) => {
                self.containers[i].world_pos = point(
                    world_pos.x - self.drag_origin.x,
                    world_pos.y - self.drag_origin.y,
                );
            }
            Some(Dragging::Canvas) => {
                // Panning — not implemented in this simple version
            }
            None => {}
        }
    }
}

impl Render for BoardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let camera = self.camera;

        div()
            .id("dc-board")
            .relative()
            .size_full()
            .bg(rgb(0xfaf9f6))
            .track_focus(&self.focus)
            .cursor(CursorStyle::default())
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(
                // Grid background layer (simple dots)
                canvas(
                    move |_bounds, _window, _cx| {},
                    move |_bounds, _, _window, _cx| {},
                )
                .absolute()
                .size_full(),
            )
            .children(self.containers.iter().enumerate().map(|(_i, container)| {
                let screen_pos = camera.world_to_screen(container.world_pos);
                div()
                    .absolute()
                    .left(px(screen_pos.x))
                    .top(px(screen_pos.y))
                    .w(px(container.width))
                    .bg(rgb(0xffffff))
                    .rounded(px(8.0))
                    .border(px(1.0))
                    .border_color(rgb(0xe0e0e0))
                    .shadow(vec![gpui::BoxShadow {
                        color: hsla(0.0, 0.0, 0.0, 0.06),
                        offset: point(px(0.0), px(2.0)),
                        blur_radius: px(8.0),
                        spread_radius: px(0.0),
                        inset: false,
                    }])
                    .p(px(16.0))
                    .child(container.editor.clone())
            }))
    }
}
