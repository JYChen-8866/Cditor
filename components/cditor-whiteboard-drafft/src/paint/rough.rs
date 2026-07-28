use kurbo::{BezPath, PathEl, Point};

/// Deterministic xorshift32 generator copied from Drafft's Vello renderer.
struct SimpleRng {
    state: u32,
}

impl SimpleRng {
    fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u32(&mut self) -> u32 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.state = value;
        value
    }

    fn offset(&mut self, amount: f64) -> f64 {
        let normalized = (self.next_u32() as f64 / u32::MAX as f64) * 2.0 - 1.0;
        normalized * amount
    }
}

/// Ported verbatim in behavior from Drafft's Vello renderer so the GPUI and
/// upstream renderers produce the same stable hand-drawn geometry.
pub(super) fn apply_hand_drawn_effect(
    path: &BezPath,
    roughness: f64,
    zoom: f64,
    seed: u32,
    stroke_index: u32,
) -> BezPath {
    if roughness <= 0.0 {
        return path.clone();
    }

    let scale = 1.0 / zoom.max(f64::EPSILON).sqrt();
    let max_randomness_offset = roughness * 2.0 * scale;
    let bowing = roughness;
    let combined_seed = seed.wrapping_add(stroke_index.wrapping_mul(99_991));
    let mut rng = SimpleRng::new(combined_seed);
    let mut result = BezPath::new();
    let mut last_point = Point::ZERO;

    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) => {
                result.move_to(Point::new(
                    point.x + rng.offset(max_randomness_offset),
                    point.y + rng.offset(max_randomness_offset),
                ));
                last_point = point;
            }
            PathEl::LineTo(point) => {
                let dx = point.x - last_point.x;
                let dy = point.y - last_point.y;
                let length = dx.hypot(dy);
                let bow_offset = bowing * roughness * length / 200.0;
                let bow = rng.offset(bow_offset) * scale;
                let (perpendicular_x, perpendicular_y) = if length > 0.001 {
                    (-dy / length, dx / length)
                } else {
                    (0.0, 0.0)
                };
                let control = Point::new(
                    (last_point.x + point.x) / 2.0 + perpendicular_x * bow,
                    (last_point.y + point.y) / 2.0 + perpendicular_y * bow,
                );
                let end = Point::new(
                    point.x + rng.offset(max_randomness_offset),
                    point.y + rng.offset(max_randomness_offset),
                );
                result.quad_to(control, end);
                last_point = point;
            }
            PathEl::QuadTo(control, end) => {
                result.quad_to(
                    Point::new(
                        control.x + rng.offset(max_randomness_offset * 0.7),
                        control.y + rng.offset(max_randomness_offset * 0.7),
                    ),
                    Point::new(
                        end.x + rng.offset(max_randomness_offset),
                        end.y + rng.offset(max_randomness_offset),
                    ),
                );
                last_point = end;
            }
            PathEl::CurveTo(control_a, control_b, end) => {
                result.curve_to(
                    Point::new(
                        control_a.x + rng.offset(max_randomness_offset * 0.5),
                        control_a.y + rng.offset(max_randomness_offset * 0.5),
                    ),
                    Point::new(
                        control_b.x + rng.offset(max_randomness_offset * 0.5),
                        control_b.y + rng.offset(max_randomness_offset * 0.5),
                    ),
                    Point::new(
                        end.x + rng.offset(max_randomness_offset),
                        end.y + rng.offset(max_randomness_offset),
                    ),
                );
                last_point = end;
            }
            PathEl::ClosePath => result.close_path(),
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Rect, Shape as _};

    #[test]
    fn rough_geometry_is_deterministic_for_a_stable_seed() {
        let path = Rect::new(0.0, 0.0, 100.0, 80.0).to_path(0.1);
        assert_eq!(
            apply_hand_drawn_effect(&path, 1.0, 1.0, 42, 0),
            apply_hand_drawn_effect(&path, 1.0, 1.0, 42, 0)
        );
    }

    #[test]
    fn second_stroke_uses_distinct_geometry() {
        let path = Rect::new(0.0, 0.0, 100.0, 80.0).to_path(0.1);
        assert_ne!(
            apply_hand_drawn_effect(&path, 1.0, 1.0, 42, 0),
            apply_hand_drawn_effect(&path, 1.0, 1.0, 42, 1)
        );
    }
}
