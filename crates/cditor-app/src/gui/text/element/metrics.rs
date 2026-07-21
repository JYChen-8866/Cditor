use std::ops::Range;

use cditor_core::edit::TextAffinity;
use cditor_core::layout::block_metrics::{
    NOTION_BODY_LINE_HEIGHT_PX, NOTION_HEADING_1_LINE_HEIGHT_PX, NOTION_HEADING_2_LINE_HEIGHT_PX,
    NOTION_HEADING_3_LINE_HEIGHT_PX,
};
use cditor_core::rich_text::{InlineSpan, RichBlockKind};
use gpui::{Bounds, FontStyle, FontWeight, Pixels, Size, point, px};

use super::super::{
    ParleyAlignment, ParleyBrush, ParleyFontSlant, ParleyInlineBoxSpec, ParleyLayoutOptions,
    ParleyLayoutSnapshot, ParleyLineHeight, ParleySelection, ParleyTextPosition,
    ParleyTextStyleConfig, RichTextLayoutInput,
};
use super::RichTextTypography;
use crate::gui::GuiTheme;
use crate::gui::rich_text::NOTION_MONO_FONT_FAMILY;

pub(in crate::gui::text) fn plain_text_from_spans(spans: &[InlineSpan]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

#[allow(clippy::too_many_arguments)]
pub(in crate::gui::text) fn parley_layout_options(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    block_text_color: Option<u32>,
    inherited_font_family: &str,
    inherited_weight: FontWeight,
    inherited_style: FontStyle,
    display_scale: f32,
    width: Option<f32>,
    typography: RichTextTypography,
    inline_boxes: Vec<ParleyInlineBoxSpec>,
) -> ParleyLayoutOptions {
    let text_size = text_size_for(&input.kind, typography);
    let line_height = line_height_for(&input.kind, typography);
    let base_text_color =
        block_text_color.unwrap_or_else(|| text_color_for_kind(&input.kind, theme));
    let inherited_family = if inherited_font_family.starts_with('.') {
        "system-ui"
    } else {
        inherited_font_family
    };
    let font_family = if matches!(input.kind, RichBlockKind::Code { .. }) {
        NOTION_MONO_FONT_FAMILY
    } else {
        inherited_family
    };
    ParleyLayoutOptions {
        width,
        display_scale,
        alignment: ParleyAlignment::from_core(input.text_align),
        base_text_color,
        base_style: ParleyTextStyleConfig {
            font_family: font_family.to_owned(),
            font_size: f32::from(text_size),
            font_slant: match inherited_style {
                FontStyle::Normal => ParleyFontSlant::Normal,
                FontStyle::Italic => ParleyFontSlant::Italic,
                FontStyle::Oblique => ParleyFontSlant::Oblique,
            },
            font_weight: typography
                .font_weight
                .unwrap_or_else(|| base_font_weight_for_kind(&input.kind, inherited_weight))
                .0,
            brush: ParleyBrush {
                foreground: base_text_color,
                background: None,
                ..ParleyBrush::default()
            },
            line_height: ParleyLineHeight::Absolute(f32::from(line_height)),
            strikethrough: is_completed_todo(&input.kind),
            ..ParleyTextStyleConfig::default()
        },
        mono_font_family: NOTION_MONO_FONT_FAMILY.to_owned(),
        inline_boxes,
        ..ParleyLayoutOptions::default()
    }
}

pub(in crate::gui::text) fn text_size_for(
    kind: &RichBlockKind,
    typography: RichTextTypography,
) -> Pixels {
    typography
        .font_size_px
        .map(px)
        .unwrap_or_else(|| text_size_for_kind(kind))
}

pub(in crate::gui::text) fn line_height_for(
    kind: &RichBlockKind,
    typography: RichTextTypography,
) -> Pixels {
    typography
        .line_height_px
        .map(px)
        .unwrap_or_else(|| line_height_for_kind(kind, text_size_for(kind, typography)))
}

pub(in crate::gui::text) fn parley_range_rects(
    layout: &ParleyLayoutSnapshot,
    range: Range<usize>,
) -> Vec<super::super::ParleyRect> {
    let start = range.start.min(layout.text().len());
    let end = range.end.min(layout.text().len()).max(start);
    if start == end {
        return Vec::new();
    }
    layout.selection_rects(ParleySelection {
        anchor: ParleyTextPosition::downstream(start),
        focus: ParleyTextPosition {
            offset: end,
            affinity: TextAffinity::Upstream,
        },
    })
}

pub(in crate::gui::text) fn parley_rect_to_bounds(
    parent: Bounds<Pixels>,
    rect: super::super::ParleyRect,
) -> Bounds<Pixels> {
    Bounds::new(
        point(parent.left() + px(rect.x), parent.top() + px(rect.y)),
        Size {
            width: px(rect.width.max(0.0)),
            height: px(rect.height.max(0.0)),
        },
    )
}

pub(in crate::gui::text) fn text_size_for_kind(kind: &RichBlockKind) -> Pixels {
    match kind {
        RichBlockKind::Heading { level: 1 } => px(30.0),
        RichBlockKind::Heading { level: 2 } => px(24.0),
        RichBlockKind::Heading { .. } => px(20.0),
        RichBlockKind::Code { .. } => px(14.0),
        RichBlockKind::FootnoteDefinition => px(14.0),
        _ => px(16.0),
    }
}

pub(in crate::gui::text) fn base_font_weight_for_kind(
    kind: &RichBlockKind,
    inherited: FontWeight,
) -> FontWeight {
    if matches!(kind, RichBlockKind::Heading { .. }) && inherited < FontWeight::SEMIBOLD {
        FontWeight::SEMIBOLD
    } else {
        inherited
    }
}

pub(in crate::gui::text) fn line_height_for_kind(
    kind: &RichBlockKind,
    _text_size: Pixels,
) -> Pixels {
    match kind {
        RichBlockKind::Code { .. } => px(24.0),
        RichBlockKind::Heading { level: 1 } => px(NOTION_HEADING_1_LINE_HEIGHT_PX as f32),
        RichBlockKind::Heading { level: 2 } => px(NOTION_HEADING_2_LINE_HEIGHT_PX as f32),
        RichBlockKind::Heading { .. } => px(NOTION_HEADING_3_LINE_HEIGHT_PX as f32),
        RichBlockKind::FootnoteDefinition => px(20.0),
        _ => px(NOTION_BODY_LINE_HEIGHT_PX as f32),
    }
}

pub(in crate::gui::text) fn text_color_for_kind(kind: &RichBlockKind, theme: GuiTheme) -> u32 {
    match kind {
        RichBlockKind::Code { .. } => theme.code_text,
        RichBlockKind::Quote => theme.quote_text,
        RichBlockKind::Todo { checked: true } => theme.muted,
        _ => theme.text,
    }
}

pub(in crate::gui::text) fn is_completed_todo(kind: &RichBlockKind) -> bool {
    matches!(kind, RichBlockKind::Todo { checked: true })
}
