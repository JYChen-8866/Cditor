use std::collections::HashMap;

use crate::shapes::{Math, SerializableColor, ShapeTrait};
use kurbo::{Affine, BezPath, Rect, Shape as KurboShape, Vec2};
use rex::{
    font::{backend::ttf_parser::TtfMathFont, common::GlyphId},
    layout::engine::LayoutBuilder,
    render::{Backend, Cursor, FontBackend, GraphicsBackend, RGBA, Renderer},
};

use super::plan::{PaintCommand, PaintKind};

const XITS_MATH: &[u8] = include_bytes!("../../assets/rex-xits.otf");
const GELPEN_REGULAR: &[u8] = include_bytes!("../../assets/GelPen.ttf");

pub(super) fn math_commands(math: &Math, parent_transform: Affine) -> Vec<PaintCommand> {
    let Ok(math_face) = ttf_parser::Face::parse(XITS_MATH, 0) else {
        return error_commands(math, parent_transform);
    };
    let Ok(math_font) = TtfMathFont::new(math_face) else {
        return error_commands(math, parent_transform);
    };
    let primary_face = ttf_parser::Face::parse(GELPEN_REGULAR, 0).ok();
    let Ok(parse_nodes) = rex::parser::parse(&math.latex) else {
        return error_commands(math, parent_transform);
    };
    let engine = LayoutBuilder::new(&math_font)
        .font_size(math.font_size)
        .build();
    let Ok(layout) = engine.layout(&parse_nodes) else {
        return error_commands(math, parent_transform);
    };

    let size = layout.size();
    math.set_cached_size(size.width, size.height, size.depth);
    let transform =
        math_render_transform(math, size.width, size.height, size.depth, parent_transform);
    let color = serialized_color(math.style.stroke_color, math.style.opacity);
    let mut backend = GpuiMathBackend::new(&math_font, primary_face.as_ref(), transform, color);
    Renderer::new().render(&layout, &mut backend);
    backend.commands
}

fn math_render_transform(
    math: &Math,
    width: f64,
    height: f64,
    depth: f64,
    parent_transform: Affine,
) -> Affine {
    let center_x = math.position.x + width / 2.0;
    let center_y = math.position.y - height / 2.0 - depth / 2.0;
    parent_transform
        * Affine::rotate_about(math.rotation, kurbo::Point::new(center_x, center_y))
        * Affine::translate(math.position.to_vec2())
}

fn error_commands(math: &Math, transform: Affine) -> Vec<PaintCommand> {
    let path = transform * math.bounds().to_path(0.1);
    vec![
        PaintCommand {
            path: path.clone(),
            color: 0xffc8c864,
            kind: PaintKind::Fill,
        },
        PaintCommand {
            path,
            color: 0xff6464ff,
            kind: PaintKind::Stroke {
                width: 1.0,
                dash: None,
            },
        },
    ]
}

struct GpuiMathBackend<'f, 'p> {
    math_font: &'f TtfMathFont<'f>,
    primary_font: Option<&'p ttf_parser::Face<'p>>,
    glyph_to_codepoint: HashMap<u16, char>,
    transform: Affine,
    color_stack: Vec<u32>,
    current_color: u32,
    commands: Vec<PaintCommand>,
}

impl<'f, 'p> GpuiMathBackend<'f, 'p> {
    fn new(
        math_font: &'f TtfMathFont<'f>,
        primary_font: Option<&'p ttf_parser::Face<'p>>,
        transform: Affine,
        color: u32,
    ) -> Self {
        let mut glyph_to_codepoint = HashMap::new();
        if primary_font.is_some() {
            for subtable in math_font
                .font()
                .tables()
                .cmap
                .iter()
                .flat_map(|cmap| cmap.subtables)
            {
                if subtable.is_unicode() {
                    subtable.codepoints(|codepoint| {
                        if let Some(character) = char::from_u32(codepoint)
                            && let Some(glyph) = subtable.glyph_index(codepoint)
                        {
                            glyph_to_codepoint.insert(glyph.0, character);
                        }
                    });
                }
            }
        }
        Self {
            math_font,
            primary_font,
            glyph_to_codepoint,
            transform,
            color_stack: Vec::new(),
            current_color: color,
            commands: Vec::new(),
        }
    }

    fn push_fill(&mut self, path: BezPath, transform: Affine) {
        self.commands.push(PaintCommand {
            path: transform * path,
            color: self.current_color,
            kind: PaintKind::Fill,
        });
    }
}

struct OutlinePath(BezPath);

impl ttf_parser::OutlineBuilder for OutlinePath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to((x1 as f64, y1 as f64), (x as f64, y as f64));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.curve_to(
            (x1 as f64, y1 as f64),
            (x2 as f64, y2 as f64),
            (x as f64, y as f64),
        );
    }

    fn close(&mut self) {
        self.0.close_path();
    }
}

impl<'f, 'p> FontBackend<TtfMathFont<'f>> for GpuiMathBackend<'f, 'p> {
    fn symbol(&mut self, position: Cursor, glyph_id: GlyphId, scale: f64, _: &TtfMathFont<'f>) {
        if let Some(primary) = self.primary_font
            && let Some(codepoint) = self.glyph_to_codepoint.get(&glyph_id.into()).copied()
            && let Some(primary_id) =
                primary.glyph_index(math_to_ascii(codepoint).unwrap_or(codepoint))
        {
            let units_per_em = primary.units_per_em() as f64;
            let adjusted_scale = scale * 0.75;
            let transform = self.transform
                * Affine::translate(Vec2::new(position.x, position.y))
                * Affine::scale_non_uniform(
                    adjusted_scale / units_per_em,
                    -adjusted_scale / units_per_em,
                );
            let mut path = OutlinePath(BezPath::new());
            if primary.outline_glyph(primary_id, &mut path).is_some() {
                self.push_fill(path.0, transform);
                return;
            }
        }

        let matrix = self.math_font.font_matrix();
        let font_matrix = Affine::new([
            matrix.sx as f64,
            matrix.ky as f64,
            matrix.kx as f64,
            matrix.sy as f64,
            matrix.tx as f64,
            matrix.ty as f64,
        ]);
        let transform = self.transform
            * Affine::translate(Vec2::new(position.x, position.y))
            * Affine::scale_non_uniform(scale, -scale)
            * font_matrix;
        let mut path = OutlinePath(BezPath::new());
        self.math_font
            .font()
            .outline_glyph(glyph_id.into(), &mut path);
        self.push_fill(path.0, transform);
    }
}

impl GraphicsBackend for GpuiMathBackend<'_, '_> {
    fn rule(&mut self, position: Cursor, width: f64, height: f64) {
        let path = Rect::new(
            position.x,
            position.y,
            position.x + width,
            position.y + height,
        )
        .to_path(0.1);
        self.push_fill(path, self.transform);
    }

    fn begin_color(&mut self, RGBA(red, green, blue, alpha): RGBA) {
        self.color_stack.push(self.current_color);
        let inherited_alpha = self.current_color & 0xff;
        self.current_color = ((red as u32) << 24)
            | ((green as u32) << 16)
            | ((blue as u32) << 8)
            | ((alpha as u32 * inherited_alpha) / 255);
    }

    fn end_color(&mut self) {
        if let Some(color) = self.color_stack.pop() {
            self.current_color = color;
        }
    }
}

impl<'f, 'p> Backend<TtfMathFont<'f>> for GpuiMathBackend<'f, 'p> {}

fn serialized_color(color: SerializableColor, opacity: f64) -> u32 {
    let alpha = (color.a as f64 * opacity.clamp(0.0, 1.0)).round() as u32;
    ((color.r as u32) << 24) | ((color.g as u32) << 16) | ((color.b as u32) << 8) | alpha.min(255)
}

fn math_to_ascii(character: char) -> Option<char> {
    let codepoint = character as u32;
    match codepoint {
        0x1D434..=0x1D44D => Some((b'A' + (codepoint - 0x1D434) as u8) as char),
        0x1D44E..=0x1D454 => Some((b'a' + (codepoint - 0x1D44E) as u8) as char),
        0x1D456..=0x1D467 => Some((b'a' + (codepoint - 0x1D456 + 8) as u8) as char),
        0x210E => Some('h'),
        0x1D400..=0x1D419 => Some((b'A' + (codepoint - 0x1D400) as u8) as char),
        0x1D41A..=0x1D433 => Some((b'a' + (codepoint - 0x1D41A) as u8) as char),
        0x1D468..=0x1D481 => Some((b'A' + (codepoint - 0x1D468) as u8) as char),
        0x1D482..=0x1D49B => Some((b'a' + (codepoint - 0x1D482) as u8) as char),
        0x1D5A0..=0x1D5B9 => Some((b'A' + (codepoint - 0x1D5A0) as u8) as char),
        0x1D5BA..=0x1D5D3 => Some((b'a' + (codepoint - 0x1D5BA) as u8) as char),
        0x1D670..=0x1D689 => Some((b'A' + (codepoint - 0x1D670) as u8) as char),
        0x1D68A..=0x1D6A3 => Some((b'a' + (codepoint - 0x1D68A) as u8) as char),
        0x1D7CE..=0x1D7D7 => Some((b'0' + (codepoint - 0x1D7CE) as u8) as char),
        0x1D7D8..=0x1D7E1 => Some((b'0' + (codepoint - 0x1D7D8) as u8) as char),
        0x1D7E2..=0x1D7EB => Some((b'0' + (codepoint - 0x1D7E2) as u8) as char),
        0x1D7EC..=0x1D7F5 => Some((b'0' + (codepoint - 0x1D7EC) as u8) as char),
        0x1D7F6..=0x1D7FF => Some((b'0' + (codepoint - 0x1D7F6) as u8) as char),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Point;

    #[test]
    fn fraction_generates_real_glyph_and_rule_geometry() {
        let math = Math::new(Point::new(20.0, 50.0), r"\frac{x^2}{y}".to_string());
        let commands = math_commands(&math, Affine::IDENTITY);
        assert!(commands.len() > 3);
        assert!(math.cached_size().is_some());
    }

    #[test]
    fn invalid_latex_generates_error_geometry() {
        let math = Math::new(Point::new(20.0, 50.0), r"\frac{".to_string());
        let commands = math_commands(&math, Affine::IDENTITY);
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn math_render_transform_applies_rotation_once() {
        let mut math = Math::new(Point::new(20.0, 50.0), "x".to_string());
        math.rotation = std::f64::consts::FRAC_PI_4;
        let center = Point::new(30.0, 44.0);
        let expected = Affine::rotate_about(math.rotation, center)
            * Affine::translate(math.position.to_vec2());
        assert_eq!(
            math_render_transform(&math, 20.0, 10.0, 2.0, Affine::IDENTITY),
            expected
        );
    }
}
