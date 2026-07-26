// Adapted from Drafft Ink's event_handler.rs. The snap geometry itself remains
// in the vendored drafftink-core crate; this module only binds it to GPUI state.

use std::collections::HashMap;

use drafftink_core::{
    selection::{HandleKind, ManipulationState, get_manipulation_target_position},
    shapes::{Shape, ShapeId},
    snap::{
        ENDPOINT_SNAP_RADIUS, GRID_SIZE, MULTI_MOVE_SNAP_RADIUS, SMART_GUIDE_THRESHOLD,
        detect_smart_guides, detect_smart_guides_for_point, snap_line_endpoint_isometric,
        snap_to_grid,
    },
    tools::{ToolKind, ToolState},
};
use kurbo::{Point, Rect, Size, Vec2};

use super::DrafftBoard;

const MAX_SNAP_CANDIDATES: usize = 200;

impl DrafftBoard {
    pub fn grid_snap_enabled(&self) -> bool {
        self.grid_snap_enabled
    }

    pub fn smart_snap_enabled(&self) -> bool {
        self.smart_snap_enabled
    }

    pub fn angle_snap_enabled(&self) -> bool {
        self.angle_snap_enabled
    }

    pub fn toggle_grid_snap(&mut self) {
        self.grid_snap_enabled = !self.grid_snap_enabled;
        self.clear_snap_feedback();
    }

    pub fn toggle_smart_snap(&mut self) {
        self.smart_snap_enabled = !self.smart_snap_enabled;
        self.clear_snap_feedback();
    }

    pub fn toggle_angle_snap(&mut self) {
        self.angle_snap_enabled = !self.angle_snap_enabled;
        self.clear_snap_feedback();
    }

    pub(crate) fn snap_result(&self) -> Option<drafftink_core::snap::SnapResult> {
        self.snap_result
    }

    pub(crate) fn angle_snap_result(
        &self,
    ) -> Option<(Point, drafftink_core::snap::AngleSnapResult)> {
        Some((self.snap_line_start?, self.angle_snap_result?))
    }

    pub(crate) fn smart_guides(&self) -> &[drafftink_core::snap::SmartGuide] {
        &self.smart_guides
    }

    pub(super) fn clear_snap_feedback(&mut self) {
        self.snap_result = None;
        self.angle_snap_result = None;
        self.snap_line_start = None;
        self.smart_guides.clear();
        self.rotation_feedback = None;
    }

    pub(super) fn snap_drawing_start(&mut self, point: Point) -> Point {
        self.clear_snap_feedback();
        if matches!(self.tool(), ToolKind::Freehand | ToolKind::Highlighter) {
            return point;
        }
        if self.grid_snap_enabled {
            let result = snap_to_grid(point, GRID_SIZE);
            self.snap_result = Some(result);
            result.point
        } else {
            point
        }
    }

    pub(super) fn snap_drawing_point(&mut self, point: Point) -> Point {
        self.snap_result = None;
        self.angle_snap_result = None;
        self.smart_guides.clear();
        let ToolState::Active { start, .. } = self.canvas.tool_manager.state else {
            return point;
        };
        if matches!(self.tool(), ToolKind::Line | ToolKind::Arrow) {
            self.snap_line_start = Some(start);
            let result = snap_line_endpoint_isometric(
                start,
                point,
                self.angle_snap_enabled,
                self.grid_snap_enabled,
                false,
                GRID_SIZE,
            );
            self.angle_snap_result = self.angle_snap_enabled.then_some(result);
            return result.point;
        }
        if self.grid_snap_enabled
            && !matches!(self.tool(), ToolKind::Freehand | ToolKind::Highlighter)
        {
            let result = snap_to_grid(point, GRID_SIZE);
            self.snap_result = Some(result);
            result.point
        } else {
            point
        }
    }

    pub(super) fn snap_move_delta(
        &mut self,
        originals: &HashMap<ShapeId, Shape>,
        raw_delta: Vec2,
    ) -> Vec2 {
        self.snap_result = None;
        self.angle_snap_result = None;
        self.smart_guides.clear();
        if raw_delta.hypot() <= 2.0 {
            return raw_delta;
        }
        let Some(original_bounds) = originals
            .values()
            .map(Shape::bounds)
            .reduce(|combined, bounds| combined.union(bounds))
        else {
            return raw_delta;
        };
        let target_bounds = original_bounds + raw_delta;
        let excluded = originals.keys().copied().collect::<Vec<_>>();
        let mut delta = raw_delta;

        if self.smart_snap_enabled {
            let zone = target_bounds.inflate(MULTI_MOVE_SNAP_RADIUS, MULTI_MOVE_SNAP_RADIUS);
            let candidates = self.collect_snap_candidates(&excluded, Some(zone));
            let result = detect_smart_guides(
                target_bounds,
                &candidates,
                SMART_GUIDE_THRESHOLD / self.canvas.camera.zoom.max(0.1),
            );
            if result.snapped_x || result.snapped_y {
                delta.x += result.point.x - target_bounds.x0;
                delta.y += result.point.y - target_bounds.y0;
                self.smart_guides = result.guides;
            }
        }
        if self.grid_snap_enabled {
            let target = Point::new(original_bounds.x0 + delta.x, original_bounds.y0 + delta.y);
            let result = snap_to_grid(target, GRID_SIZE);
            delta = result.point - Point::new(original_bounds.x0, original_bounds.y0);
            self.snap_result = Some(result);
        }
        delta
    }

    pub(super) fn manipulation_target_position(&self, manipulation: &ManipulationState) -> Point {
        get_manipulation_target_position(&manipulation.original_shape, manipulation.handle)
    }

    pub(super) fn snap_manipulation_point(
        &mut self,
        manipulation: &ManipulationState,
        world: Point,
    ) -> Point {
        self.snap_result = None;
        self.angle_snap_result = None;
        self.smart_guides.clear();
        let original = self.manipulation_target_position(manipulation);
        let target = original + (world - manipulation.start_point);
        let mut point = target;
        let is_endpoint = matches!(
            manipulation.handle,
            Some(HandleKind::Endpoint(_) | HandleKind::IntermediatePoint(_))
        ) && matches!(
            manipulation.original_shape,
            Shape::Line(_) | Shape::Arrow(_)
        );

        if is_endpoint && self.angle_snap_enabled {
            let origin = other_endpoint(&manipulation.original_shape, manipulation.handle);
            let result = snap_line_endpoint_isometric(
                origin,
                target,
                true,
                self.grid_snap_enabled,
                false,
                GRID_SIZE,
            );
            self.snap_line_start = Some(origin);
            self.angle_snap_result = Some(result);
            point = result.point;
        } else {
            if self.smart_snap_enabled {
                let zone = Rect::from_center_size(
                    target,
                    Size::new(ENDPOINT_SNAP_RADIUS * 2.0, ENDPOINT_SNAP_RADIUS * 2.0),
                );
                let mut candidates =
                    self.collect_snap_candidates(&[manipulation.shape_id], Some(zone));
                candidates.extend(self_snap_rects(
                    &manipulation.original_shape,
                    manipulation.handle,
                ));
                let result = detect_smart_guides_for_point(
                    target,
                    &candidates,
                    SMART_GUIDE_THRESHOLD / self.canvas.camera.zoom.max(0.1),
                );
                if result.snapped_x || result.snapped_y {
                    point = result.point;
                    self.smart_guides = result.guides;
                }
            }
            if self.grid_snap_enabled {
                point = snap_to_grid(point, GRID_SIZE).point;
            }
        }
        self.snap_result = Some(drafftink_core::snap::SnapResult {
            point,
            snapped_x: (point.x - target.x).abs() > 0.001,
            snapped_y: (point.y - target.y).abs() > 0.001,
        });
        point
    }

    fn collect_snap_candidates(&self, excluded: &[ShapeId], zone: Option<Rect>) -> Vec<Rect> {
        let viewport = self.canvas.visible_world_bounds();
        let mut bounds = Vec::new();
        for shape in self
            .canvas
            .document
            .shapes_ordered()
            .filter(|shape| !excluded.contains(&shape.id()))
            .filter(|shape| shape.bounds().overlaps(viewport))
            .filter(|shape| zone.is_none_or(|zone| shape.bounds().overlaps(zone)))
            .take(MAX_SNAP_CANDIDATES)
        {
            bounds.push(shape.bounds());
            bounds.extend(
                shape
                    .snap_points()
                    .into_iter()
                    .map(|point| Rect::new(point.x, point.y, point.x, point.y)),
            );
        }
        bounds
    }
}

fn self_snap_rects(shape: &Shape, handle: Option<HandleKind>) -> Vec<Rect> {
    let dragged = match handle {
        Some(HandleKind::Endpoint(index)) => Some(index),
        Some(HandleKind::IntermediatePoint(index)) => Some(index + 1),
        _ => None,
    };
    shape
        .snap_points()
        .into_iter()
        .enumerate()
        .filter(|(index, _)| dragged != Some(*index))
        .map(|(_, point)| Rect::new(point.x, point.y, point.x, point.y))
        .collect()
}

fn other_endpoint(shape: &Shape, handle: Option<HandleKind>) -> Point {
    match (shape, handle) {
        (Shape::Line(line), Some(HandleKind::Endpoint(0))) => line.end,
        (Shape::Line(line), _) => line.start,
        (Shape::Arrow(arrow), Some(HandleKind::Endpoint(0))) => arrow.end,
        (Shape::Arrow(arrow), _) => arrow.start,
        _ => Point::ZERO,
    }
}
