use parley::{FontData, FontStyle, Layout, PositionedLayoutItem};
use skrifa::{FontRef, MetadataProvider, raw::TableProvider, string::StringId};

use super::TextBrush;
use crate::{
    FontFaceKey, FontInstanceKey, FontSynthesisKey, FontVariationSettingKey,
    font_identity::font_blob_digest,
};

#[derive(Debug, Clone)]
pub struct TextPaintPlan {
    pub runs: Vec<TextPaintRun>,
    pub backgrounds: Vec<TextPaintBackground>,
}

#[derive(Debug, Clone)]
pub struct TextPaintRun {
    pub font: TextPaintFont,
    pub text_range: std::ops::Range<usize>,
    pub is_rtl: bool,
    pub font_size: f32,
    pub brush: TextBrush,
    pub glyphs: Vec<TextPaintGlyph>,
    pub decoration_x: f32,
    pub decoration_width: f32,
    pub baseline: f32,
    pub underline: Option<TextPaintDecoration>,
    pub strikethrough: Option<TextPaintDecoration>,
}

#[derive(Debug, Clone)]
pub struct TextPaintFont {
    data: FontData,
    instance_key: FontInstanceKey,
    pub family: String,
    pub weight: f32,
    pub style: TextPaintFontStyle,
    pub synthesized: bool,
    pub normalized_coords: Vec<i16>,
}

impl TextPaintFont {
    pub fn instance_key(&self) -> &FontInstanceKey {
        &self.instance_key
    }

    pub fn face_index(&self) -> u32 {
        self.data.index
    }

    pub fn blob_id(&self) -> u64 {
        self.data.data.id()
    }

    pub fn blob_digest(&self) -> crate::FontBlobDigest {
        font_blob_digest(self.blob_id(), self.data())
    }

    pub fn data(&self) -> &[u8] {
        self.data.data.data()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPaintFontStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy)]
pub struct TextPaintGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub color: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct TextPaintDecoration {
    pub color: u32,
    pub offset: f32,
    pub size: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct TextPaintBackground {
    pub rect: super::TextLayoutRect,
    pub color: u32,
    pub radius: f32,
}

impl TextPaintPlan {
    pub(crate) fn from_layout(layout: &Layout<TextBrush>, scale: f32) -> Self {
        let mut runs = Vec::new();
        let mut backgrounds = Vec::new();
        for line in layout.lines() {
            let line_metrics = line.metrics();
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let style = glyph_run.style();
                let brush = style.brush;
                let x = glyph_run.offset() / scale;
                let width = glyph_run.advance() / scale;
                if let Some(color) = brush.background {
                    backgrounds.push(TextPaintBackground {
                        rect: super::TextLayoutRect {
                            x: x - f32::from(brush.background_padding_x),
                            y: line_metrics.block_min_coord / scale
                                + f32::from(brush.background_padding_y),
                            width: width + f32::from(brush.background_padding_x) * 2.0,
                            height: ((line_metrics.block_max_coord - line_metrics.block_min_coord)
                                / scale
                                - f32::from(brush.background_padding_y) * 2.0)
                                .max(0.0),
                        },
                        color,
                        radius: f32::from(brush.background_radius),
                    });
                }
                let metrics = run.metrics();
                let font = run.font().clone();
                let synthesis = run.synthesis();
                let instance_key = FontInstanceKey::new(
                    FontFaceKey::new(font.data.id(), font.data.data().len(), font.index),
                    run.normalized_coords().to_vec(),
                    FontSynthesisKey::new(
                        synthesis
                            .variation_settings()
                            .iter()
                            .map(|(tag, value)| {
                                FontVariationSettingKey::new(tag.to_be_bytes(), *value)
                            })
                            .collect(),
                        synthesis.embolden(),
                        synthesis.skew(),
                    ),
                );
                let family = font_family_name(&font).unwrap_or_else(|| "system-ui".to_owned());
                let font_ref = FontRef::from_index(font.data.data(), font.index).ok();
                let glyphs = glyph_run
                    .positioned_glyphs()
                    .map(|glyph| TextPaintGlyph {
                        id: glyph.id,
                        x: glyph.x / scale,
                        y: glyph.y / scale,
                        color: font_ref
                            .as_ref()
                            .is_some_and(|font| font_ref_has_color_glyph(font, glyph.id)),
                    })
                    .collect();
                let underline = style
                    .underline
                    .as_ref()
                    .map(|decoration| TextPaintDecoration {
                        color: decoration.brush.foreground,
                        offset: decoration.offset.unwrap_or(metrics.underline_offset) / scale,
                        size: decoration.size.unwrap_or(metrics.underline_size) / scale,
                    });
                let strikethrough =
                    style
                        .strikethrough
                        .as_ref()
                        .map(|decoration| TextPaintDecoration {
                            color: decoration.brush.foreground,
                            offset: decoration.offset.unwrap_or(metrics.strikethrough_offset)
                                / scale,
                            size: decoration.size.unwrap_or(metrics.strikethrough_size) / scale,
                        });
                runs.push(TextPaintRun {
                    font: TextPaintFont {
                        data: font,
                        instance_key,
                        family,
                        weight: run.font_attrs().weight.value(),
                        style: match run.font_attrs().style {
                            FontStyle::Normal => TextPaintFontStyle::Normal,
                            FontStyle::Italic => TextPaintFontStyle::Italic,
                            FontStyle::Oblique(_) => TextPaintFontStyle::Oblique,
                        },
                        synthesized: run.synthesis().any(),
                        normalized_coords: run.normalized_coords().to_vec(),
                    },
                    text_range: run.text_range(),
                    is_rtl: run.is_rtl(),
                    font_size: run.font_size() / scale,
                    brush,
                    glyphs,
                    decoration_x: x,
                    decoration_width: width,
                    baseline: glyph_run.baseline() / scale,
                    underline,
                    strikethrough,
                });
            }
        }
        Self { runs, backgrounds }
    }
}

fn font_family_name(font: &FontData) -> Option<String> {
    let font_ref = FontRef::from_index(font.data.data(), font.index).ok()?;
    font_ref
        .localized_strings(StringId::TYPOGRAPHIC_FAMILY_NAME)
        .english_or_first()
        .or_else(|| {
            font_ref
                .localized_strings(StringId::FAMILY_NAME)
                .english_or_first()
        })
        .map(|name| name.to_string())
}

pub(crate) fn font_ref_has_color_glyph(font_ref: &FontRef<'_>, glyph_id: u32) -> bool {
    let glyph_id = skrifa::GlyphId::new(glyph_id);
    font_ref.color_glyphs().get(glyph_id).is_some()
        || font_ref
            .bitmap_strikes()
            .iter()
            .any(|strike| strike.get(glyph_id).is_some())
        || font_ref
            .svg()
            .ok()
            .and_then(|svg| svg.glyph_data(glyph_id).ok().flatten())
            .is_some()
}
