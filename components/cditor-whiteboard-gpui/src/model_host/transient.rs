use crate::tools::ToolKind;
use kurbo::Point;

use super::DrafftBoard;

pub(crate) const ERASER_RADIUS: f64 = 10.0;
const LASER_TRAIL_LIMIT: usize = 50;
const LASER_FADE_PER_SECOND: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LaserPoint {
    pub(crate) position: Point,
    pub(crate) alpha: f32,
}

impl DrafftBoard {
    pub(crate) fn pointer_hover(&mut self, screen: Point) -> bool {
        let world = self.canvas.camera.screen_to_world(screen);
        match self.tool() {
            ToolKind::Eraser => {
                let changed = self.eraser_position != Some(world);
                self.eraser_position = Some(world);
                changed
            }
            ToolKind::LaserPointer => {
                self.push_laser_point(world);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn eraser_cursor(&self) -> Option<(Point, f64)> {
        (self.tool() == ToolKind::Eraser)
            .then_some(self.eraser_position)
            .flatten()
            .map(|position| (position, ERASER_RADIUS))
    }

    pub(crate) fn laser_position(&self) -> Option<Point> {
        (self.tool() == ToolKind::LaserPointer)
            .then_some(self.laser_position)
            .flatten()
    }

    pub(crate) fn laser_trail(&self) -> &[LaserPoint] {
        &self.laser_trail
    }

    pub(crate) fn has_laser_trail(&self) -> bool {
        !self.laser_trail.is_empty()
    }

    pub(crate) fn fade_laser_trail(&mut self, elapsed_seconds: f32) -> bool {
        let fade = elapsed_seconds.max(0.0) * LASER_FADE_PER_SECOND;
        for point in &mut self.laser_trail {
            point.alpha -= fade;
        }
        self.laser_trail.retain(|point| point.alpha > 0.0);
        !self.laser_trail.is_empty()
    }

    pub(super) fn push_laser_point(&mut self, position: Point) {
        self.laser_position = Some(position);
        self.laser_trail.push(LaserPoint {
            position,
            alpha: 1.0,
        });
        let overflow = self.laser_trail.len().saturating_sub(LASER_TRAIL_LIMIT);
        if overflow > 0 {
            self.laser_trail.drain(..overflow);
        }
    }
}

impl DrafftBoard {
    pub(super) fn erase_segment(&mut self, start: Point, end: Point, undo_started: &mut bool) {
        let distance = start.distance(end);
        let steps = (distance / (ERASER_RADIUS * 0.5)).ceil().max(1.0) as usize;
        let hits = self
            .canvas
            .document
            .shapes_ordered()
            .filter(|shape| {
                (0..=steps).any(|index| {
                    let ratio = index as f64 / steps as f64;
                    let point = start.lerp(end, ratio);
                    shape.hit_test(point, ERASER_RADIUS)
                })
            })
            .map(|shape| shape.id())
            .collect::<Vec<_>>();

        if hits.is_empty() {
            return;
        }
        if !*undo_started {
            self.history.begin();
            self.capture_history_order();
            *undo_started = true;
        }
        self.capture_history_shapes(hits.iter().copied());
        for id in hits {
            self.canvas.remove_shape(id);
        }
    }
}
