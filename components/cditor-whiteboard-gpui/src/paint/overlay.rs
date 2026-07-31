use crate::snap::{AngleSnapResult, SmartGuide, SmartGuideKind, SnapResult};
use kurbo::{Affine, BezPath, Circle, Point, Shape as KurboShape};

use crate::model_host::LaserPoint;

use super::plan::{PaintCommand, PaintKind};

pub(super) fn transient_overlay_commands(
    eraser: Option<(Point, f64)>,
    laser_position: Option<Point>,
    laser_trail: &[LaserPoint],
    snap_result: Option<SnapResult>,
    angle_snap: Option<(Point, AngleSnapResult)>,
    smart_guides: &[SmartGuide],
    rotation_feedback: Option<(Point, Point, bool)>,
    camera: Affine,
    zoom: f64,
) -> Vec<PaintCommand> {
    let mut commands = Vec::new();
    if let Some((position, radius)) = eraser {
        let circle = camera * Circle::new(position, radius).to_path(0.1);
        commands.push(fill(circle.clone(), 0xff646432));
        commands.push(stroke(circle, 0xc83232c8, 2.0));
    }

    let zoom = zoom.max(0.1);
    for point in laser_trail {
        let alpha = point.alpha.clamp(0.0, 1.0);
        let radius = 4.0 / zoom * alpha as f64;
        let circle = camera * Circle::new(point.position, radius).to_path(0.1);
        commands.push(fill(circle, rgba(255, 0, 0, alpha)));
    }

    if let Some(position) = laser_position {
        commands.push(fill(
            camera * Circle::new(position, 12.0 / zoom).to_path(0.1),
            0xff000064,
        ));
        commands.push(fill(
            camera * Circle::new(position, 6.0 / zoom).to_path(0.1),
            0xff3232ff,
        ));
        commands.push(fill(
            camera * Circle::new(position, 2.0 / zoom).to_path(0.1),
            0xffffffff,
        ));
    }
    commands.extend(snap_commands(
        snap_result,
        angle_snap,
        smart_guides,
        camera,
        zoom,
    ));
    if let Some((center, pointer, snapped)) = rotation_feedback {
        let mut helper = BezPath::new();
        helper.move_to(center);
        helper.line_to(pointer);
        commands.push(PaintCommand {
            path: camera * helper,
            color: if snapped { 0xec4899ff } else { 0x8b5cf6ff },
            kind: PaintKind::Stroke {
                width: 1.0,
                dash: Some([5.0, 4.0]),
            },
        });
        commands.push(fill(
            camera * Circle::new(center, 3.0 / zoom).to_path(0.1),
            0x8b5cf6ff,
        ));
        // Draw protractor-style angle indicator
        commands.extend(angle_indicator_arc(center, pointer, snapped, zoom, camera));
    }
    commands
}

fn snap_commands(
    snap_result: Option<SnapResult>,
    angle_snap: Option<(Point, AngleSnapResult)>,
    guides: &[SmartGuide],
    camera: Affine,
    zoom: f64,
) -> Vec<PaintCommand> {
    const GUIDE_COLOR: u32 = 0x8b5cf6ff;
    const SNAP_COLOR: u32 = 0xec4899ff;
    let mut commands = Vec::new();
    for guide in guides {
        let mut path = BezPath::new();
        match guide.kind {
            SmartGuideKind::Vertical | SmartGuideKind::EqualSpacingV => {
                path.move_to((guide.position, guide.start));
                path.line_to((guide.position, guide.end));
                for point in &guide.snap_points {
                    commands.push(fill(
                        camera * Circle::new((guide.position, *point), 2.5 / zoom).to_path(0.1),
                        GUIDE_COLOR,
                    ));
                }
            }
            SmartGuideKind::Horizontal | SmartGuideKind::EqualSpacingH => {
                path.move_to((guide.start, guide.position));
                path.line_to((guide.end, guide.position));
                for point in &guide.snap_points {
                    commands.push(fill(
                        camera * Circle::new((*point, guide.position), 2.5 / zoom).to_path(0.1),
                        GUIDE_COLOR,
                    ));
                }
            }
        }
        commands.push(PaintCommand {
            path: camera * path,
            color: GUIDE_COLOR,
            kind: PaintKind::Stroke {
                width: 1.0,
                dash: Some([4.0, 3.0]),
            },
        });
    }
    if let Some(result) = snap_result.filter(SnapResult::is_snapped) {
        commands.push(fill(
            camera * Circle::new(result.point, 4.0 / zoom).to_path(0.1),
            SNAP_COLOR,
        ));
    }
    if let Some((origin, result)) = angle_snap.filter(|(_, result)| result.snapped) {
        let mut path = BezPath::new();
        path.move_to(origin);
        path.line_to(result.point);
        commands.push(PaintCommand {
            path: camera * path,
            color: GUIDE_COLOR,
            kind: PaintKind::Stroke {
                width: 1.0,
                dash: Some([5.0, 4.0]),
            },
        });
    }
    commands
}

fn fill(path: BezPath, color: u32) -> PaintCommand {
    PaintCommand {
        path,
        color,
        kind: PaintKind::Fill,
    }
}

fn stroke(path: BezPath, color: u32, width: f32) -> PaintCommand {
    PaintCommand {
        path,
        color,
        kind: PaintKind::Stroke { width, dash: None },
    }
}

fn rgba(red: u8, green: u8, blue: u8, alpha: f32) -> u32 {
    ((red as u32) << 24)
        | ((green as u32) << 16)
        | ((blue as u32) << 8)
        | (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
}

/// Draw a protractor-style angle indicator when rotating shapes.
/// Shows a semicircular arc with tick marks at 15° intervals,
/// and highlights the current rotation angle.
fn angle_indicator_arc(
    center: Point,
    pointer: Point,
    snapped: bool,
    zoom: f64,
    camera: Affine,
) -> Vec<PaintCommand> {
    use std::f64::consts::{FRAC_PI_2, PI};
    let mut commands = Vec::new();

    let dx = pointer.x - center.x;
    let dy = pointer.y - center.y;
    let angle_rad = dy.atan2(dx);
    let _angle_deg = angle_rad.to_degrees(); // reserved for future text display

    // Protractor radius in world coordinates (40px / zoom)
    let arc_radius = 40.0 / zoom;
    let color = if snapped { 0xec4899ff } else { 0x8b5cf6dd };

    // Draw the semicircular arc (0° to 180° in standard math orientation,
    // drawn from -90° to 90° relative to the 0° line pointing right)
    let mut arc_path = BezPath::new();
    let segments = 60;
    let start_angle = -FRAC_PI_2;
    let end_angle = FRAC_PI_2;
    let angle_step = (end_angle - start_angle) / segments as f64;
    let first = Point::new(
        center.x + arc_radius * start_angle.cos(),
        center.y - arc_radius * start_angle.sin(),
    );
    arc_path.move_to(first);
    for i in 1..=segments {
        let a = start_angle + angle_step * i as f64;
        let p = Point::new(
            center.x + arc_radius * a.cos(),
            center.y - arc_radius * a.sin(),
        );
        arc_path.line_to(p);
    }
    commands.push(PaintCommand {
        path: camera * arc_path,
        color: color & 0x00ffffff | 0x44000000,
        kind: PaintKind::Stroke {
            width: 1.0,
            dash: None,
        },
    });

    // Draw tick marks at multiples of 15°
    for deg in (-90..=90).step_by(15) {
        let a = deg as f64 * PI / 180.0;
        let is_major = deg % 45 == 0;
        let tick_len = if is_major { 8.0 / zoom } else { 4.0 / zoom };
        let inner_r = arc_radius - tick_len;
        let outer_r = arc_radius;
        let mut tick = BezPath::new();
        tick.move_to(Point::new(
            center.x + inner_r * a.cos(),
            center.y - inner_r * a.sin(),
        ));
        tick.line_to(Point::new(
            center.x + outer_r * a.cos(),
            center.y - outer_r * a.sin(),
        ));
        let tick_color = if is_major { color } else { color & 0x00ffffff | 0x66000000 };
        commands.push(PaintCommand {
            path: camera * tick,
            color: tick_color,
            kind: PaintKind::Stroke {
                width: if is_major { 1.5 } else { 0.75 },
                dash: None,
            },
        });
    }

    // Draw the current angle line from center to the arc
    let mut angle_line = BezPath::new();
    angle_line.move_to(center);
    angle_line.line_to(Point::new(
        center.x + arc_radius * angle_rad.cos(),
        center.y - arc_radius * angle_rad.sin(),
    ));
    commands.push(PaintCommand {
        path: camera * angle_line,
        color,
        kind: PaintKind::Stroke {
            width: 2.0,
            dash: None,
        },
    });

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn laser_overlay_contains_trail_glow_pointer_and_center() {
        let position = Point::new(20.0, 30.0);
        let commands = transient_overlay_commands(
            None,
            Some(position),
            &[LaserPoint {
                position,
                alpha: 0.5,
            }],
            None,
            None,
            &[],
            None,
            Affine::IDENTITY,
            1.0,
        );
        assert_eq!(commands.len(), 4);
        assert_eq!(commands[0].color & 0xff, 128);
    }

    #[test]
    fn eraser_overlay_has_fill_and_stroke() {
        let commands = transient_overlay_commands(
            Some((Point::new(10.0, 10.0), 10.0)),
            None,
            &[],
            None,
            None,
            &[],
            None,
            Affine::IDENTITY,
            1.0,
        );
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn smart_guides_and_snap_points_emit_overlay_commands() {
        let guide = SmartGuide {
            kind: SmartGuideKind::Vertical,
            position: 20.0,
            start: 0.0,
            end: 100.0,
            snap_points: vec![30.0],
        };
        let commands = snap_commands(
            Some(SnapResult {
                point: Point::new(20.0, 30.0),
                snapped_x: true,
                snapped_y: false,
            }),
            None,
            &[guide],
            Affine::IDENTITY,
            1.0,
        );
        assert_eq!(commands.len(), 3);
    }
}
