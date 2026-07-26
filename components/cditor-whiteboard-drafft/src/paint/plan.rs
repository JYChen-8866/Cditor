use drafftink_core::{
    Canvas,
    shapes::{FillPattern, Freehand, Shape, ShapeStyle, ShapeTrait, StrokeStyle, Text},
};
use kurbo::{Affine, BezPath, Point, Rect, Shape as KurboShape, Size, Vec2};

use super::{
    image::ImagePaintEngine,
    math::math_commands,
    overlay::transient_overlay_commands,
    pattern::generate_clipped_fill_pattern,
    rough::apply_hand_drawn_effect,
    selection::{marquee_commands, selection_commands},
    text_outline::{TextGeometry, TextOutlineEngine},
};
use crate::DrafftBoard;

const GRID_SIZE: f64 = 20.0;
const GRID_COLOR: u32 = 0xc8c8c864;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GridStyle {
    None,
    #[default]
    Lines,
    HorizontalLines,
    CrossPlus,
    Dots,
}

impl GridStyle {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::None => Self::Lines,
            Self::Lines => Self::HorizontalLines,
            Self::HorizontalLines => Self::CrossPlus,
            Self::CrossPlus => Self::Dots,
            Self::Dots => Self::None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum PaintKind {
    Fill,
    Stroke { width: f32, dash: Option<[f32; 2]> },
    Text(TextPaint),
    Image(ImagePaint),
}

#[derive(Clone, Debug)]
pub(crate) struct ImagePaint {
    pub image: std::sync::Arc<gpui::Image>,
    pub bounds: Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct TextPaint {
    pub shape_id: drafftink_core::shapes::ShapeId,
    pub transform: Affine,
    pub geometry: TextGeometry,
    pub editing: Option<TextEditingPaint>,
}

#[derive(Clone, Debug)]
pub(crate) struct TextEditingPaint {
    pub caret: usize,
    pub anchor: usize,
    pub marked_range: Option<std::ops::Range<usize>>,
    pub caret_visible: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PaintCommand {
    pub path: BezPath,
    pub color: u32,
    pub kind: PaintKind,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PaintPlan {
    pub commands: Vec<PaintCommand>,
}

impl PaintPlan {
    pub(crate) fn build_with_selection(
        canvas: &Canvas,
        viewport_size: Size,
        selection_rect: Option<Rect>,
        grid_style: GridStyle,
        text_engine: &mut TextOutlineEngine,
        image_engine: &mut ImagePaintEngine,
    ) -> Self {
        let mut plan = Self::default();
        let camera = canvas.camera.transform();
        let visible = visible_world_bounds(canvas, viewport_size);
        plan.push_grid(visible, camera, canvas.camera.zoom, grid_style);

        for shape in canvas.document.shapes_ordered() {
            if shape.bounds().inflate(24.0, 24.0).overlaps(visible) {
                plan.push_shape(
                    shape,
                    camera,
                    canvas.camera.zoom,
                    canvas.is_selected(shape.id()),
                    text_engine,
                    image_engine,
                );
            }
        }
        if let Some(preview) = canvas.tool_manager.preview_shape() {
            plan.push_shape(
                &preview,
                camera,
                canvas.camera.zoom,
                false,
                text_engine,
                image_engine,
            );
        }
        if let Some(rect) = selection_rect {
            plan.commands.extend(marquee_commands(rect, camera));
        }
        plan
    }

    pub(crate) fn push_transient_overlays(&mut self, board: &DrafftBoard) {
        self.commands.extend(transient_overlay_commands(
            board.eraser_cursor(),
            board.laser_position(),
            board.laser_trail(),
            board.snap_result(),
            board.angle_snap_result(),
            board.smart_guides(),
            board.rotation_feedback(),
            board.canvas.camera.transform(),
            board.canvas.camera.zoom,
        ));
    }

    pub(crate) fn set_text_editing(
        &mut self,
        shape_id: drafftink_core::shapes::ShapeId,
        caret: usize,
        anchor: usize,
        marked_range: Option<std::ops::Range<usize>>,
        caret_visible: bool,
    ) {
        for command in &mut self.commands {
            let PaintKind::Text(text) = &mut command.kind else {
                continue;
            };
            if text.shape_id == shape_id {
                text.editing = Some(TextEditingPaint {
                    caret,
                    anchor,
                    marked_range,
                    caret_visible,
                });
                break;
            }
        }
    }

    fn push_grid(&mut self, visible: Rect, transform: Affine, zoom: f64, style: GridStyle) {
        if style == GridStyle::None {
            return;
        }
        let mut grid = BezPath::new();
        let stride = (8.0 / (GRID_SIZE * zoom.max(0.1))).ceil().max(1.0);
        let spacing = GRID_SIZE * stride;
        let start_x = (visible.x0 / spacing).floor() * spacing;
        let start_y = (visible.y0 / spacing).floor() * spacing;
        let end_x = (visible.x1 / spacing).ceil() * spacing;
        let end_y = (visible.y1 / spacing).ceil() * spacing;

        match style {
            GridStyle::Lines | GridStyle::HorizontalLines => {
                if style == GridStyle::Lines {
                    let mut x = start_x;
                    while x <= end_x {
                        grid.move_to(Point::new(x, start_y));
                        grid.line_to(Point::new(x, end_y));
                        x += spacing;
                    }
                }
                let mut y = start_y;
                while y <= end_y {
                    grid.move_to(Point::new(start_x, y));
                    grid.line_to(Point::new(end_x, y));
                    y += spacing;
                }
            }
            GridStyle::CrossPlus | GridStyle::Dots => {
                let radius = if style == GridStyle::Dots { 0.8 } else { 2.5 } / zoom.max(0.1);
                let mut x = start_x;
                while x <= end_x {
                    let mut y = start_y;
                    while y <= end_y {
                        if style == GridStyle::Dots {
                            grid.move_to(Point::new(x - radius, y - radius));
                            grid.line_to(Point::new(x + radius, y - radius));
                            grid.line_to(Point::new(x + radius, y + radius));
                            grid.line_to(Point::new(x - radius, y + radius));
                            grid.close_path();
                        } else {
                            grid.move_to(Point::new(x - radius, y));
                            grid.line_to(Point::new(x + radius, y));
                            grid.move_to(Point::new(x, y - radius));
                            grid.line_to(Point::new(x, y + radius));
                        }
                        y += spacing;
                    }
                    x += spacing;
                }
            }
            GridStyle::None => {}
        }
        if style == GridStyle::Dots {
            self.push_fill(transform * grid, GRID_COLOR);
        } else {
            self.commands.push(PaintCommand {
                path: transform * grid,
                color: GRID_COLOR,
                kind: PaintKind::Stroke {
                    width: (0.5 * zoom).clamp(0.5, 1.0) as f32,
                    dash: None,
                },
            });
        }
    }

    fn push_shape(
        &mut self,
        shape: &Shape,
        parent_transform: Affine,
        zoom: f64,
        selected: bool,
        text_engine: &mut TextOutlineEngine,
        image_engine: &mut ImagePaintEngine,
    ) {
        let text_geometry = match shape {
            Shape::Text(text) => Some(text_engine.prepare(text)),
            _ => None,
        };
        let shape_transform = rotation_transform(shape, parent_transform);
        if let Shape::Group(group) = shape {
            for child in group.children() {
                self.push_shape(
                    child,
                    shape_transform,
                    zoom,
                    false,
                    text_engine,
                    image_engine,
                );
            }
        } else if let Shape::Freehand(freehand) = shape
            && freehand.has_pressure()
            && !freehand.closed
        {
            let path = pressure_path(freehand);
            self.push_fill(shape_transform * path, rgba(shape.style(), false));
        } else if let Shape::Text(text) = shape {
            self.push_text(
                text,
                shape_transform,
                zoom,
                text_geometry.expect("text geometry prepared"),
                text_engine,
            );
        } else if let Shape::Math(math) = shape {
            self.commands.extend(math_commands(math, parent_transform));
        } else if let Shape::Image(image) = shape {
            if let Some(command) = image_engine.command(image, parent_transform) {
                self.commands.push(command);
            }
        } else {
            let path = shape.to_path();
            let closed = match shape {
                Shape::Rectangle(_) | Shape::Ellipse(_) => true,
                Shape::Line(line) => line.closed,
                Shape::Freehand(freehand) => freehand.closed,
                _ => false,
            };
            self.push_styled_path(
                &path,
                shape.style(),
                stroke_style(shape),
                shape_transform,
                zoom,
                closed,
            );
        }

        if selected {
            self.commands
                .extend(selection_commands(shape, parent_transform));
        }
    }

    fn push_text(
        &mut self,
        text: &Text,
        transform: Affine,
        _zoom: f64,
        geometry: TextGeometry,
        text_engine: &mut TextOutlineEngine,
    ) {
        self.commands.extend(text_engine.commands(text, transform));
        self.commands.push(PaintCommand {
            path: BezPath::new(),
            color: rgba_serialized(text.style.stroke_color, text.style.opacity),
            kind: PaintKind::Text(TextPaint {
                shape_id: text.id(),
                transform: transform
                    * Affine::translate(text.position.to_vec2())
                    * Affine::translate(geometry.origin_offset),
                geometry,
                editing: None,
            }),
        });
    }

    fn push_styled_path(
        &mut self,
        path: &BezPath,
        style: &ShapeStyle,
        stroke_style: StrokeStyle,
        transform: Affine,
        zoom: f64,
        closed: bool,
    ) {
        let roughness = style.sloppiness.roughness();
        let scale = transform_scale(transform);

        if closed && let Some(fill_color) = style.fill_color {
            let fill_path = if roughness > 0.0 {
                apply_hand_drawn_effect(path, roughness * 0.3, zoom, style.seed, 0)
            } else {
                path.clone()
            };
            let color = rgba_serialized(fill_color, style.opacity);
            match style.fill_pattern {
                FillPattern::Solid => self.push_fill(transform * fill_path, color),
                pattern => {
                    self.push_fill(
                        transform * fill_path.clone(),
                        with_alpha_factor(color, 0.15),
                    );
                    let pattern_path = generate_clipped_fill_pattern(
                        pattern,
                        path.bounding_box(),
                        &fill_path,
                        style.stroke_width,
                        style.seed,
                    );
                    self.push_stroke(
                        transform * pattern_path,
                        color,
                        (style.stroke_width * 0.5 * scale) as f32,
                        StrokeStyle::Solid,
                    );
                }
            }
        }

        let stroke_color = rgba(style, false);
        let stroke_width = (style.stroke_width * scale) as f32;
        if roughness > 0.0 {
            for stroke_index in 0..2 {
                self.push_stroke(
                    transform
                        * apply_hand_drawn_effect(path, roughness, zoom, style.seed, stroke_index),
                    stroke_color,
                    stroke_width,
                    stroke_style,
                );
            }
        } else {
            self.push_stroke(
                transform * path.clone(),
                stroke_color,
                stroke_width,
                stroke_style,
            );
        }
    }

    fn push_fill(&mut self, path: BezPath, color: u32) {
        self.commands.push(PaintCommand {
            path,
            color,
            kind: PaintKind::Fill,
        });
    }

    fn push_stroke(&mut self, path: BezPath, color: u32, width: f32, style: StrokeStyle) {
        let width = width.max(0.5);
        let dash = match style {
            StrokeStyle::Solid => None,
            StrokeStyle::Dashed => Some([width * 4.0, width * 2.0]),
            StrokeStyle::Dotted => Some([width, width * 2.0]),
        };
        self.commands.push(PaintCommand {
            path,
            color,
            kind: PaintKind::Stroke { width, dash },
        });
    }
}

fn visible_world_bounds(canvas: &Canvas, viewport_size: Size) -> Rect {
    let top_left = canvas.camera.screen_to_world(Point::ZERO);
    let bottom_right = canvas
        .camera
        .screen_to_world(Point::new(viewport_size.width, viewport_size.height));
    Rect::new(
        top_left.x.min(bottom_right.x),
        top_left.y.min(bottom_right.y),
        top_left.x.max(bottom_right.x),
        top_left.y.max(bottom_right.y),
    )
}

pub(crate) fn rotation_transform(shape: &Shape, parent: Affine) -> Affine {
    let rotation = shape.rotation();
    if rotation.abs() <= 0.001 {
        return parent;
    }
    let center = shape.bounds().center();
    let center_vector = Vec2::new(center.x, center.y);
    parent
        * Affine::translate(center_vector)
        * Affine::rotate(rotation)
        * Affine::translate(-center_vector)
}

fn transform_scale(transform: Affine) -> f64 {
    let coefficients = transform.as_coeffs();
    (coefficients[0].hypot(coefficients[1]) + coefficients[2].hypot(coefficients[3])) / 2.0
}

fn stroke_style(shape: &Shape) -> StrokeStyle {
    match shape {
        Shape::Line(line) => line.stroke_style,
        Shape::Arrow(arrow) => arrow.stroke_style,
        _ => StrokeStyle::Solid,
    }
}

fn rgba(style: &ShapeStyle, fill: bool) -> u32 {
    let value = if fill {
        style.fill_color.unwrap_or(style.stroke_color)
    } else {
        style.stroke_color
    };
    rgba_serialized(value, style.opacity)
}

fn rgba_serialized(color: drafftink_core::shapes::SerializableColor, opacity: f64) -> u32 {
    let alpha = ((color.a as f64 * opacity.clamp(0.0, 1.0)).round() as u8) as u32;
    ((color.r as u32) << 24) | ((color.g as u32) << 16) | ((color.b as u32) << 8) | alpha
}

fn with_alpha_factor(color: u32, factor: f64) -> u32 {
    let alpha = color & 0xff;
    (color & 0xffffff00) | ((alpha as f64 * factor).round() as u32).min(0xff)
}

// Ported from Drafft's Vello renderer so pressure behavior stays identical.
fn pressure_path(freehand: &Freehand) -> BezPath {
    if freehand.points.len() < 2 {
        return freehand.to_path();
    }
    let base_size = freehand.style.stroke_width * 2.3;
    let mut left = Vec::with_capacity(freehand.points.len());
    let mut right = Vec::with_capacity(freehand.points.len());

    for (index, point) in freehand.points.iter().copied().enumerate() {
        let pressure = freehand.pressure_at(index);
        let eased = (pressure * std::f64::consts::PI / 2.0).sin();
        let width = base_size * (1.0 - 0.6 * (1.0 - eased));
        let direction = if index == 0 {
            freehand.points[1] - point
        } else if index + 1 == freehand.points.len() {
            point - freehand.points[index - 1]
        } else {
            freehand.points[index + 1] - freehand.points[index - 1]
        };
        let length = direction.hypot();
        if length <= f64::EPSILON {
            continue;
        }
        let normal = Vec2::new(-direction.y / length, direction.x / length) * (width / 2.0);
        left.push(point + normal);
        right.push(point - normal);
    }

    let mut path = BezPath::new();
    if let Some(first) = left.first().copied() {
        path.move_to(first);
        for point in left.iter().skip(1) {
            path.line_to(*point);
        }
        for point in right.iter().rev() {
            path.line_to(*point);
        }
        path.close_path();
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use drafftink_core::shapes::{Rectangle, Shape, Sloppiness};

    #[test]
    fn plan_culls_shapes_outside_viewport() {
        let mut text_engine = TextOutlineEngine::new();
        let mut image_engine = ImagePaintEngine::default();
        let mut canvas = Canvas::new();
        canvas.document.add_shape(Shape::Rectangle(Rectangle::new(
            Point::new(100_000.0, 100_000.0),
            100.0,
            100.0,
        )));

        let plan = PaintPlan::build_with_selection(
            &canvas,
            Size::new(800.0, 600.0),
            None,
            GridStyle::Lines,
            &mut text_engine,
            &mut image_engine,
        );
        assert_eq!(plan.commands.len(), 1, "only the grid should be emitted");
    }

    #[test]
    fn architect_shape_emits_one_stroke_and_artist_shape_emits_two() {
        let mut text_engine = TextOutlineEngine::new();
        let mut image_engine = ImagePaintEngine::default();
        let mut architect_canvas = Canvas::new();
        let mut architect = Rectangle::new(Point::new(10.0, 10.0), 100.0, 80.0);
        architect.style.sloppiness = Sloppiness::Architect;
        architect_canvas
            .document
            .add_shape(Shape::Rectangle(architect));

        let mut artist_canvas = Canvas::new();
        let mut artist = Rectangle::new(Point::new(10.0, 10.0), 100.0, 80.0);
        artist.style.sloppiness = Sloppiness::Artist;
        artist_canvas.document.add_shape(Shape::Rectangle(artist));

        let viewport = Size::new(800.0, 600.0);
        assert_eq!(
            PaintPlan::build_with_selection(
                &architect_canvas,
                viewport,
                None,
                GridStyle::Lines,
                &mut text_engine,
                &mut image_engine,
            )
            .commands
            .len(),
            2
        );
        assert_eq!(
            PaintPlan::build_with_selection(
                &artist_canvas,
                viewport,
                None,
                GridStyle::Lines,
                &mut text_engine,
                &mut image_engine,
            )
            .commands
            .len(),
            3
        );
    }
}
