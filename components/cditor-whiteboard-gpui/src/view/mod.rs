mod actions;
mod bottom_toolbar;
mod components;
mod file_menu;
mod focus;
mod input_bridge;
mod keyboard;
mod math_editor;
mod persistence;
mod properties;
mod right_panel;
mod tabs;
mod text_edit;
mod toolbar;

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use gpui::{
    App, AppContext, Bounds, Context, CursorStyle, Entity, FocusHandle, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
    Point, Render, ScrollWheelEvent, Styled, Subscription, Window, canvas, div, px,
};
use kurbo::{Point as KurboPoint, Size as KurboSize, Vec2};

use crate::selection::{Corner, HandleKind};
use crate::tools::ToolKind;
use crate::{Camera, canvas::CanvasDocument};
use crate::{
    DrafftBoard, WhiteboardTheme,
    font::UI_FONT_FAMILY,
    model_host::{HoverTarget, PointerOutcome},
    paint,
};

use self::components::color_picker::{ColorPicker, ColorPickerEvent};
use self::components::slider::{SliderEvent, SliderState};
use self::math_editor::MathEditState;
use self::{input_bridge::DrafftTextInputElement, text_edit::TextEditState};
use actions::{
    Cancel, Copy, Cut, DRAFFT_KEY_CONTEXT, DeleteBackward, DeleteForward, Duplicate, Ignore,
    MoveLeft, MoveRight, MoveToEnd, MoveToStart, Newline, Paste, Redo, SelectAll, SelectLeft,
    SelectRight, SelectToEnd, SelectToStart, Undo,
};

pub type SceneChangeFn = Rc<dyn Fn(String, &mut App)>;
pub use actions::bind_drafft_keys;
pub use focus::FocusRequestFn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrafftChromeMode {
    Full,
    BottomToolbarOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DrafftChromeVisibility {
    full_editor_chrome: bool,
    bottom_toolbar: bool,
}

fn chrome_visibility(read_only: bool, mode: DrafftChromeMode) -> DrafftChromeVisibility {
    DrafftChromeVisibility {
        full_editor_chrome: !read_only && matches!(mode, DrafftChromeMode::Full),
        bottom_toolbar: !read_only,
    }
}

pub struct DrafftBoardView {
    board: DrafftBoard,
    focus: FocusHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    cursor: CursorStyle,
    last_pointer: KurboPoint,
    shape_clipboard: Option<String>,
    space_pressed: bool,
    file_menu_open: bool,
    shortcuts_open: bool,
    current_path: Option<PathBuf>,
    recent_paths: Vec<PathBuf>,
    file_status: Option<String>,
    export_scale: u8,
    tabs: Vec<BoardTab>,
    active_tab: usize,
    grid_style: paint::GridStyle,
    last_theme_ink: Option<crate::shapes::SerializableColor>,
    stroke_picker: Entity<ColorPicker>,
    fill_picker: Entity<ColorPicker>,
    opacity_slider: Entity<SliderState>,
    opacity_dragging: bool,
    laser_fade_scheduled: bool,
    text_edit: Option<TextEditState>,
    text_caret_visible: bool,
    text_caret_epoch: u64,
    math_edit: Option<MathEditState>,
    math_input_bounds: Rc<Cell<Bounds<Pixels>>>,
    text_outline_engine: Rc<RefCell<paint::TextOutlineEngine>>,
    image_paint_engine: Rc<RefCell<paint::ImagePaintEngine>>,
    read_only: bool,
    chrome_mode: DrafftChromeMode,
    on_change: Option<SceneChangeFn>,
    on_focus_request: Option<FocusRequestFn>,
    last_observed_document_revision: u64,
    last_emitted_scene_json: String,
    persistence_epoch: u64,
    persistence_scheduled: bool,
    pointer_interaction_active: bool,
    _subscriptions: Vec<Subscription>,
}

struct BoardTab {
    name: String,
    document: Option<CanvasDocument>,
    camera: Camera,
}

impl DrafftBoardView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let board = DrafftBoard::new();
        Self::build(board, false, cx)
    }

    pub fn new_read_only(cx: &mut Context<Self>) -> Self {
        let board = DrafftBoard::new();
        Self::build(board, true, cx)
    }

    pub fn from_document_json(
        content: &str,
        read_only: bool,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let document = crate::parse_document_json(content)?;
        let board = DrafftBoard::with_canvas(crate::Canvas::with_document(document));
        Ok(Self::build(board, read_only, cx))
    }

    fn build(mut board: DrafftBoard, read_only: bool, cx: &mut Context<Self>) -> Self {
        if read_only {
            board.set_tool(ToolKind::Pan);
        }
        let initial_scene_json = board.document_json().unwrap_or_default();
        let initial_document_revision = board.document_revision();
        let style = board.canvas.tool_manager.current_style.clone();
        let stroke_picker = cx.new(|_| {
            ColorPicker::new(
                Some(properties::serializable_to_hsla(style.stroke_color)),
                false,
            )
        });
        let fill_picker = cx.new(|_| {
            ColorPicker::new(style.fill_color.map(properties::serializable_to_hsla), true)
        });
        let opacity_slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(style.opacity as f32)
        });
        let subscriptions = vec![
            cx.subscribe(&stroke_picker, |view, _, event, cx| {
                let ColorPickerEvent::Change(color) = event;
                if let Some(color) = color {
                    view.board
                        .set_stroke_color(properties::hsla_to_serializable(*color));
                    cx.notify();
                }
            }),
            cx.subscribe(&fill_picker, |view, _, event, cx| {
                let ColorPickerEvent::Change(color) = event;
                view.board
                    .set_fill_color(color.map(properties::hsla_to_serializable));
                cx.notify();
            }),
            cx.subscribe(&opacity_slider, |view, _, event, cx| {
                match event {
                    SliderEvent::Change(value) => {
                        if !view.opacity_dragging {
                            view.board.begin_opacity_edit();
                            view.opacity_dragging = true;
                        }
                        view.board.set_opacity_live(*value as f64);
                    }
                    SliderEvent::Release(value) => {
                        if !view.opacity_dragging {
                            view.board.begin_opacity_edit();
                        }
                        view.board.set_opacity_live(*value as f64);
                        view.board.commit_opacity_edit();
                        view.opacity_dragging = false;
                    }
                }
                cx.notify();
            }),
        ];
        let initial_tab = BoardTab {
            name: board.canvas.document.name.clone(),
            document: None,
            camera: board.canvas.camera.clone(),
        };
        Self {
            board,
            focus: cx.focus_handle(),
            bounds: Rc::new(Cell::new(Bounds::default())),
            cursor: CursorStyle::Arrow,
            last_pointer: KurboPoint::ZERO,
            shape_clipboard: None,
            space_pressed: false,
            file_menu_open: false,
            shortcuts_open: false,
            current_path: None,
            recent_paths: Vec::new(),
            file_status: None,
            export_scale: 1,
            tabs: vec![initial_tab],
            active_tab: 0,
            grid_style: paint::GridStyle::default(),
            last_theme_ink: None,
            stroke_picker,
            fill_picker,
            opacity_slider,
            opacity_dragging: false,
            laser_fade_scheduled: false,
            text_edit: None,
            text_caret_visible: false,
            text_caret_epoch: 0,
            math_edit: None,
            math_input_bounds: Rc::new(Cell::new(Bounds::default())),
            text_outline_engine: Rc::new(RefCell::new(paint::TextOutlineEngine::new())),
            image_paint_engine: Rc::new(RefCell::new(paint::ImagePaintEngine::default())),
            read_only,
            chrome_mode: DrafftChromeMode::Full,
            on_change: None,
            on_focus_request: None,
            last_observed_document_revision: initial_document_revision,
            last_emitted_scene_json: initial_scene_json,
            persistence_epoch: 0,
            persistence_scheduled: false,
            pointer_interaction_active: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn with_board(board: DrafftBoard, cx: &mut Context<Self>) -> Self {
        Self::build(board, false, cx)
    }

    pub fn board(&self) -> &DrafftBoard {
        &self.board
    }

    pub fn board_mut(&mut self) -> &mut DrafftBoard {
        &mut self.board
    }

    pub fn scene_json(&self) -> Result<String, serde_json::Error> {
        self.board.document_json()
    }

    pub fn set_on_change(&mut self, on_change: SceneChangeFn) {
        self.on_change = Some(on_change);
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        if self.read_only == read_only {
            return;
        }
        self.board.cancel_pointer();
        self.board.canvas.clear_selection();
        self.pointer_interaction_active = false;
        self.read_only = read_only;
        self.board.set_tool(if read_only {
            ToolKind::Pan
        } else {
            ToolKind::Select
        });
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn set_chrome_mode(&mut self, mode: DrafftChromeMode) {
        self.chrome_mode = mode;
    }

    pub fn chrome_mode(&self) -> DrafftChromeMode {
        self.chrome_mode
    }

    fn local_point(&self, point: Point<Pixels>) -> KurboPoint {
        let origin = self.bounds.get().origin;
        KurboPoint::new(
            f32::from(point.x - origin.x) as f64,
            f32::from(point.y - origin.y) as f64,
        )
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.request_focus(window, cx) {
            return;
        }
        self.pointer_interaction_active = true;
        if self.read_only {
            self.board.set_tool(ToolKind::Pan);
        } else {
            cx.stop_propagation();
        }
        let force_pan = event.button == MouseButton::Middle || event.modifiers.control;
        let local = self.local_point(event.position);
        self.last_pointer = local;
        if event.button == MouseButton::Left
            && event.click_count >= 2
            && self.board.tool() == ToolKind::Select
        {
            if self.board.reset_rotation_handle_at(local) {
                cx.notify();
                return;
            }
            match self.board.editable_shape_at(local) {
                PointerOutcome::BeginTextEdit(id) => {
                    self.begin_text_edit(id, false, cx);
                    self.place_text_caret(local, event.modifiers.shift, cx);
                    return;
                }
                PointerOutcome::OpenMathEditor(id) => {
                    self.open_math_editor(id, false, cx);
                    return;
                }
                PointerOutcome::None => {}
            }
        }
        let outcome = self.board.pointer_down_with_options(
            local,
            force_pan,
            event.modifiers.shift,
            event.modifiers.alt,
        );
        if let PointerOutcome::BeginTextEdit(id) = outcome {
            self.begin_text_edit(id, false, cx);
            self.place_text_caret(self.local_point(event.position), event.modifiers.shift, cx);
        }
        self.schedule_laser_fade(cx);
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button.is_some() || self.space_pressed {
            let local = self.local_point(event.position);
            self.last_pointer = local;
            self.board.pointer_move(local, event.modifiers.shift);
            self.schedule_laser_fade(cx);
            cx.notify();
            if !self.read_only {
                cx.stop_propagation();
            }
        } else {
            let local = self.local_point(event.position);
            self.last_pointer = local;
            if self.board.pointer_hover(local) {
                self.schedule_laser_fade(cx);
                cx.notify();
            }
            let cursor = cursor_for_tool(self.board.tool(), self.board.hover_target(local));
            if cursor != self.cursor {
                self.cursor = cursor;
                cx.notify();
            }
        }
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let local = self.local_point(event.position);
        self.last_pointer = local;
        let outcome = self.board.pointer_up(local, event.modifiers.shift);
        match outcome {
            PointerOutcome::BeginTextEdit(id) => self.begin_text_edit(id, true, cx),
            PointerOutcome::OpenMathEditor(id) => self.open_math_editor(id, true, cx),
            PointerOutcome::None => {}
        }
        self.sync_style_controls(cx);
        self.pointer_interaction_active = false;
        self.flush_current_scene(cx);
        cx.notify();
        if !self.read_only {
            cx.stop_propagation();
        }
    }

    fn on_mouse_up_out(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.pointer_interaction_active {
            return;
        }
        self.on_mouse_up(event, window, cx);
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only || !self.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        let delta = event.delta.pixel_delta(px(20.0));
        if event.modifiers.platform || event.modifiers.control {
            let factor = (1.0 + f32::from(delta.y) * 0.0025).clamp(0.5, 2.0) as f64;
            self.board.zoom_at(self.local_point(event.position), factor);
        } else {
            self.board.pan(Vec2::new(
                f32::from(delta.x) as f64,
                f32::from(delta.y) as f64,
            ));
        }
        cx.notify();
        cx.stop_propagation();
    }

    fn schedule_laser_fade(&mut self, cx: &mut Context<Self>) {
        if self.laser_fade_scheduled || !self.board.has_laser_trail() {
            return;
        }
        self.laser_fade_scheduled = true;
        let tick = cx.background_executor().timer(Duration::from_millis(16));
        cx.spawn(async move |view, cx| {
            let _ = tick.await;
            let _ = view.update(cx, |view, cx| {
                view.laser_fade_scheduled = false;
                if view.board.fade_laser_trail(1.0 / 60.0) {
                    view.schedule_laser_fade(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for DrafftBoardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = WhiteboardTheme::get(cx);
        // Keep the default tool ink legible on the themed canvas. The board's
        // tool style defaults to black, which disappears on a dark canvas.
        let ink = crate::shapes::SerializableColor::new(
            ((theme.ink >> 16) & 0xff) as u8,
            ((theme.ink >> 8) & 0xff) as u8,
            (theme.ink & 0xff) as u8,
            255,
        );
        if self.last_theme_ink != Some(ink) {
            let apply = {
                let current = self.board.canvas.tool_manager.current_style.stroke_color;
                let previous = self.last_theme_ink;
                let is_default = current == crate::shapes::SerializableColor::black();
                let unchanged = previous.is_none_or(|prev| current == prev);
                is_default || unchanged
            };
            if apply {
                self.last_theme_ink = Some(ink);
                self.board.canvas.tool_manager.current_style.stroke_color = ink;
            }
        }
        if (self.text_edit.is_some() || self.math_edit.is_some()) && !self.focus.is_focused(window)
        {
            window.focus(&self.focus, cx);
        }
        if !self.read_only && !self.pointer_interaction_active {
            self.observe_document_change(cx);
        }
        let bounds_cell = self.bounds.clone();
        let previous_bounds = self.bounds.get();
        let viewport_width = f32::from(previous_bounds.size.width);
        let viewport_height = f32::from(previous_bounds.size.height);
        let viewport_size = KurboSize::new(
            if viewport_width > 0.0 {
                viewport_width as f64
            } else {
                800.0
            },
            if viewport_height > 0.0 {
                viewport_height as f64
            } else {
                600.0
            },
        );
        self.text_outline_engine.borrow_mut().begin_frame();
        self.image_paint_engine.borrow_mut().begin_frame();
        self.text_outline_engine
            .borrow_mut()
            .prepare_interactive_geometry(&self.board.canvas, viewport_size);
        let mut plan = paint::PaintPlan::build_with_selection(
            &self.board.canvas,
            viewport_size,
            self.board.selection_rect(),
            self.grid_style,
            theme.grid,
            &mut self.text_outline_engine.borrow_mut(),
            &mut self.image_paint_engine.borrow_mut(),
        );
        self.text_outline_engine.borrow_mut().finish_frame();
        self.image_paint_engine.borrow_mut().finish_frame();
        plan.push_transient_overlays(&self.board);
        if let Some(edit) = &self.text_edit {
            plan.set_text_editing(
                edit.shape_id,
                edit.caret,
                edit.anchor,
                edit.marked_range.clone(),
                self.text_caret_visible,
            );
        }
        let board_layer = canvas(
            move |bounds, _, _| bounds_cell.set(bounds),
            move |bounds, _, window, cx| {
                paint::paint_plan(&plan, bounds.origin, window, cx);
            },
        )
        .absolute()
        .size_full();

        // The board owns pointer and wheel input only inside this surface.
        // Chrome is rendered later as sibling layers, so UI events never enter
        // the canvas state machine through ancestor bubbling.
        let board_surface = div()
            .id("drafft-board-surface")
            .absolute()
            .size_full()
            .child(board_layer)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up_out))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::on_mouse_up_out))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll));

        let mut root = div()
            .key_context(DRAFFT_KEY_CONTEXT)
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .cursor(self.cursor)
            .font_family(UI_FONT_FAMILY)
            .text_color(gpui::rgb(theme.text))
            .bg(gpui::rgb(theme.page))
            .child(board_surface)
            .child(
                div()
                    .absolute()
                    .size_full()
                    .child(DrafftTextInputElement::new(cx.entity())),
            )
            .on_action(cx.listener(|view, _: &Newline, _window, cx| view.handle_newline_action(cx)))
            .on_action(
                cx.listener(|view, _: &Cancel, window, cx| view.handle_cancel_action(window, cx)),
            )
            .on_action(cx.listener(|view, _: &MoveLeft, _window, cx| {
                view.handle_horizontal_action(true, false, cx)
            }))
            .on_action(cx.listener(|view, _: &MoveRight, _window, cx| {
                view.handle_horizontal_action(false, false, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectLeft, _window, cx| {
                view.handle_horizontal_action(true, true, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectRight, _window, cx| {
                view.handle_horizontal_action(false, true, cx)
            }))
            .on_action(cx.listener(|view, _: &MoveToStart, _window, cx| {
                view.handle_line_edge_action(true, false, cx)
            }))
            .on_action(cx.listener(|view, _: &MoveToEnd, _window, cx| {
                view.handle_line_edge_action(false, false, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectToStart, _window, cx| {
                view.handle_line_edge_action(true, true, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectToEnd, _window, cx| {
                view.handle_line_edge_action(false, true, cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteBackward, _window, cx| {
                view.handle_delete_action(true, cx)
            }))
            .on_action(cx.listener(|view, _: &DeleteForward, _window, cx| {
                view.handle_delete_action(false, cx)
            }))
            .on_action(
                cx.listener(|view, _: &SelectAll, _window, cx| view.handle_select_all_action(cx)),
            )
            .on_action(
                cx.listener(|view, _: &Copy, _window, cx| view.handle_copy_action(false, cx)),
            )
            .on_action(cx.listener(|view, _: &Cut, _window, cx| view.handle_copy_action(true, cx)))
            .on_action(cx.listener(|view, _: &Paste, _window, cx| view.handle_paste_action(cx)))
            .on_action(
                cx.listener(|view, _: &Undo, _window, cx| view.handle_history_action(false, cx)),
            )
            .on_action(
                cx.listener(|view, _: &Redo, _window, cx| view.handle_history_action(true, cx)),
            )
            .on_action(
                cx.listener(|view, _: &Duplicate, _window, cx| view.handle_duplicate_action(cx)),
            )
            .on_action(cx.listener(|view, _: &Ignore, _window, cx| view.ignore_bound_action(cx)))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up));
        let chrome = chrome_visibility(self.read_only, self.chrome_mode);
        if chrome.full_editor_chrome {
            root = root
                .child(self.render_toolbar(cx))
                .child(self.render_file_menu(cx))
                .child(self.render_tabs(cx))
                .child(self.render_properties(cx))
                .child(self.render_right_panel(cx))
                .child(self.render_math_editor(cx));
        }
        if chrome.bottom_toolbar {
            root = root.child(self.render_bottom_toolbar(cx));
        }
        root
    }
}

#[cfg(test)]
mod chrome_tests {
    use super::*;

    #[test]
    fn embedded_mode_keeps_only_the_bottom_toolbar() {
        assert_eq!(
            chrome_visibility(false, DrafftChromeMode::BottomToolbarOnly),
            DrafftChromeVisibility {
                full_editor_chrome: false,
                bottom_toolbar: true,
            }
        );
        assert_eq!(
            chrome_visibility(true, DrafftChromeMode::BottomToolbarOnly),
            DrafftChromeVisibility {
                full_editor_chrome: false,
                bottom_toolbar: false,
            }
        );
    }

    #[test]
    fn full_mode_preserves_editable_and_read_only_chrome_policy() {
        assert_eq!(
            chrome_visibility(false, DrafftChromeMode::Full),
            DrafftChromeVisibility {
                full_editor_chrome: true,
                bottom_toolbar: true,
            }
        );
        assert_eq!(
            chrome_visibility(true, DrafftChromeMode::Full),
            DrafftChromeVisibility {
                full_editor_chrome: false,
                bottom_toolbar: false,
            }
        );
    }
}

fn cursor_for_target(target: HoverTarget) -> CursorStyle {
    match target {
        HoverTarget::Handle(HandleKind::Corner(Corner::TopLeft | Corner::BottomRight)) => {
            CursorStyle::ResizeUpRightDownLeft
        }
        HoverTarget::Handle(HandleKind::Corner(Corner::TopRight | Corner::BottomLeft)) => {
            CursorStyle::ResizeUpLeftDownRight
        }
        HoverTarget::Handle(HandleKind::Edge(_)) => CursorStyle::ResizeLeftRight,
        HoverTarget::Handle(
            HandleKind::Endpoint(_)
            | HandleKind::IntermediatePoint(_)
            | HandleKind::SegmentMidpoint(_),
        ) => CursorStyle::Crosshair,
        HoverTarget::Handle(HandleKind::Rotate) => CursorStyle::OpenHand,
        HoverTarget::Shape => CursorStyle::OpenHand,
        HoverTarget::Canvas => CursorStyle::Arrow,
    }
}

fn cursor_for_tool(tool: ToolKind, target: HoverTarget) -> CursorStyle {
    match tool {
        ToolKind::Select => cursor_for_target(target),
        ToolKind::Pan => CursorStyle::OpenHand,
        ToolKind::Text => CursorStyle::IBeam,
        _ => CursorStyle::Crosshair,
    }
}
