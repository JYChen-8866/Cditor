use crate::{
    Canvas,
    selection::{
        HANDLE_HIT_TOLERANCE, HandleKind, ManipulationState, apply_manipulation, apply_rotation,
        hit_test_handles,
    },
    shapes::{Freehand, PathStyle, Shape, ShapeId, StrokeStyle},
    snap::{AngleSnapResult, SmartGuide, SnapResult},
    tools::{ToolKind, ToolState},
};
use kurbo::{Affine, Point, Vec2};

mod actions;
mod clipboard;
pub mod document;
mod history;
mod math;
mod snap;
mod text;
mod transient;

use history::DocumentHistory;
pub(crate) use transient::LaserPoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoverTarget {
    Handle(HandleKind),
    Shape,
    Canvas,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerOutcome {
    #[default]
    None,
    BeginTextEdit(ShapeId),
    OpenMathEditor(ShapeId),
}

fn rotation_aware_hit_test(shape: &Shape, point: Point, tolerance: f64) -> bool {
    let rotation = shape.rotation();
    if rotation.abs() <= 0.001 || matches!(shape, Shape::Math(_)) {
        return shape.hit_test(point, tolerance);
    }
    let local = Affine::rotate_about(-rotation, shape.bounds().center()) * point;
    shape.hit_test(local, tolerance)
}

#[derive(Clone, Debug)]
enum PointerState {
    Idle,
    Drawing,
    Panning {
        last: Point,
    },
    MovingSelection {
        start: Point,
        originals: std::collections::HashMap<ShapeId, Shape>,
        undo_started: bool,
        duplicate: bool,
        duplicated: bool,
    },
    Erasing {
        last: Point,
        undo_started: bool,
    },
    Laser,
    Manipulating(ManipulationState),
    Selecting {
        start: Point,
        current: Point,
        extend: bool,
    },
}

/// GPUI host state around Drafft's authoritative canvas.
///
/// Camera, tool state, document order, selection and undo remain owned by the
/// vendored upstream `Canvas`; this type only translates host pointer events.
#[derive(Clone)]
pub struct DrafftBoard {
    pub canvas: Canvas,
    document_revision: u64,
    pointer: PointerState,
    path_style: PathStyle,
    stroke_style: StrokeStyle,
    eraser_position: Option<Point>,
    laser_position: Option<Point>,
    laser_trail: Vec<LaserPoint>,
    grid_snap_enabled: bool,
    smart_snap_enabled: bool,
    angle_snap_enabled: bool,
    snap_result: Option<SnapResult>,
    angle_snap_result: Option<AngleSnapResult>,
    snap_line_start: Option<Point>,
    smart_guides: Vec<SmartGuide>,
    rotation_feedback: Option<(Point, Point, bool)>,
    history: DocumentHistory,
}

impl Default for DrafftBoard {
    fn default() -> Self {
        Self::new()
    }
}

impl DrafftBoard {
    pub fn new() -> Self {
        Self {
            canvas: Canvas::new(),
            document_revision: 0,
            pointer: PointerState::Idle,
            path_style: PathStyle::Direct,
            stroke_style: StrokeStyle::Solid,
            eraser_position: None,
            laser_position: None,
            laser_trail: Vec::new(),
            grid_snap_enabled: false,
            smart_snap_enabled: false,
            angle_snap_enabled: false,
            snap_result: None,
            angle_snap_result: None,
            snap_line_start: None,
            smart_guides: Vec::new(),
            rotation_feedback: None,
            history: DocumentHistory::default(),
        }
    }

    pub fn with_canvas(canvas: Canvas) -> Self {
        Self {
            canvas,
            document_revision: 0,
            pointer: PointerState::Idle,
            path_style: PathStyle::Direct,
            stroke_style: StrokeStyle::Solid,
            eraser_position: None,
            laser_position: None,
            laser_trail: Vec::new(),
            grid_snap_enabled: false,
            smart_snap_enabled: false,
            angle_snap_enabled: false,
            snap_result: None,
            angle_snap_result: None,
            snap_line_start: None,
            smart_guides: Vec::new(),
            rotation_feedback: None,
            history: DocumentHistory::default(),
        }
    }

    pub fn tool(&self) -> ToolKind {
        self.canvas.tool_manager.current_tool
    }

    pub fn set_tool(&mut self, tool: ToolKind) {
        self.cancel_pointer();
        self.eraser_position = None;
        self.laser_position = None;
        self.canvas.set_tool(tool);
    }

    pub fn pointer_down(
        &mut self,
        screen: Point,
        force_pan: bool,
        extend_selection: bool,
    ) -> PointerOutcome {
        self.pointer_down_with_options(screen, force_pan, extend_selection, false)
    }

    pub fn pointer_down_with_options(
        &mut self,
        screen: Point,
        force_pan: bool,
        extend_selection: bool,
        duplicate: bool,
    ) -> PointerOutcome {
        let tool = self.tool();
        if force_pan || tool == ToolKind::Pan {
            self.pointer = PointerState::Panning { last: screen };
            return PointerOutcome::None;
        }

        let world = self.canvas.camera.screen_to_world(screen);
        if let Some(manipulation) = self.manipulation_at(world) {
            self.pointer = PointerState::Manipulating(manipulation);
            return PointerOutcome::None;
        }
        match tool {
            ToolKind::Select => {
                if let Some(id) = self.shape_at(world, 6.0) {
                    if extend_selection && self.canvas.selection.contains(&id) {
                        self.canvas.selection.retain(|selected| *selected != id);
                        return PointerOutcome::None;
                    }
                    if !extend_selection && !self.canvas.selection.contains(&id) {
                        self.canvas.clear_selection();
                    }
                    self.canvas.add_to_selection(id);
                    let originals = self
                        .canvas
                        .selection
                        .iter()
                        .filter_map(|id| {
                            self.canvas
                                .document
                                .get_shape(*id)
                                .cloned()
                                .map(|shape| (*id, shape))
                        })
                        .collect();
                    self.pointer = PointerState::MovingSelection {
                        start: world,
                        originals,
                        undo_started: false,
                        duplicate,
                        duplicated: false,
                    };
                } else {
                    if !extend_selection {
                        self.canvas.clear_selection();
                    }
                    self.pointer = PointerState::Selecting {
                        start: world,
                        current: world,
                        extend: extend_selection,
                    };
                }
            }
            ToolKind::Eraser => {
                self.eraser_position = Some(world);
                let mut undo_started = false;
                self.erase_segment(world, world, &mut undo_started);
                self.pointer = PointerState::Erasing {
                    last: world,
                    undo_started,
                };
            }
            ToolKind::LaserPointer => {
                self.push_laser_point(world);
                self.pointer = PointerState::Laser;
            }
            ToolKind::Text => {
                if let Some(id) = self.shape_at(world, 5.0)
                    && matches!(self.canvas.document.get_shape(id), Some(Shape::Text(_)))
                {
                    self.canvas.clear_selection();
                    self.canvas.add_to_selection(id);
                    return PointerOutcome::BeginTextEdit(id);
                }
                let start = self.snap_drawing_start(world);
                self.canvas.tool_manager.begin(start);
                self.pointer = PointerState::Drawing;
            }
            _ => {
                let start = self.snap_drawing_start(world);
                self.canvas.tool_manager.begin(start);
                self.pointer = PointerState::Drawing;
            }
        }
        PointerOutcome::None
    }

    pub fn pointer_move(&mut self, screen: Point, constrain: bool) {
        let mut pointer = std::mem::replace(&mut self.pointer, PointerState::Idle);
        match &mut pointer {
            PointerState::Idle => {}
            PointerState::Panning { last } => {
                self.canvas
                    .camera
                    .pan(Vec2::new(screen.x - last.x, screen.y - last.y));
                *last = screen;
            }
            PointerState::Drawing => {
                let world = self.canvas.camera.screen_to_world(screen);
                let point = self.snap_drawing_point(world);
                self.canvas.tool_manager.update(point);
            }
            PointerState::MovingSelection {
                start,
                originals,
                undo_started,
                duplicate,
                duplicated,
            } => {
                let world = self.canvas.camera.screen_to_world(screen);
                let delta = self.snap_move_delta(originals, world - *start);
                if delta.hypot2() > f64::EPSILON {
                    if !*undo_started {
                        self.begin_history_transaction();
                        *undo_started = true;
                    }
                    if *duplicate && !*duplicated {
                        let ordered = self
                            .canvas
                            .document
                            .z_order
                            .iter()
                            .filter_map(|id| originals.get(id).cloned())
                            .collect::<Vec<_>>();
                        originals.clear();
                        self.canvas.clear_selection();
                        for mut shape in ordered {
                            shape.regenerate_id();
                            let id = shape.id();
                            self.capture_created_shapes([id]);
                            originals.insert(id, shape.clone());
                            self.canvas.document.add_shape(shape);
                            self.canvas.add_to_selection(id);
                        }
                        *duplicated = true;
                    }
                    let translation = Affine::translate(delta);
                    for (id, original) in originals.iter() {
                        if let Some(shape) = self.canvas.document.get_shape_mut(*id) {
                            let mut preview = original.clone();
                            preview.transform(translation);
                            *shape = preview;
                        }
                    }
                }
            }
            PointerState::Erasing { last, undo_started } => {
                let world = self.canvas.camera.screen_to_world(screen);
                self.erase_segment(*last, world, undo_started);
                self.eraser_position = Some(world);
                *last = world;
            }
            PointerState::Laser => {
                let world = self.canvas.camera.screen_to_world(screen);
                self.push_laser_point(world);
            }
            PointerState::Manipulating(manipulation) => {
                let world = self.canvas.camera.screen_to_world(screen);
                if matches!(manipulation.handle, Some(HandleKind::Rotate)) {
                    let center = manipulation.original_shape.bounds().center();
                    if let Some(shape) = self.canvas.document.get_shape_mut(manipulation.shape_id) {
                        apply_rotation(shape, world, constrain);
                    }
                    self.rotation_feedback = Some((center, world, constrain));
                    manipulation.current_point = world;
                } else {
                    let original_target = self.manipulation_target_position(manipulation);
                    let snapped = self.snap_manipulation_point(manipulation, world);
                    let preview = apply_manipulation(
                        &manipulation.original_shape,
                        manipulation.handle,
                        snapped - manipulation.start_point,
                        constrain,
                    );
                    if let Some(shape) = self.canvas.document.get_shape_mut(manipulation.shape_id) {
                        *shape = preview;
                    }
                    manipulation.current_point =
                        manipulation.start_point + (snapped - original_target);
                }
            }
            PointerState::Selecting { current, .. } => {
                *current = self.canvas.camera.screen_to_world(screen);
            }
        }
        self.pointer = pointer;
    }

    pub fn pointer_up(&mut self, screen: Point, constrain: bool) -> PointerOutcome {
        let pointer = std::mem::replace(&mut self.pointer, PointerState::Idle);
        let document_changed = matches!(
            &pointer,
            PointerState::Drawing
                | PointerState::Manipulating(_)
                | PointerState::MovingSelection {
                    undo_started: true,
                    ..
                }
                | PointerState::Erasing {
                    undo_started: true,
                    ..
                }
        );
        let outcome = match pointer {
            PointerState::Drawing => {
                let world = self.canvas.camera.screen_to_world(screen);
                let point = self.snap_drawing_point(world);
                self.finish_drawing(point)
            }
            PointerState::Manipulating(manipulation) => {
                self.commit_manipulation(manipulation, constrain);
                PointerOutcome::None
            }
            PointerState::Selecting {
                start,
                current,
                extend,
            } => {
                self.commit_selection_rect(start, current, extend);
                PointerOutcome::None
            }
            PointerState::Idle
            | PointerState::Panning { .. }
            | PointerState::MovingSelection { .. }
            | PointerState::Erasing { .. }
            | PointerState::Laser => PointerOutcome::None,
        };
        self.eraser_position = None;
        self.clear_snap_feedback();
        if document_changed {
            self.mark_document_changed();
        }
        outcome
    }

    pub fn pan(&mut self, delta: Vec2) {
        self.canvas.camera.pan(delta);
    }

    pub fn zoom_at(&mut self, screen: Point, factor: f64) {
        self.canvas.camera.zoom_at(screen, factor);
    }

    pub fn zoom_reset_at(&mut self, screen: Point) {
        let factor = crate::camera::BASE_ZOOM / self.canvas.camera.zoom;
        self.canvas.camera.zoom_at(screen, factor);
    }

    pub fn zoom_to_fit(&mut self, viewport: kurbo::Size) {
        self.canvas
            .set_viewport_size(viewport.width, viewport.height);
        let selected_bounds =
            self.canvas
                .selection
                .iter()
                .fold(None::<kurbo::Rect>, |bounds, id| {
                    let shape_bounds = self.canvas.document.get_shape(*id)?.bounds();
                    Some(match bounds {
                        Some(bounds) => bounds.union(shape_bounds),
                        None => shape_bounds,
                    })
                });
        if let Some(bounds) = selected_bounds {
            self.canvas.camera.fit_to_bounds(bounds, viewport, 50.0);
        } else {
            self.canvas.fit_to_content();
        }
    }

    pub fn center_canvas(&mut self) {
        self.canvas.camera.offset = Vec2::ZERO;
    }

    pub fn zoom_percent(&self) -> u32 {
        ((self.canvas.camera.zoom / crate::camera::BASE_ZOOM) * 100.0).round() as u32
    }

    pub fn cancel_pointer(&mut self) {
        if !matches!(self.canvas.tool_manager.state, ToolState::Idle) {
            self.canvas.tool_manager.cancel();
        }
        let pointer = std::mem::replace(&mut self.pointer, PointerState::Idle);
        self.eraser_position = None;
        self.clear_snap_feedback();
        if let PointerState::Manipulating(manipulation) = pointer
            && let Some(shape) = self.canvas.document.get_shape_mut(manipulation.shape_id)
        {
            *shape = manipulation.original_shape;
        }
    }

    pub fn selected(&self) -> &[ShapeId] {
        &self.canvas.selection
    }

    pub fn delete_selected(&mut self) -> bool {
        if self.canvas.selection.is_empty() {
            return false;
        }
        self.begin_history_transaction();
        self.capture_history_order();
        self.canvas.delete_selected();
        self.mark_document_changed();
        true
    }

    pub fn select_all(&mut self) {
        self.canvas.select_all();
    }

    pub(crate) fn selection_rect(&self) -> Option<kurbo::Rect> {
        let PointerState::Selecting { start, current, .. } = &self.pointer else {
            return None;
        };
        Some(kurbo::Rect::from_points(*start, *current))
    }

    pub(crate) fn rotation_feedback(&self) -> Option<(Point, Point, bool)> {
        self.rotation_feedback
    }

    pub(crate) fn hover_target(&self, screen: Point) -> HoverTarget {
        if self.tool() != ToolKind::Select {
            return HoverTarget::Canvas;
        }
        let world = self.canvas.camera.screen_to_world(screen);
        if let Some(manipulation) = self.manipulation_at(world) {
            return HoverTarget::Handle(manipulation.handle.expect("handle hit"));
        }
        if self.shape_at(world, 5.0).is_some() {
            HoverTarget::Shape
        } else {
            HoverTarget::Canvas
        }
    }

    pub fn reset_rotation_handle_at(&mut self, screen: Point) -> bool {
        let world = self.canvas.camera.screen_to_world(screen);
        let tolerance = HANDLE_HIT_TOLERANCE / self.canvas.camera.zoom.max(0.1);
        let target = self.canvas.selection.iter().rev().find_map(|id| {
            let shape = self.canvas.document.get_shape(*id)?;
            matches!(
                hit_test_handles(shape, world, tolerance),
                Some(HandleKind::Rotate)
            )
            .then_some(*id)
        });
        let Some(id) = target else {
            return false;
        };
        let Some(shape) = self.canvas.document.get_shape(id) else {
            return false;
        };
        if shape.rotation().abs() <= 0.001 {
            return true;
        }
        self.begin_history_transaction();
        if let Some(shape) = self.canvas.document.get_shape_mut(id) {
            shape.set_rotation(0.0);
        }
        self.mark_document_changed();
        true
    }

    fn manipulation_at(&self, world: Point) -> Option<ManipulationState> {
        let tolerance = HANDLE_HIT_TOLERANCE / self.canvas.camera.zoom.max(0.1);
        self.canvas.selection.iter().rev().find_map(|shape_id| {
            let shape = self.canvas.document.get_shape(*shape_id)?;
            let handle = hit_test_handles(shape, world, tolerance)?;
            Some(ManipulationState::new(
                *shape_id,
                Some(handle),
                world,
                shape.clone(),
            ))
        })
    }

    fn commit_manipulation(&mut self, manipulation: ManipulationState, constrain: bool) {
        let Some(final_shape) = self
            .canvas
            .document
            .get_shape(manipulation.shape_id)
            .cloned()
        else {
            return;
        };
        let changed = if matches!(manipulation.handle, Some(HandleKind::Rotate)) {
            (final_shape.rotation() - manipulation.original_shape.rotation()).abs() > 0.001
        } else {
            let delta = manipulation.delta();
            delta.x.abs() > 0.1 || delta.y.abs() > 0.1
        };
        if !changed {
            if let Some(shape) = self.canvas.document.get_shape_mut(manipulation.shape_id) {
                *shape = manipulation.original_shape;
            }
            return;
        }

        if let Some(shape) = self.canvas.document.get_shape_mut(manipulation.shape_id) {
            *shape = manipulation.original_shape.clone();
        }
        self.begin_history_transaction();
        let committed = if matches!(manipulation.handle, Some(HandleKind::Rotate)) {
            final_shape
        } else {
            apply_manipulation(
                &manipulation.original_shape,
                manipulation.handle,
                manipulation.delta(),
                constrain,
            )
        };
        if let Some(shape) = self.canvas.document.get_shape_mut(manipulation.shape_id) {
            *shape = committed;
        }
    }

    fn shape_at(&self, world: Point, screen_tolerance: f64) -> Option<ShapeId> {
        let tolerance = screen_tolerance / self.canvas.camera.zoom.max(0.1);
        self.canvas
            .document
            .shapes_ordered()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .find(|shape| rotation_aware_hit_test(shape, world, tolerance))
            .map(Shape::id)
    }

    fn commit_selection_rect(&mut self, start: Point, current: Point, extend: bool) {
        let rect = kurbo::Rect::from_points(start, current);
        if rect.width() <= 2.0 || rect.height() <= 2.0 {
            return;
        }
        if !extend {
            self.canvas.clear_selection();
        }
        for id in self.canvas.document.shapes_in_rect(rect) {
            self.canvas.add_to_selection(id);
        }
    }

    // Mirrors Drafft's native app release path: freehand keeps pressure data and
    // is simplified before commit; geometric tools reject accidental clicks.
    fn finish_drawing(&mut self, world: Point) -> PointerOutcome {
        match self.tool() {
            ToolKind::Freehand | ToolKind::Highlighter => {
                let points = self.canvas.tool_manager.freehand_points().to_vec();
                let pressures = self.canvas.tool_manager.freehand_pressures().to_vec();
                let mut style = self.canvas.tool_manager.current_style.clone();
                let tool = self.tool();
                self.canvas.tool_manager.cancel();
                if points.len() < 2 {
                    return PointerOutcome::None;
                }

                let mut freehand = Freehand::from_points_with_pressure(points, pressures);
                freehand.simplify(2.0);
                if tool == ToolKind::Highlighter {
                    style.stroke_width = style.stroke_width.max(12.0);
                    style.stroke_color.a = 128;
                }
                freehand.style = style;
                self.commit_shape(Shape::Freehand(freehand));
                PointerOutcome::None
            }
            _ => {
                if let Some(mut shape) = self.canvas.tool_manager.end(world) {
                    match &mut shape {
                        Shape::Line(line) => {
                            line.path_style = self.path_style;
                            line.stroke_style = self.stroke_style;
                        }
                        Shape::Arrow(arrow) => {
                            arrow.path_style = self.path_style;
                            arrow.stroke_style = self.stroke_style;
                        }
                        _ => {}
                    }
                    if matches!(shape, Shape::Text(_) | Shape::Math(_)) {
                        let is_text = matches!(shape, Shape::Text(_));
                        let id = self.commit_shape(shape);
                        self.canvas.clear_selection();
                        self.canvas.add_to_selection(id);
                        return if is_text {
                            PointerOutcome::BeginTextEdit(id)
                        } else {
                            PointerOutcome::OpenMathEditor(id)
                        };
                    }
                    let bounds = shape.bounds();
                    if bounds.width() > 1.0 || bounds.height() > 1.0 {
                        self.commit_shape(shape);
                    }
                }
                PointerOutcome::None
            }
        }
    }

    fn commit_shape(&mut self, shape: Shape) -> ShapeId {
        let id = shape.id();
        self.history.begin();
        self.capture_created_shapes([id]);
        self.canvas.document.add_shape(shape);
        id
    }
}

#[cfg(test)]
#[path = "model_host_tests.rs"]
mod tests;
