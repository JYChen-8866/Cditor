use cditor_core::layout::block_metrics::{
    NOTION_BODY_LINE_HEIGHT_PX, NOTION_HEADING_1_LINE_HEIGHT_PX, NOTION_HEADING_2_LINE_HEIGHT_PX,
    NOTION_HEADING_3_LINE_HEIGHT_PX,
};
use cditor_core::rich_text::RichBlockKind;
use gpui::{AvailableSpace, Bounds, FontStyle, FontWeight, Pixels, Size, point, px};

use super::super::{
    InlineBoxSpec, RichTextLayoutInput, TextAlignment, TextBrush, TextFontSlant, TextLayoutOptions,
    TextLineHeight, TextStyleConfig,
};
use super::RichTextTypography;
use crate::presentation::rich_text::NOTION_MONO_FONT_FAMILY;
use crate::theme::GuiTheme;

pub(in crate::text) fn measured_wrap_width(
    known_width: Option<Pixels>,
    available_width: AvailableSpace,
    projected_width_px: f64,
) -> Pixels {
    if let Some(width) = known_width {
        return width.max(px(1.0));
    }
    if let AvailableSpace::Definite(width) = available_width {
        return width.max(px(1.0));
    }

    let projected_width_px = if projected_width_px.is_finite() {
        projected_width_px.max(1.0) as f32
    } else {
        1.0
    };
    px(projected_width_px)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn text_layout_options(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    block_text_color: Option<u32>,
    inherited_font_family: &str,
    inherited_weight: FontWeight,
    inherited_style: FontStyle,
    display_scale: f32,
    width: Option<f32>,
    typography: RichTextTypography,
    inline_boxes: Vec<InlineBoxSpec>,
) -> TextLayoutOptions {
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
    TextLayoutOptions {
        width,
        display_scale,
        alignment: TextAlignment::from_core(input.text_align),
        base_text_color,
        base_style: TextStyleConfig {
            font_family: font_family.to_owned(),
            font_size: f32::from(text_size),
            font_slant: match inherited_style {
                FontStyle::Normal => TextFontSlant::Normal,
                FontStyle::Italic => TextFontSlant::Italic,
                FontStyle::Oblique => TextFontSlant::Oblique,
            },
            font_weight: typography
                .font_weight
                .unwrap_or_else(|| base_font_weight_for_kind(&input.kind, inherited_weight))
                .0,
            brush: TextBrush {
                foreground: base_text_color,
                background: None,
                ..TextBrush::default()
            },
            line_height: TextLineHeight::Absolute(f32::from(line_height)),
            strikethrough: is_completed_todo(&input.kind),
            ..TextStyleConfig::default()
        },
        mono_font_family: NOTION_MONO_FONT_FAMILY.to_owned(),
        inline_boxes,
        ..TextLayoutOptions::default()
    }
}

pub(in crate::text) fn text_size_for(
    kind: &RichBlockKind,
    typography: RichTextTypography,
) -> Pixels {
    typography
        .font_size_px
        .map(px)
        .unwrap_or_else(|| text_size_for_kind(kind))
}

pub(in crate::text) fn line_height_for(
    kind: &RichBlockKind,
    typography: RichTextTypography,
) -> Pixels {
    typography
        .line_height_px
        .map(px)
        .unwrap_or_else(|| line_height_for_kind(kind, text_size_for(kind, typography)))
}

pub(in crate::text) fn text_rect_to_bounds(
    parent: Bounds<Pixels>,
    rect: super::super::TextLayoutRect,
) -> Bounds<Pixels> {
    Bounds::new(
        point(parent.left() + px(rect.x), parent.top() + px(rect.y)),
        Size {
            width: px(rect.width.max(0.0)),
            height: px(rect.height.max(0.0)),
        },
    )
}

pub(in crate::text) fn text_size_for_kind(kind: &RichBlockKind) -> Pixels {
    match kind {
        RichBlockKind::Heading { level: 1 } => px(30.0),
        RichBlockKind::Heading { level: 2 } => px(24.0),
        RichBlockKind::Heading { .. } => px(20.0),
        RichBlockKind::Code { .. } => px(14.0),
        RichBlockKind::FootnoteDefinition => px(14.0),
        _ => px(16.0),
    }
}

pub(in crate::text) fn base_font_weight_for_kind(
    kind: &RichBlockKind,
    inherited: FontWeight,
) -> FontWeight {
    if matches!(kind, RichBlockKind::Heading { .. }) && inherited < FontWeight::SEMIBOLD {
        FontWeight::SEMIBOLD
    } else {
        inherited
    }
}

pub(in crate::text) fn line_height_for_kind(kind: &RichBlockKind, _text_size: Pixels) -> Pixels {
    match kind {
        RichBlockKind::Code { .. } => px(24.0),
        RichBlockKind::Heading { level: 1 } => px(NOTION_HEADING_1_LINE_HEIGHT_PX as f32),
        RichBlockKind::Heading { level: 2 } => px(NOTION_HEADING_2_LINE_HEIGHT_PX as f32),
        RichBlockKind::Heading { .. } => px(NOTION_HEADING_3_LINE_HEIGHT_PX as f32),
        RichBlockKind::FootnoteDefinition => px(20.0),
        _ => px(NOTION_BODY_LINE_HEIGHT_PX as f32),
    }
}

pub(in crate::text) fn text_color_for_kind(kind: &RichBlockKind, theme: GuiTheme) -> u32 {
    match kind {
        RichBlockKind::Code { .. } => theme.code_text,
        RichBlockKind::Quote => theme.quote_text,
        RichBlockKind::Todo { checked: true } => theme.muted,
        _ => theme.text,
    }
}

pub(in crate::text) fn is_completed_todo(kind: &RichBlockKind) -> bool {
    matches!(kind, RichBlockKind::Todo { checked: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_wrap_width_prefers_final_or_definite_parent_width() {
        assert_eq!(
            measured_wrap_width(Some(px(640.0)), AvailableSpace::Definite(px(320.0)), 720.0,),
            px(640.0)
        );
        assert_eq!(
            measured_wrap_width(None, AvailableSpace::Definite(px(480.0)), 720.0),
            px(480.0)
        );
    }

    #[test]
    fn intrinsic_measurement_keeps_the_projected_document_track_width() {
        assert_eq!(
            measured_wrap_width(None, AvailableSpace::MinContent, 684.0),
            px(684.0)
        );
        assert_eq!(
            measured_wrap_width(None, AvailableSpace::MaxContent, 684.0),
            px(684.0)
        );
    }

    #[test]
    fn measured_wrap_width_rejects_degenerate_widths() {
        assert_eq!(
            measured_wrap_width(None, AvailableSpace::Definite(px(0.0)), 720.0),
            px(1.0)
        );
        assert_eq!(
            measured_wrap_width(None, AvailableSpace::MinContent, f64::NAN),
            px(1.0)
        );
    }
}
