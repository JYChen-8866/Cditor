use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, OnceLock},
};

use crate::{
    Canvas,
    shapes::{SerializableColor, Shape, ShapeTrait, Text},
};
use kurbo::{Affine, BezPath, Rect, Shape as KurboShape, Size, Vec2};
use parley::{
    FontContext, LayoutContext, StyleProperty,
    editing::{Cursor, Selection},
    layout::{Affinity, Layout, PositionedLayoutItem},
};
use peniko::Brush;

use super::plan::{PaintCommand, PaintKind};
use font_registration::register_outline_fonts;

mod font_registration;

#[derive(Clone)]
struct CachedTextLayout {
    commands: Vec<PaintCommand>,
    geometry: TextGeometry,
}

#[derive(Clone, Debug)]
pub(crate) struct TextGeometry {
    pub(crate) origin_offset: Vec2,
    pub(crate) size: Size,
    layout: Arc<Layout<Brush>>,
}

impl TextGeometry {
    pub(crate) fn caret_rect(&self, byte_index: usize, width: f64) -> Rect {
        bounding_box_rect(
            Cursor::from_byte_index(&self.layout, byte_index, Affinity::Downstream)
                .geometry(&self.layout, width as f32),
        )
    }

    pub(crate) fn selection_rects(&self, anchor: usize, focus: usize) -> Vec<Rect> {
        let anchor = Cursor::from_byte_index(&self.layout, anchor, Affinity::Downstream);
        let focus = Cursor::from_byte_index(&self.layout, focus, Affinity::Downstream);
        Selection::new(anchor, focus)
            .geometry(&self.layout)
            .into_iter()
            .map(|(bounds, _)| bounding_box_rect(bounds))
            .collect()
    }

    pub(crate) fn byte_index_for_point(&self, point: kurbo::Point) -> usize {
        Cursor::from_point(&self.layout, point.x as f32, point.y as f32).index()
    }
}

/// Persistent Parley state and local-space glyph cache for GPUI path rendering.
pub(crate) struct TextOutlineEngine {
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    cache: HashMap<String, (u64, CachedTextLayout)>,
    touched: HashSet<String>,
}

impl TextOutlineEngine {
    pub(crate) fn new() -> Self {
        Self::with_font_context(FontContext::new(), true)
    }

    fn with_font_context(mut font_cx: FontContext, allow_system_hanzipen: bool) -> Self {
        register_outline_fonts(&mut font_cx, allow_system_hanzipen);
        Self {
            font_cx,
            layout_cx: LayoutContext::new(),
            cache: HashMap::new(),
            touched: HashSet::new(),
        }
    }

    #[cfg(test)]
    fn without_system_fonts() -> Self {
        let font_cx = FontContext {
            collection: parley::fontique::Collection::new(parley::fontique::CollectionOptions {
                shared: false,
                system_fonts: false,
            }),
            source_cache: Default::default(),
        };
        Self::with_font_context(font_cx, false)
    }

    pub(crate) fn begin_frame(&mut self) {
        self.touched.clear();
    }

    pub(crate) fn finish_frame(&mut self) {
        self.cache.retain(|id, _| self.touched.contains(id));
    }

    pub(crate) fn commands(&mut self, text: &Text, transform: Affine) -> Vec<PaintCommand> {
        if text.content.is_empty() {
            return Vec::new();
        }
        let cached = self.cached_layout(text);
        let text_transform = transform
            * Affine::translate(text.position.to_vec2())
            * Affine::translate(cached.geometry.origin_offset);
        cached
            .commands
            .into_iter()
            .map(|mut command| {
                command.path = text_transform * command.path;
                command
            })
            .collect()
    }

    /// Synchronize exact text geometry into the authoritative model before it
    /// is cloned for painting. Pointer hit testing and the paint snapshot then
    /// observe identical bounds and rotation centers.
    pub(crate) fn prepare_interactive_geometry(&mut self, canvas: &Canvas, viewport: Size) {
        let visible = (viewport.width > 0.0 && viewport.height > 0.0).then(|| {
            let top_left = canvas.camera.screen_to_world(kurbo::Point::ZERO);
            let bottom_right = canvas
                .camera
                .screen_to_world(kurbo::Point::new(viewport.width, viewport.height));
            Rect::new(
                top_left.x.min(bottom_right.x),
                top_left.y.min(bottom_right.y),
                top_left.x.max(bottom_right.x),
                top_left.y.max(bottom_right.y),
            )
        });

        for shape in canvas.document.shapes_ordered() {
            let should_prepare = canvas.is_selected(shape.id())
                || visible.is_none_or(|bounds| shape.bounds().inflate(24.0, 24.0).overlaps(bounds));
            if should_prepare {
                self.prepare_shape_geometry(shape);
            }
        }
    }

    fn prepare_shape_geometry(&mut self, shape: &Shape) {
        match shape {
            Shape::Text(text) => {
                self.prepare(text);
            }
            Shape::Group(group) => {
                for child in group.children() {
                    self.prepare_shape_geometry(child);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn prepare(&mut self, text: &Text) -> TextGeometry {
        let cached = self.cached_layout(text);
        text.set_cached_size(cached.geometry.size.width, cached.geometry.size.height);
        cached.geometry
    }

    fn cached_layout(&mut self, text: &Text) -> CachedTextLayout {
        let key = text_cache_key(text);
        let id = text.id().to_string();
        self.touched.insert(id.clone());
        let cached = match self.cache.get(&id) {
            Some((cached_key, cached)) if *cached_key == key => cached.clone(),
            _ => {
                let cached = self.layout(text);
                self.cache.insert(id, (key, cached.clone()));
                cached
            }
        };
        cached
    }

    fn layout(&mut self, text: &Text) -> CachedTextLayout {
        let default_color = serialized_color(text.style.stroke_color, text.style.opacity);
        let mut builder =
            self.layout_cx
                .ranged_builder(&mut self.font_cx, &text.content, 1.0, false);
        builder.push_default(StyleProperty::FontSize(text.font_size as f32));
        builder.push_default(StyleProperty::Brush(Brush::Solid(default_color)));
        // Every face in the stack is registered as the same optical role. This
        // keeps Virgil and its CJK pair on real outlines with no faux weight.
        builder.push_default(StyleProperty::FontWeight(parley::FontWeight::NORMAL));
        // Existing Cditor scenes persist Drafft's font_family metadata, while
        // the Cditor canvas contract fixes Latin text to the bundled Virgil.
        builder.push_default(StyleProperty::FontFamily(parley::FontFamily::Source(
            crate::font::CANVAS_FONT_STACK.into(),
        )));
        let mut byte_offset = 0;
        for (index, character) in text.content.chars().enumerate() {
            let end = byte_offset + character.len_utf8();
            if let Some(Some(color)) = text.char_colors.get(index) {
                builder.push(
                    StyleProperty::Brush(Brush::Solid(serialized_color(
                        *color,
                        text.style.opacity,
                    ))),
                    byte_offset..end,
                );
            }
            byte_offset += character.len_utf8();
        }

        let mut layout = builder.build(&text.content);
        layout.break_all_lines(None);
        layout.align(
            parley::Alignment::Start,
            parley::AlignmentOptions::default(),
        );
        trace_font_runs(text, &layout);
        // Interactive bounds must include trailing whitespace because Parley
        // places the end caret after its full advance.
        let width = layout.full_width() as f64;
        let height = layout.height() as f64;
        let mut commands = Vec::new();

        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let font = run.font();
                let Ok(face) = ttf_parser::Face::parse(font.data.data(), font.index) else {
                    continue;
                };
                let scale = run.font_size() as f64 / face.units_per_em() as f64;
                let skew = run
                    .synthesis()
                    .skew()
                    .map(|angle| Affine::skew(angle.to_radians().tan() as f64, 0.0))
                    .unwrap_or(Affine::IDENTITY);
                let color = brush_color(&glyph_run.style().brush);
                for glyph in glyph_run.positioned_glyphs() {
                    let mut outline = OutlinePath(BezPath::new());
                    if face
                        .outline_glyph(ttf_parser::GlyphId(glyph.id as u16), &mut outline)
                        .is_some()
                    {
                        let glyph_transform = Affine::translate((glyph.x as f64, glyph.y as f64))
                            * skew
                            * Affine::scale_non_uniform(scale, -scale);
                        commands.push(PaintCommand {
                            path: glyph_transform * outline.0,
                            color,
                            kind: PaintKind::Fill,
                        });
                    }
                }
            }
        }

        let ink_bounds = commands
            .iter()
            .map(|command| command.path.bounding_box())
            .reduce(|bounds, next| bounds.union(next));
        let logical_bounds = Rect::new(0.0, 0.0, width, height);
        let visual_bounds = ink_bounds
            .map(|bounds| logical_bounds.union(bounds))
            .unwrap_or(logical_bounds);
        let geometry = TextGeometry {
            origin_offset: Vec2::new(-visual_bounds.x0, -visual_bounds.y0),
            size: visual_bounds.size(),
            layout: Arc::new(layout),
        };

        CachedTextLayout { commands, geometry }
    }
}

fn bounding_box_rect(bounds: parley::BoundingBox) -> Rect {
    Rect::new(bounds.x0, bounds.y0, bounds.x1, bounds.y1)
}

fn font_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_TEXT_FONT")
            .ok()
            .is_some_and(|value| value != "0")
    })
}

fn trace_font_runs(text: &Text, layout: &Layout<Brush>) {
    if !font_trace_enabled() {
        return;
    }

    for line in layout.lines() {
        for run in line.runs() {
            let range = run.text_range();
            let font = run.font();
            let (face_name, face_weight) = ttf_parser::Face::parse(font.data.data(), font.index)
                .map(|face| (font_face_name(&face), face.weight()))
                .unwrap_or_else(|_| ("<unparsed>".to_string(), ttf_parser::Weight::Normal));
            eprintln!(
                "[cditor][whiteboard][font] id={} model_weight={:?} range={:?} text={:?} face={:?} face_index={} face_weight={:?} synthesis={:?}",
                text.id(),
                text.font_weight,
                range,
                text.content.get(run.text_range()).unwrap_or_default(),
                face_name,
                font.index,
                face_weight,
                run.synthesis(),
            );
        }
    }
}

fn font_face_name(face: &ttf_parser::Face<'_>) -> String {
    font_name(face, ttf_parser::name_id::POST_SCRIPT_NAME)
        .or_else(|| font_name(face, ttf_parser::name_id::FAMILY))
        .unwrap_or_else(|| "<unnamed>".to_string())
}

fn font_name(face: &ttf_parser::Face<'_>, name_id: u16) -> Option<String> {
    face.names()
        .into_iter()
        .filter(|name| name.name_id == name_id)
        .find_map(|name| {
            name.to_string()
                .or_else(|| std::str::from_utf8(name.name).ok().map(str::to_owned))
        })
}

fn serialized_color(color: SerializableColor, opacity: f64) -> peniko::Color {
    peniko::Color::from_rgba8(
        color.r,
        color.g,
        color.b,
        (color.a as f64 * opacity.clamp(0.0, 1.0)).round() as u8,
    )
}

fn brush_color(brush: &Brush) -> u32 {
    let Brush::Solid(color) = brush else {
        return 0x000000ff;
    };
    let [red, green, blue, alpha] = color.components;
    ((component(red) as u32) << 24)
        | ((component(green) as u32) << 16)
        | ((component(blue) as u32) << 8)
        | component(alpha) as u32
}

fn component(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn text_cache_key(text: &Text) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.content.hash(&mut hasher);
    text.font_size.to_bits().hash(&mut hasher);
    text.style.opacity.to_bits().hash(&mut hasher);
    text.style.stroke_color.r.hash(&mut hasher);
    text.style.stroke_color.g.hash(&mut hasher);
    text.style.stroke_color.b.hash(&mut hasher);
    text.style.stroke_color.a.hash(&mut hasher);
    for color in &text.char_colors {
        color
            .map(|value| (value.r, value.g, value.b, value.a))
            .hash(&mut hasher);
    }
    hasher.finish()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{FontFamily, FontWeight};
    use kurbo::Point;

    fn test_engine() -> TextOutlineEngine {
        TextOutlineEngine::without_system_fonts()
    }

    fn data_hash(data: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        hasher.finish()
    }

    fn first_glyph_font_hash(layout: &Layout<Brush>) -> u64 {
        layout
            .lines()
            .flat_map(|line| line.items())
            .find_map(|item| match item {
                PositionedLayoutItem::GlyphRun(glyph_run) => {
                    Some(data_hash(glyph_run.run().font().data.data()))
                }
                _ => None,
            })
            .expect("text layout should contain a glyph run")
    }

    #[test]
    fn rotated_text_generates_transformed_glyph_outlines() {
        let mut engine = test_engine();
        let mut text = Text::new(Point::new(20.0, 30.0), "Rotate".to_string());
        let plain = engine.commands(&text, Affine::IDENTITY);
        text.rotation = std::f64::consts::FRAC_PI_4;
        let center = text.bounds().center();
        let rotated = engine.commands(&text, Affine::rotate_about(text.rotation, center));
        assert!(!plain.is_empty());
        assert_eq!(plain.len(), rotated.len());
        assert_ne!(plain[0].path.bounding_box(), rotated[0].path.bounding_box());
    }

    #[test]
    fn bundled_fallback_generates_chinese_glyph_outlines() {
        let mut engine = test_engine();
        let text = Text::new(Point::new(20.0, 30.0), "中文白板".to_string());
        let commands = engine.commands(&text, Affine::IDENTITY);
        assert_eq!(commands.len(), 4);
        assert!(commands.iter().all(|command| !command.path.is_empty()));
    }

    #[test]
    fn prepared_chinese_bounds_survive_the_paint_snapshot_clone() {
        let mut engine = test_engine();
        let text = Text::new(Point::new(20.0, 30.0), "中文白板".to_string());
        engine.prepare(&text);

        let cloned = text.clone();
        assert_eq!(text.bounds(), cloned.bounds());
    }

    #[test]
    fn stored_drafft_text_style_does_not_replace_cditor_canvas_font() {
        let reference = Text::new(Point::new(20.0, 30.0), "Canvas text".to_string());
        let reference_key = text_cache_key(&reference);
        for family in FontFamily::all() {
            for weight in FontWeight::all() {
                let mut engine = test_engine();
                let mut text = Text::new(Point::new(20.0, 30.0), "Canvas text".to_string());
                text.font_family = *family;
                text.font_weight = *weight;

                assert_eq!(text_cache_key(&text), reference_key);
                let cached = engine.layout(&text);
                assert_eq!(
                    first_glyph_font_hash(&cached.geometry.layout),
                    data_hash(crate::font::VIRGIL),
                    "stored {family:?}/{weight:?} metadata changed the Cditor Latin font contract"
                );
                let run = cached
                    .geometry
                    .layout
                    .lines()
                    .next()
                    .unwrap()
                    .runs()
                    .next()
                    .unwrap();
                assert!(
                    !run.synthesis().embolden(),
                    "optical regular must not synthesize Virgil bold"
                );
            }
        }
    }

    #[test]
    fn font_family_round_trip_keeps_data_without_changing_rendering() {
        let mut text = Text::new(Point::new(20.0, 30.0), "Persisted".to_string());
        text.font_family = FontFamily::GelPen;
        text.font_weight = FontWeight::Light;
        let serialized = serde_json::to_string(&text).expect("serialize text");
        let restored: Text = serde_json::from_str(&serialized).expect("deserialize text");
        assert_eq!(restored.font_family, FontFamily::GelPen);
        assert_eq!(restored.font_weight, FontWeight::Light);

        let mut engine = test_engine();
        let cached = engine.layout(&restored);
        assert_eq!(
            first_glyph_font_hash(&cached.geometry.layout),
            data_hash(crate::font::VIRGIL)
        );
    }

    #[test]
    fn mixed_script_layout_keeps_latin_primary_and_cjk_fallback_runs() {
        let reference = Text::new(Point::new(20.0, 30.0), "阿赛啊asda爱撒啊".to_string());
        let reference_key = text_cache_key(&reference);
        for weight in FontWeight::all() {
            let mut engine = test_engine();
            let mut text = Text::new(Point::new(20.0, 30.0), "阿赛啊asda爱撒啊".to_string());
            text.font_weight = *weight;
            assert_eq!(text_cache_key(&text), reference_key);
            let cached = engine.layout(&text);
            let runs = cached
                .geometry
                .layout
                .lines()
                .flat_map(|line| line.runs())
                .collect::<Vec<_>>();

            assert_eq!(runs.len(), 3, "unexpected {weight:?} script runs");
            assert_ne!(
                data_hash(runs[0].font().data.data()),
                data_hash(crate::font::VIRGIL)
            );
            assert_eq!(
                data_hash(runs[1].font().data.data()),
                data_hash(crate::font::VIRGIL)
            );
            assert_ne!(
                data_hash(runs[2].font().data.data()),
                data_hash(crate::font::VIRGIL)
            );
            for run in runs {
                assert!(
                    !run.synthesis().embolden(),
                    "{weight:?} unexpectedly used faux bold"
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_layout_uses_exact_hanzipen_w5_without_synthetic_weight() {
        let mut engine = TextOutlineEngine::new();
        let Some(family) = engine.font_cx.collection.family_by_name("HanziPen SC") else {
            return;
        };
        let has_w5 = family.fonts().iter().any(|font| {
            font.load(Some(&mut engine.font_cx.source_cache))
                .is_some_and(|data| {
                    ttf_parser::Face::parse(data.as_ref(), font.index())
                        .is_ok_and(|face| font_face_name(&face) == "HanziPenSC-W5")
                })
        });
        if !has_w5 {
            return;
        }

        let text = Text::new(Point::new(20.0, 30.0), "阿赛啊asda爱撒啊".to_string());
        let cached = engine.layout(&text);
        let runs = cached
            .geometry
            .layout
            .lines()
            .flat_map(|line| line.runs())
            .collect::<Vec<_>>();
        assert_eq!(runs.len(), 3);
        for run in runs.iter().step_by(2) {
            let font = run.font();
            let face = ttf_parser::Face::parse(font.data.data(), font.index).unwrap();
            assert_eq!(font.index, 0);
            assert_eq!(font_face_name(&face), "HanziPenSC-W5");
            assert!(!run.synthesis().embolden());
        }
        let latin = &runs[1];
        let font = latin.font();
        let face = ttf_parser::Face::parse(font.data.data(), font.index).unwrap();
        assert_eq!(font_face_name(&face), "Virgil3YOFF");
        assert!(!latin.synthesis().embolden());
    }
}
