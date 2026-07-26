//! Shape definitions for the whiteboard.

mod arrow;
mod ellipse;
mod freehand;
mod group;
mod image;
mod line;
mod math;
mod rectangle;
mod text;

pub use arrow::Arrow;
pub use ellipse::Ellipse;
pub use freehand::Freehand;
pub use group::Group;
pub use image::{Image, ImageFormat};
pub use line::{Line, PathStyle};
pub use math::Math;
pub use rectangle::Rectangle;
pub use text::{FontFamily, FontWeight, Text};

use kurbo::{Affine, BezPath, Point, Rect};
use peniko::Color;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Serializable color representation (RGBA8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerializableColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl SerializableColor {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn black() -> Self {
        Self::new(0, 0, 0, 255)
    }

    pub fn white() -> Self {
        Self::new(255, 255, 255, 255)
    }

    pub fn transparent() -> Self {
        Self::new(0, 0, 0, 0)
    }
}

impl From<Color> for SerializableColor {
    fn from(color: Color) -> Self {
        let rgba = color.to_rgba8();
        Self {
            r: rgba.r,
            g: rgba.g,
            b: rgba.b,
            a: rgba.a,
        }
    }
}

impl From<SerializableColor> for Color {
    fn from(color: SerializableColor) -> Self {
        Color::from_rgba8(color.r, color.g, color.b, color.a)
    }
}

/// Sloppiness level for hand-drawn effect.
/// Based on Excalidraw's roughness algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Sloppiness {
    /// No roughness - clean, precise lines (roughness = 0)
    Architect = 0,
    /// Medium roughness - slight hand-drawn feel (roughness = 1)
    #[default]
    Artist = 1,
    /// High roughness - very sketchy, cartoon-like (roughness = 2)
    Cartoonist = 2,
    /// Extreme roughness - chaotic, wild strokes (roughness = 3.5)
    Drunk = 3,
}

impl Sloppiness {
    /// Get the roughness value for this sloppiness level.
    pub fn roughness(&self) -> f64 {
        match self {
            Sloppiness::Architect => 0.0,
            Sloppiness::Artist => 1.0,
            Sloppiness::Cartoonist => 2.0,
            Sloppiness::Drunk => 3.5,
        }
    }

    /// Cycle to the next sloppiness level.
    pub fn next(self) -> Self {
        match self {
            Sloppiness::Architect => Sloppiness::Artist,
            Sloppiness::Artist => Sloppiness::Cartoonist,
            Sloppiness::Cartoonist => Sloppiness::Drunk,
            Sloppiness::Drunk => Sloppiness::Architect,
        }
    }
}

/// Fill pattern style for shapes (inspired by roughjs/roughr).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FillPattern {
    /// Solid fill color.
    #[default]
    Solid,
    /// Parallel diagonal lines.
    Hachure,
    /// Zigzag pattern.
    ZigZag,
    /// Cross-hatched lines.
    CrossHatch,
    /// Dot pattern.
    Dots,
    /// Dashed lines.
    Dashed,
    /// Zigzag line pattern.
    ZigZagLine,
}

impl FillPattern {
    /// Cycle to the next fill pattern.
    pub fn next(self) -> Self {
        match self {
            FillPattern::Solid => FillPattern::Hachure,
            FillPattern::Hachure => FillPattern::ZigZag,
            FillPattern::ZigZag => FillPattern::CrossHatch,
            FillPattern::CrossHatch => FillPattern::Dots,
            FillPattern::Dots => FillPattern::Dashed,
            FillPattern::Dashed => FillPattern::ZigZagLine,
            FillPattern::ZigZagLine => FillPattern::Solid,
        }
    }
}

/// Stroke style for lines and arrows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StrokeStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl StrokeStyle {
    /// Cycle to the next stroke style.
    pub fn next(self) -> Self {
        match self {
            StrokeStyle::Solid => StrokeStyle::Dashed,
            StrokeStyle::Dashed => StrokeStyle::Dotted,
            StrokeStyle::Dotted => StrokeStyle::Solid,
        }
    }
}

/// Style properties for shapes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapeStyle {
    /// Stroke color.
    pub stroke_color: SerializableColor,
    /// Stroke width.
    pub stroke_width: f64,
    /// Fill color (None = no fill).
    pub fill_color: Option<SerializableColor>,
    /// Fill pattern style.
    #[serde(default)]
    pub fill_pattern: FillPattern,
    /// Sloppiness level for hand-drawn effect.
    pub sloppiness: Sloppiness,
    /// Random seed for hand-drawn effect (ensures consistent rendering across transforms).
    #[serde(default = "generate_seed")]
    pub seed: u32,
    /// Overall opacity (0.0 = fully transparent, 1.0 = fully opaque).
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

fn default_opacity() -> f64 {
    1.0
}

/// Generate a random seed for new shapes.
/// Uses a simple counter + hash approach that works on all platforms including WASM.
fn generate_seed() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};

    // Global counter for seed generation - ensures uniqueness even without time
    static SEED_COUNTER: AtomicU32 = AtomicU32::new(1);

    // Get next counter value
    let counter = SEED_COUNTER.fetch_add(1, Ordering::Relaxed);

    // Mix the counter with some constants for better distribution
    // Using a simple hash function (similar to splitmix32)
    let mut x = counter.wrapping_mul(0x9E3779B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EBCA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2AE35);
    x ^= x >> 16;
    x
}

impl ShapeStyle {
    /// Get the stroke color as a peniko Color.
    pub fn stroke(&self) -> Color {
        self.stroke_color.into()
    }

    /// Get the stroke color with opacity applied.
    pub fn stroke_with_opacity(&self) -> Color {
        let color: Color = self.stroke_color.into();
        let rgba = color.to_rgba8();
        let alpha = (rgba.a as f64 * self.opacity) as u8;
        Color::from_rgba8(rgba.r, rgba.g, rgba.b, alpha)
    }

    /// Get the fill color as a peniko Color.
    pub fn fill(&self) -> Option<Color> {
        self.fill_color.map(|c| c.into())
    }

    /// Get the fill color with opacity applied.
    pub fn fill_with_opacity(&self) -> Option<Color> {
        self.fill_color.map(|c| {
            let color: Color = c.into();
            let rgba = color.to_rgba8();
            let alpha = (rgba.a as f64 * self.opacity) as u8;
            Color::from_rgba8(rgba.r, rgba.g, rgba.b, alpha)
        })
    }

    /// Set the stroke color from a peniko Color.
    pub fn set_stroke(&mut self, color: Color) {
        self.stroke_color = color.into();
    }

    /// Set the fill color from a peniko Color.
    pub fn set_fill(&mut self, color: Option<Color>) {
        self.fill_color = color.map(|c| c.into());
    }
}

impl Default for ShapeStyle {
    fn default() -> Self {
        Self {
            stroke_color: SerializableColor::black(),
            stroke_width: 2.0,
            fill_color: None,
            fill_pattern: FillPattern::default(),
            sloppiness: Sloppiness::default(),
            seed: generate_seed(),
            opacity: 1.0,
        }
    }
}

/// Unique identifier for shapes.
pub type ShapeId = Uuid;

/// Distance from a point to a line segment (a→b).
pub fn point_to_segment_dist(point: Point, a: Point, b: Point) -> f64 {
    let seg = kurbo::Vec2::new(b.x - a.x, b.y - a.y);
    let pv = kurbo::Vec2::new(point.x - a.x, point.y - a.y);
    let len_sq = seg.hypot2();
    if len_sq < f64::EPSILON {
        return pv.hypot();
    }
    let t = (pv.dot(seg) / len_sq).clamp(0.0, 1.0);
    let proj = Point::new(a.x + t * seg.x, a.y + t * seg.y);
    ((point.x - proj.x).powi(2) + (point.y - proj.y).powi(2)).sqrt()
}

/// Minimum distance from a point to a polyline (sequence of connected segments).
pub fn point_to_polyline_dist(point: Point, points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|w| point_to_segment_dist(point, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

/// Common trait for all shapes.
pub trait ShapeTrait {
    /// Get the unique identifier.
    fn id(&self) -> ShapeId;

    /// Get the bounding box in world coordinates.
    fn bounds(&self) -> Rect;

    /// Check if a point (in world coordinates) hits this shape.
    fn hit_test(&self, point: Point, tolerance: f64) -> bool;

    /// Get the path representation for rendering.
    fn to_path(&self) -> BezPath;

    /// Get the style.
    fn style(&self) -> &ShapeStyle;

    /// Get mutable style.
    fn style_mut(&mut self) -> &mut ShapeStyle;

    /// Apply a transform to this shape.
    fn transform(&mut self, affine: Affine);

    /// Clone this shape into a boxed trait object.
    fn clone_box(&self) -> Box<dyn ShapeTrait + Send + Sync>;
}

/// Enum wrapper for all shape types (for serialization).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Shape {
    Rectangle(Rectangle),
    Ellipse(Ellipse),
    Line(Line),
    Arrow(Arrow),
    Freehand(Freehand),
    Text(Text),
    Group(Group),
    Image(Image),
    Math(Math),
}

impl Shape {
    pub fn id(&self) -> ShapeId {
        match self {
            Shape::Rectangle(s) => s.id(),
            Shape::Ellipse(s) => s.id(),
            Shape::Line(s) => s.id(),
            Shape::Arrow(s) => s.id(),
            Shape::Freehand(s) => s.id(),
            Shape::Text(s) => s.id(),
            Shape::Group(s) => s.id(),
            Shape::Image(s) => s.id(),
            Shape::Math(s) => s.id(),
        }
    }

    pub fn bounds(&self) -> Rect {
        match self {
            Shape::Rectangle(s) => s.bounds(),
            Shape::Ellipse(s) => s.bounds(),
            Shape::Line(s) => s.bounds(),
            Shape::Arrow(s) => s.bounds(),
            Shape::Freehand(s) => s.bounds(),
            Shape::Text(s) => s.bounds(),
            Shape::Group(s) => s.bounds(),
            Shape::Image(s) => s.bounds(),
            Shape::Math(s) => s.bounds(),
        }
    }

    /// Return key snap points for smart guide alignment.
    /// For lines/arrows, returns all segment endpoints.
    /// For other shapes, returns an empty vec (bounds suffice).
    pub fn snap_points(&self) -> Vec<Point> {
        match self {
            Shape::Line(s) => s.all_points(),
            Shape::Arrow(s) => s.all_points(),
            _ => Vec::new(),
        }
    }

    /// Test if this shape intersects a selection rectangle.
    /// For lines/arrows, checks if any segment crosses or is inside the rect.
    /// For other shapes, checks bounding box intersection.
    pub fn intersects_rect(&self, rect: Rect) -> bool {
        match self {
            Shape::Line(s) => line_segments_intersect_rect(&s.all_points(), rect),
            Shape::Arrow(s) => line_segments_intersect_rect(&s.all_points(), rect),
            _ => {
                let bounds = self.bounds();
                rect.intersect(bounds.inflate(1.0, 1.0)).area() > 0.0
            }
        }
    }

    pub fn hit_test(&self, point: Point, tolerance: f64) -> bool {
        match self {
            Shape::Rectangle(s) => s.hit_test(point, tolerance),
            Shape::Ellipse(s) => s.hit_test(point, tolerance),
            Shape::Line(s) => s.hit_test(point, tolerance),
            Shape::Arrow(s) => s.hit_test(point, tolerance),
            Shape::Freehand(s) => s.hit_test(point, tolerance),
            Shape::Text(s) => s.hit_test(point, tolerance),
            Shape::Group(s) => s.hit_test(point, tolerance),
            Shape::Image(s) => s.hit_test(point, tolerance),
            Shape::Math(s) => s.hit_test(point, tolerance),
        }
    }

    pub fn to_path(&self) -> BezPath {
        match self {
            Shape::Rectangle(s) => s.to_path(),
            Shape::Ellipse(s) => s.to_path(),
            Shape::Line(s) => s.to_path(),
            Shape::Arrow(s) => s.to_path(),
            Shape::Freehand(s) => s.to_path(),
            Shape::Text(s) => s.to_path(),
            Shape::Group(s) => s.to_path(),
            Shape::Image(s) => s.to_path(),
            Shape::Math(s) => s.to_path(),
        }
    }

    pub fn style(&self) -> &ShapeStyle {
        match self {
            Shape::Rectangle(s) => s.style(),
            Shape::Ellipse(s) => s.style(),
            Shape::Line(s) => s.style(),
            Shape::Arrow(s) => s.style(),
            Shape::Freehand(s) => s.style(),
            Shape::Text(s) => s.style(),
            Shape::Group(s) => s.style(),
            Shape::Image(s) => s.style(),
            Shape::Math(s) => s.style(),
        }
    }

    pub fn style_mut(&mut self) -> &mut ShapeStyle {
        match self {
            Shape::Rectangle(s) => s.style_mut(),
            Shape::Ellipse(s) => s.style_mut(),
            Shape::Line(s) => s.style_mut(),
            Shape::Arrow(s) => s.style_mut(),
            Shape::Freehand(s) => s.style_mut(),
            Shape::Text(s) => s.style_mut(),
            Shape::Group(s) => s.style_mut(),
            Shape::Image(s) => s.style_mut(),
            Shape::Math(s) => s.style_mut(),
        }
    }

    pub fn transform(&mut self, affine: Affine) {
        match self {
            Shape::Rectangle(s) => s.transform(affine),
            Shape::Ellipse(s) => s.transform(affine),
            Shape::Line(s) => s.transform(affine),
            Shape::Arrow(s) => s.transform(affine),
            Shape::Freehand(s) => s.transform(affine),
            Shape::Text(s) => s.transform(affine),
            Shape::Group(s) => s.transform(affine),
            Shape::Image(s) => s.transform(affine),
            Shape::Math(s) => s.transform(affine),
        }
    }

    /// Check if this shape is a group.
    pub fn is_group(&self) -> bool {
        matches!(self, Shape::Group(_))
    }

    /// Get the group if this shape is a group.
    pub fn as_group(&self) -> Option<&Group> {
        match self {
            Shape::Group(g) => Some(g),
            _ => None,
        }
    }

    /// Get the mutable group if this shape is a group.
    pub fn as_group_mut(&mut self) -> Option<&mut Group> {
        match self {
            Shape::Group(g) => Some(g),
            _ => None,
        }
    }

    /// Regenerate the shape's ID with a new unique identifier.
    /// This is used when duplicating or pasting shapes to ensure they have unique IDs.
    pub fn regenerate_id(&mut self) {
        let new_id = Uuid::new_v4();
        match self {
            Shape::Rectangle(s) => s.id = new_id,
            Shape::Ellipse(s) => s.id = new_id,
            Shape::Line(s) => s.id = new_id,
            Shape::Arrow(s) => s.id = new_id,
            Shape::Freehand(s) => s.id = new_id,
            Shape::Text(s) => s.id = new_id,
            Shape::Group(s) => s.id = new_id,
            Shape::Image(s) => s.id = new_id,
            Shape::Math(s) => s.id = new_id,
        }
    }

    /// Check if this shape is an image.
    pub fn is_image(&self) -> bool {
        matches!(self, Shape::Image(_))
    }

    /// Get the image if this shape is an image.
    pub fn as_image(&self) -> Option<&Image> {
        match self {
            Shape::Image(img) => Some(img),
            _ => None,
        }
    }

    /// Get the rotation angle in radians (0 for shapes that don't support rotation).
    pub fn rotation(&self) -> f64 {
        match self {
            Shape::Rectangle(r) => r.rotation,
            Shape::Ellipse(e) => e.rotation,
            Shape::Text(t) => t.rotation,
            Shape::Image(i) => i.rotation,
            Shape::Math(m) => m.rotation,
            Shape::Group(g) => g.rotation,
            _ => 0.0,
        }
    }

    /// Set the rotation angle in radians.
    pub fn set_rotation(&mut self, rotation: f64) {
        match self {
            Shape::Rectangle(r) => r.rotation = rotation,
            Shape::Ellipse(e) => e.rotation = rotation,
            Shape::Text(t) => t.rotation = rotation,
            Shape::Image(i) => i.rotation = rotation,
            Shape::Math(m) => m.rotation = rotation,
            Shape::Group(g) => g.rotation = rotation,
            _ => {}
        }
    }

    /// Check if this shape supports rotation.
    pub fn supports_rotation(&self) -> bool {
        matches!(
            self,
            Shape::Rectangle(_)
                | Shape::Ellipse(_)
                | Shape::Text(_)
                | Shape::Image(_)
                | Shape::Math(_)
                | Shape::Group(_)
        )
    }
}

/// Test if any line segment (defined by consecutive points) intersects or is inside a rectangle.
fn line_segments_intersect_rect(points: &[Point], rect: Rect) -> bool {
    // Any point inside the rect?
    if points.iter().any(|p| rect.contains(*p)) {
        return true;
    }
    // Any segment crosses a rect edge?
    let corners = [
        Point::new(rect.x0, rect.y0),
        Point::new(rect.x1, rect.y0),
        Point::new(rect.x1, rect.y1),
        Point::new(rect.x0, rect.y1),
    ];
    let edges = [
        (corners[0], corners[1]),
        (corners[1], corners[2]),
        (corners[2], corners[3]),
        (corners[3], corners[0]),
    ];
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        for &(c, d) in &edges {
            if segments_intersect(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

/// Test if two line segments (a-b) and (c-d) intersect.
fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let cross = |o: Point, p: Point, q: Point| -> f64 {
        (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x)
    };
    let d1 = cross(c, d, a);
    let d2 = cross(c, d, b);
    let d3 = cross(a, b, c);
    let d4 = cross(a, b, d);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    // Collinear cases: check if endpoint lies on the other segment
    let on_segment = |p: Point, q: Point, r: Point| -> bool {
        r.x >= p.x.min(q.x) && r.x <= p.x.max(q.x) && r.y >= p.y.min(q.y) && r.y <= p.y.max(q.y)
    };
    (d1.abs() < 1e-10 && on_segment(c, d, a))
        || (d2.abs() < 1e-10 && on_segment(c, d, b))
        || (d3.abs() < 1e-10 && on_segment(a, b, c))
        || (d4.abs() < 1e-10 && on_segment(a, b, d))
}
