use std::{cell::RefCell, sync::Arc};

use fontique::Blob;
use parley::{
    Alignment, AlignmentOptions, FontContext, IndentOptions, InlineBox,
    InlineBoxKind as ParleyInlineBoxKind, LayoutContext,
};

use super::{TextBrush, TextLayoutSnapshot, TextStyleConfig, TextStyleRun, text_style_runs};
use crate::{TextLayoutInput, TextSnapshot, TextTheme};

thread_local! {
    static PARLEY_CONTEXTS: RefCell<ParleyContexts> = RefCell::new(ParleyContexts::new());
}

struct ParleyContexts {
    font: FontContext,
    layout: LayoutContext<TextBrush>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredFontFamily {
    pub name: String,
    pub face_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontRegistrationError {
    EmptyData,
    NoFontFaces,
}

impl std::fmt::Display for FontRegistrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyData => formatter.write_str("font data is empty"),
            Self::NoFontFaces => formatter.write_str("font data contains no registerable faces"),
        }
    }
}

impl std::error::Error for FontRegistrationError {}

impl ParleyContexts {
    fn new() -> Self {
        Self {
            font: FontContext::new(),
            layout: LayoutContext::new(),
        }
    }
}

pub fn register_font_data(
    data: Vec<u8>,
) -> Result<Vec<RegisteredFontFamily>, FontRegistrationError> {
    if data.is_empty() {
        return Err(FontRegistrationError::EmptyData);
    }
    let families = PARLEY_CONTEXTS.with(|contexts| {
        let mut contexts = contexts.borrow_mut();
        let registered = contexts
            .font
            .collection
            .register_fonts(Blob::new(Arc::new(data)), None);
        if registered.is_empty() {
            return Err(FontRegistrationError::NoFontFaces);
        }
        Ok(registered
            .into_iter()
            .map(|(family_id, faces)| RegisteredFontFamily {
                name: contexts
                    .font
                    .collection
                    .family_name(family_id)
                    .unwrap_or("unknown")
                    .to_owned(),
                face_count: faces.len(),
            })
            .collect())
    })?;
    crate::cache::clear_text_layout_cache();
    Ok(families)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TextAlignment {
    #[default]
    Start,
    End,
    Left,
    Center,
    Right,
    Justify,
}

impl TextAlignment {
    pub fn from_core(align: cditor_core::rich_text::TextAlign) -> Self {
        match align {
            cditor_core::rich_text::TextAlign::Start => Self::Start,
            cditor_core::rich_text::TextAlign::Center => Self::Center,
            cditor_core::rich_text::TextAlign::End => Self::End,
        }
    }

    pub(crate) fn as_parley(self) -> Alignment {
        match self {
            Self::Start => Alignment::Start,
            Self::End => Alignment::End,
            Self::Left => Alignment::Left,
            Self::Center => Alignment::Center,
            Self::Right => Alignment::Right,
            Self::Justify => Alignment::Justify,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextIndent {
    pub amount: f32,
    pub each_line: bool,
    pub hanging: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InlineBoxKind {
    #[default]
    InFlow,
    OutOfFlow,
    CustomOutOfFlow,
}

impl InlineBoxKind {
    fn as_parley(self) -> ParleyInlineBoxKind {
        match self {
            Self::InFlow => ParleyInlineBoxKind::InFlow,
            Self::OutOfFlow => ParleyInlineBoxKind::OutOfFlow,
            Self::CustomOutOfFlow => ParleyInlineBoxKind::CustomOutOfFlow,
        }
    }

    pub(crate) fn from_parley(kind: ParleyInlineBoxKind) -> Self {
        match kind {
            ParleyInlineBoxKind::InFlow => Self::InFlow,
            ParleyInlineBoxKind::OutOfFlow => Self::OutOfFlow,
            ParleyInlineBoxKind::CustomOutOfFlow => Self::CustomOutOfFlow,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InlineBoxSpec {
    pub id: u64,
    pub kind: InlineBoxKind,
    pub index: usize,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLayoutOptions {
    pub width: Option<f32>,
    pub display_scale: f32,
    pub quantize: bool,
    pub alignment: TextAlignment,
    pub base_style: TextStyleConfig,
    pub base_text_color: u32,
    pub mono_font_family: String,
    pub text_indent: TextIndent,
    pub inline_boxes: Vec<InlineBoxSpec>,
}

impl Default for TextLayoutOptions {
    fn default() -> Self {
        Self {
            width: None,
            display_scale: 1.0,
            quantize: true,
            alignment: TextAlignment::Start,
            base_style: TextStyleConfig::default(),
            base_text_color: 0,
            mono_font_family: "monospace".to_owned(),
            text_indent: TextIndent::default(),
            inline_boxes: Vec::new(),
        }
    }
}

pub fn build_text_layout(
    input: &TextLayoutInput,
    theme: TextTheme,
    options: &TextLayoutOptions,
) -> TextLayoutSnapshot {
    let text = input.plain_text();
    let mut style_runs = text_style_runs(
        &input.spans,
        &input.kind,
        theme,
        options.base_text_color,
        &options.base_style,
        &options.mono_font_family,
    );
    ensure_complete_style_coverage(&text, &mut style_runs, &options.base_style);

    let scale = options.display_scale.max(f32::EPSILON);
    PARLEY_CONTEXTS.with(|contexts| {
        let mut contexts = contexts.borrow_mut();
        let ParleyContexts { font, layout } = &mut *contexts;
        let mut builder = layout.style_run_builder(font, &text, scale, options.quantize);
        builder.reserve(style_runs.len(), style_runs.len());
        for run in &style_runs {
            let style_index = builder.push_style(run.style.as_parley_style());
            builder.push_style_run(style_index, run.range.clone());
        }
        for inline_box in &options.inline_boxes {
            builder.push_inline_box(InlineBox {
                id: inline_box.id,
                kind: inline_box.kind.as_parley(),
                index: inline_box.index.min(text.len()),
                width: inline_box.width * scale,
                height: inline_box.height * scale,
            });
        }
        let mut layout = builder.build(&text);
        if options.text_indent.amount != 0.0 {
            layout.set_text_indent(
                options.text_indent.amount * scale,
                IndentOptions {
                    each_line: options.text_indent.each_line,
                    hanging: options.text_indent.hanging,
                },
            );
        }
        layout.break_all_lines(options.width.map(|width| width.max(0.0) * scale));
        layout.align(options.alignment.as_parley(), AlignmentOptions::default());
        TextLayoutSnapshot::new(
            TextSnapshot::new(text),
            layout,
            scale,
            options.width,
            options.alignment,
            options.quantize,
        )
    })
}

fn ensure_complete_style_coverage(
    text: &str,
    style_runs: &mut Vec<TextStyleRun>,
    base_style: &TextStyleConfig,
) {
    if text.is_empty() {
        if style_runs.is_empty() {
            style_runs.push(TextStyleRun {
                range: 0..0,
                style: base_style.clone(),
            });
        }
        return;
    }
    if style_runs.is_empty() {
        style_runs.push(TextStyleRun {
            range: 0..text.len(),
            style: base_style.clone(),
        });
        return;
    }
    debug_assert_eq!(style_runs.first().map(|run| run.range.start), Some(0));
    debug_assert_eq!(style_runs.last().map(|run| run.range.end), Some(text.len()));
}
