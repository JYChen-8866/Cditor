use drafftink_core::{
    selection::{Handle, HandleKind, get_handles},
    shapes::Shape,
};
use kurbo::{Affine, BezPath, Ellipse, Point, Rect, Shape as KurboShape, Vec2};

use super::plan::{PaintCommand, PaintKind};

const SELECTION_COLOR: u32 = 0x3b82f6ff;
const HANDLE_FILL: u32 = 0xffffffff;
const MIDPOINT_FILL: u32 = 0xc8dcffc8;
const HANDLE_SIZE_PX: f64 = 16.0;

pub(super) fn marquee_commands(rect: Rect, camera: Affine) -> [PaintCommand; 2] {
    let path = camera * rect.to_path(0.1);
    [
        fill(path.clone(), 0x3b82f619),
        stroke(path, 1.0, Some([4.0, 4.0])),
    ]
}

pub(super) fn selection_commands(shape: &Shape, camera: Affine) -> Vec<PaintCommand> {
    let mut commands = Vec::new();
    if !matches!(shape, Shape::Line(_) | Shape::Arrow(_)) {
        let transform = rotated_bounds_transform(shape, camera);
        commands.push(stroke(
            transform * shape.bounds().to_path(0.1),
            1.0,
            Some([4.0, 4.0]),
        ));
    }

    for handle in get_handles(shape) {
        push_handle(&mut commands, handle, camera);
    }
    commands
}

fn push_handle(commands: &mut Vec<PaintCommand>, handle: Handle, camera: Affine) {
    let position = camera * handle.position;
    match handle.kind {
        HandleKind::Endpoint(_) | HandleKind::IntermediatePoint(_) => {
            push_circle(commands, position, HANDLE_SIZE_PX / 2.0, HANDLE_FILL, 2.0);
        }
        HandleKind::SegmentMidpoint(_) => {
            push_circle(commands, position, HANDLE_SIZE_PX / 3.0, MIDPOINT_FILL, 1.5);
        }
        HandleKind::Corner(_) | HandleKind::Edge(_) => {
            let half = HANDLE_SIZE_PX / 2.0;
            let path = Rect::new(
                position.x - half,
                position.y - half,
                position.x + half,
                position.y + half,
            )
            .to_path(0.1);
            commands.push(fill(path.clone(), HANDLE_FILL));
            commands.push(stroke(path, 1.5, None));
        }
        HandleKind::Rotate => {
            let radius = HANDLE_SIZE_PX / 2.0;
            push_circle(commands, position, radius, HANDLE_FILL, 2.0);
            commands.push(stroke(rotation_arc(position, radius * 0.5), 1.5, None));
        }
    }
}

fn push_circle(
    commands: &mut Vec<PaintCommand>,
    center: Point,
    radius: f64,
    fill_color: u32,
    stroke_width: f32,
) {
    let path = Ellipse::new(center, (radius, radius), 0.0).to_path(0.1);
    commands.push(fill(path.clone(), fill_color));
    commands.push(stroke(path, stroke_width, None));
}

fn rotation_arc(center: Point, radius: f64) -> BezPath {
    let start_angle = -std::f64::consts::FRAC_PI_4;
    let end_angle = start_angle + std::f64::consts::PI * 1.5;
    let mut path = BezPath::new();
    for index in 0..=12 {
        let progress = index as f64 / 12.0;
        let angle = start_angle + progress * (end_angle - start_angle);
        let point = Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        );
        if index == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    path
}

fn rotated_bounds_transform(shape: &Shape, camera: Affine) -> Affine {
    let rotation = shape.rotation();
    if rotation.abs() <= 0.001 {
        return camera;
    }
    let center = shape.bounds().center();
    let vector = Vec2::new(center.x, center.y);
    camera * Affine::translate(vector) * Affine::rotate(rotation) * Affine::translate(-vector)
}

fn fill(path: BezPath, color: u32) -> PaintCommand {
    PaintCommand {
        path,
        color,
        kind: PaintKind::Fill,
    }
}

fn stroke(path: BezPath, width: f32, dash: Option<[f32; 2]>) -> PaintCommand {
    PaintCommand {
        path,
        color: SELECTION_COLOR,
        kind: PaintKind::Stroke { width, dash },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drafftink_core::shapes::Rectangle;

    #[test]
    fn rectangle_selection_emits_bounds_and_five_handles() {
        let shape = Shape::Rectangle(Rectangle::new(Point::ZERO, 100.0, 80.0));
        let commands = selection_commands(&shape, Affine::IDENTITY);

        // One bounds stroke, two commands for each corner, and three for rotate.
        assert_eq!(commands.len(), 12);
    }

    #[test]
    fn handle_geometry_stays_screen_sized_at_high_zoom() {
        let shape = Shape::Rectangle(Rectangle::new(Point::ZERO, 100.0, 80.0));
        let commands = selection_commands(&shape, Affine::scale(4.0));
        let first_handle_bounds = commands[1].path.bounding_box();

        assert_eq!(first_handle_bounds.width(), HANDLE_SIZE_PX);
        assert_eq!(first_handle_bounds.height(), HANDLE_SIZE_PX);
    }
}
