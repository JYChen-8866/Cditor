use crate::shapes::FillPattern;
use kurbo::{BezPath, Line, ParamCurve, PathEl, Point, Rect, Shape as KurboShape, flatten};
use roughr::core::{FillStyle, OpSetType, OpType, OptionsBuilder};

const FLATTEN_TOLERANCE: f64 = 0.2;
const INTERSECTION_EPSILON: f64 = 1e-7;

pub(super) fn generate_clipped_fill_pattern(
    pattern: FillPattern,
    bounds: Rect,
    clip_path: &BezPath,
    stroke_width: f64,
    seed: u32,
) -> BezPath {
    let pattern_path = generate_fill_pattern(pattern, bounds, stroke_width, seed);
    clip_path_segments(&pattern_path, clip_path)
}

/// Pattern generation matches Drafft's Vello renderer; only clipping is moved
/// from a renderer clip layer into geometry because GPUI exposes rectangular
/// content masks rather than arbitrary path masks.
fn generate_fill_pattern(
    pattern: FillPattern,
    bounds: Rect,
    stroke_width: f64,
    seed: u32,
) -> BezPath {
    let fill_style = match pattern {
        FillPattern::Solid => return BezPath::new(),
        FillPattern::Hachure => FillStyle::Hachure,
        FillPattern::ZigZag => FillStyle::ZigZag,
        FillPattern::CrossHatch => FillStyle::CrossHatch,
        FillPattern::Dots => FillStyle::Dots,
        FillPattern::Dashed => FillStyle::Dashed,
        FillPattern::ZigZagLine => FillStyle::ZigZagLine,
    };
    let color = roughr::Srgba::new(0.0, 0.0, 0.0, 1.0);
    let options = OptionsBuilder::default()
        .seed(seed as u64)
        .fill_style(fill_style)
        .fill(color)
        .stroke(color)
        .fill_weight((stroke_width * 0.5) as f32)
        .hachure_gap((stroke_width * 4.0) as f32)
        .build()
        .expect("Drafft fill pattern options are valid");
    let drawing = roughr::generator::Generator::default().rectangle::<f64>(
        bounds.x0,
        bounds.y0,
        bounds.width(),
        bounds.height(),
        &Some(options),
    );

    let mut path = BezPath::new();
    for set in &drawing.sets {
        if set.op_set_type != OpSetType::FillSketch {
            continue;
        }
        for operation in &set.ops {
            match operation.op {
                OpType::Move => path.move_to(Point::new(operation.data[0], operation.data[1])),
                OpType::LineTo => path.line_to(Point::new(operation.data[0], operation.data[1])),
                OpType::BCurveTo => path.curve_to(
                    Point::new(operation.data[0], operation.data[1]),
                    Point::new(operation.data[2], operation.data[3]),
                    Point::new(operation.data[4], operation.data[5]),
                ),
            }
        }
    }
    path
}

fn clip_path_segments(source: &BezPath, clip_path: &BezPath) -> BezPath {
    let boundary_segments: Vec<_> = clip_path.segments().collect();
    let mut flattened = Vec::new();
    flatten(source.iter(), FLATTEN_TOLERANCE, |element| {
        flattened.push(element)
    });

    let mut result = BezPath::new();
    let mut current = None;
    for element in flattened {
        match element {
            PathEl::MoveTo(point) => current = Some(point),
            PathEl::LineTo(point) => {
                if let Some(start) = current {
                    append_clipped_line(
                        &mut result,
                        Line::new(start, point),
                        clip_path,
                        &boundary_segments,
                    );
                }
                current = Some(point);
            }
            PathEl::ClosePath | PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
        }
    }
    result
}

fn append_clipped_line(
    output: &mut BezPath,
    line: Line,
    clip_path: &BezPath,
    boundary_segments: &[kurbo::PathSeg],
) {
    if line.p0.distance(line.p1) <= INTERSECTION_EPSILON {
        return;
    }

    let mut parameters = vec![0.0, 1.0];
    for boundary in boundary_segments {
        parameters.extend(
            boundary
                .intersect_line(line)
                .iter()
                .map(|intersection| intersection.line_t.clamp(0.0, 1.0)),
        );
    }
    parameters.sort_by(f64::total_cmp);
    parameters.dedup_by(|left, right| (*left - *right).abs() <= INTERSECTION_EPSILON);

    for interval in parameters.windows(2) {
        let start_t = interval[0];
        let end_t = interval[1];
        if end_t - start_t <= INTERSECTION_EPSILON {
            continue;
        }
        let midpoint = line.eval((start_t + end_t) * 0.5);
        if clip_path.winding(midpoint) != 0 {
            output.move_to(line.eval(start_t));
            output.line_to(line.eval(end_t));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Ellipse;

    fn assert_all_segment_midpoints_inside(path: &BezPath, clip: &BezPath) {
        for segment in path.segments() {
            assert_ne!(clip.winding(segment.eval(0.5)), 0);
        }
    }

    #[test]
    fn hatch_is_clipped_to_rectangle() {
        let clip = Rect::new(10.0, 20.0, 110.0, 100.0).to_path(0.1);
        let pattern =
            generate_clipped_fill_pattern(FillPattern::Hachure, clip.bounding_box(), &clip, 2.0, 7);
        assert!(!pattern.is_empty());
        assert_all_segment_midpoints_inside(&pattern, &clip);
    }

    #[test]
    fn cross_hatch_is_clipped_to_ellipse() {
        let clip = Ellipse::new((60.0, 60.0), (50.0, 35.0), 0.0).to_path(0.1);
        let pattern = generate_clipped_fill_pattern(
            FillPattern::CrossHatch,
            clip.bounding_box(),
            &clip,
            2.0,
            11,
        );
        assert!(!pattern.is_empty());
        assert_all_segment_midpoints_inside(&pattern, &clip);
    }
}
