use std::{borrow::Cow, ops::Range};

use cditor_core::rich_text::{InlineMark, InlineSpan, RichBlockKind};
use parley::{
    FontFamily, FontFeatures, FontStyle, FontVariations, FontWeight, FontWidth, Language,
    LineHeight, OverflowWrap, TextStyle, TextWrapMode, WordBreak,
};

use crate::TextTheme;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextBrush {
    pub foreground: u32,
    pub background: Option<u32>,
    pub background_padding_x: u8,
    pub background_padding_y: u8,
    pub background_radius: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextFontSlant {
    #[default]
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextLineHeight {
    FontSizeRelative(f32),
    Absolute(f32),
}

impl Default for TextLineHeight {
    fn default() -> Self {
        Self::FontSizeRelative(1.5)
    }
}

impl TextLineHeight {
    fn as_parley(self) -> LineHeight {
        match self {
            Self::FontSizeRelative(value) => LineHeight::FontSizeRelative(value),
            Self::Absolute(value) => LineHeight::Absolute(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyleConfig {
    pub font_family: String,
    pub font_size: f32,
    pub font_width: f32,
    pub font_slant: TextFontSlant,
    pub font_weight: f32,
    pub font_variations: String,
    pub font_features: String,
    pub locale: Option<String>,
    pub brush: TextBrush,
    pub underline: bool,
    pub underline_offset: Option<f32>,
    pub underline_size: Option<f32>,
    pub underline_brush: Option<TextBrush>,
    pub strikethrough: bool,
    pub strikethrough_offset: Option<f32>,
    pub strikethrough_size: Option<f32>,
    pub strikethrough_brush: Option<TextBrush>,
    pub line_height: TextLineHeight,
    pub word_spacing: f32,
    pub letter_spacing: f32,
    pub word_break: WordBreak,
    pub overflow_wrap: OverflowWrap,
    pub text_wrap_mode: TextWrapMode,
}

impl Default for TextStyleConfig {
    fn default() -> Self {
        Self {
            font_family: "system-ui".to_owned(),
            font_size: 16.0,
            font_width: 1.0,
            font_slant: TextFontSlant::Normal,
            font_weight: 400.0,
            font_variations: String::new(),
            font_features: String::new(),
            locale: None,
            brush: TextBrush::default(),
            underline: false,
            underline_offset: None,
            underline_size: None,
            underline_brush: None,
            strikethrough: false,
            strikethrough_offset: None,
            strikethrough_size: None,
            strikethrough_brush: None,
            line_height: TextLineHeight::default(),
            word_spacing: 0.0,
            letter_spacing: 0.0,
            word_break: WordBreak::Normal,
            overflow_wrap: OverflowWrap::Anywhere,
            text_wrap_mode: TextWrapMode::Wrap,
        }
    }
}

impl TextStyleConfig {
    pub(crate) fn as_parley_style(&self) -> TextStyle<'_, '_, TextBrush> {
        let font_variations = if self.font_variations.is_empty() {
            FontVariations::empty()
        } else {
            FontVariations::Source(Cow::Borrowed(self.font_variations.as_str()))
        };
        let font_features = if self.font_features.is_empty() {
            FontFeatures::empty()
        } else {
            FontFeatures::Source(Cow::Borrowed(self.font_features.as_str()))
        };
        TextStyle {
            font_family: FontFamily::Source(Cow::Borrowed(self.font_family.as_str())),
            font_size: self.font_size,
            font_width: FontWidth::from_ratio(self.font_width),
            font_style: match self.font_slant {
                TextFontSlant::Normal => FontStyle::Normal,
                TextFontSlant::Italic => FontStyle::Italic,
                TextFontSlant::Oblique => FontStyle::Oblique(None),
            },
            font_weight: FontWeight::new(self.font_weight),
            font_variations,
            font_features,
            locale: self
                .locale
                .as_deref()
                .and_then(|locale| Language::parse(locale).ok()),
            brush: self.brush,
            has_underline: self.underline,
            underline_offset: self.underline_offset,
            underline_size: self.underline_size,
            underline_brush: self.underline_brush,
            has_strikethrough: self.strikethrough,
            strikethrough_offset: self.strikethrough_offset,
            strikethrough_size: self.strikethrough_size,
            strikethrough_brush: self.strikethrough_brush,
            line_height: self.line_height.as_parley(),
            word_spacing: self.word_spacing,
            letter_spacing: self.letter_spacing,
            word_break: self.word_break,
            overflow_wrap: self.overflow_wrap,
            text_wrap_mode: self.text_wrap_mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyleRun {
    pub range: Range<usize>,
    pub style: TextStyleConfig,
}

pub fn text_style_runs(
    spans: &[InlineSpan],
    kind: &RichBlockKind,
    theme: TextTheme,
    base_text_color: u32,
    base_style: &TextStyleConfig,
    mono_font_family: &str,
) -> Vec<TextStyleRun> {
    let completed_todo = matches!(kind, RichBlockKind::Todo { checked: true });
    let mut offset = 0;
    let mut runs = Vec::with_capacity(spans.len());
    for span in spans {
        let start = offset;
        offset += span.text.len();
        if start == offset {
            continue;
        }
        let visual = inline_mark_visual_style(&span.marks, theme, base_text_color);
        let mut style = base_style.clone();
        style.brush = TextBrush {
            foreground: visual.text_color,
            background: visual.background_color,
            background_padding_x: if visual.code { 3 } else { 1 },
            background_padding_y: u8::from(visual.code),
            background_radius: if visual.code { 3 } else { 0 },
        };
        if visual.bold {
            style.font_weight = style.font_weight.max(700.0);
        }
        if visual.italic {
            style.font_slant = TextFontSlant::Italic;
        }
        if visual.code {
            style.font_family = mono_font_family.to_owned();
            // Code typography must not turn source characters into discretionary ligatures.
            style.font_features = "'liga' 0, 'calt' 0".to_owned();
        }
        style.underline = visual.underline;
        style.strikethrough = visual.strike || completed_todo;
        runs.push(TextStyleRun {
            range: start..offset,
            style,
        });
    }
    runs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InlineMarkVisualStyle {
    text_color: u32,
    background_color: Option<u32>,
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    underline: bool,
}

fn inline_mark_visual_style(
    marks: &[InlineMark],
    theme: TextTheme,
    base_text_color: u32,
) -> InlineMarkVisualStyle {
    let mut explicit_text_color = None;
    let mut explicit_background = None;
    let mut bold = false;
    let mut italic = false;
    let mut code = false;
    let mut strike = false;
    let mut underline = false;
    let mut link = false;

    for mark in marks {
        match mark {
            InlineMark::Bold => bold = true,
            InlineMark::Italic => italic = true,
            InlineMark::Underline => underline = true,
            InlineMark::Strike => strike = true,
            InlineMark::Code => code = true,
            InlineMark::Link { .. } => link = true,
            InlineMark::Color(color) => {
                explicit_text_color = parse_hex_color(color).or(explicit_text_color);
            }
            InlineMark::Background(color) => {
                explicit_background = parse_hex_color(color).or(explicit_background);
            }
        }
    }

    let text_color = explicit_text_color.unwrap_or({
        if link {
            theme.link_text
        } else if code {
            theme.inline_code_text
        } else {
            base_text_color
        }
    });
    InlineMarkVisualStyle {
        text_color,
        background_color: explicit_background.or(code.then_some(theme.inline_code_background)),
        bold,
        italic,
        code,
        strike,
        underline: underline || link,
    }
}

fn parse_hex_color(color: &str) -> Option<u32> {
    let value = color.strip_prefix('#').unwrap_or(color);
    (value.len() == 6)
        .then(|| u32::from_str_radix(value, 16).ok())
        .flatten()
}
