use cditor_core::rich_text::InlineSpan;
use gpui::{Bounds, Pixels, TextAlign, WrappedLine as GpuiWrappedLine, fill, point, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::rich_text::{
    NOTION_INLINE_CODE_PADDING_X_PX, NOTION_INLINE_CODE_PADDING_Y_PX, NOTION_INLINE_CODE_RADIUS_PX,
    inline_mark_visual_style,
};

use super::platform::platform_range_segment_bounds;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct InlineBackgroundDecoration {
    pub(super) range: std::ops::Range<usize>,
    pub(super) background_color: u32,
    pub(super) horizontal_padding_px: f32,
}

pub(super) fn inline_background_decorations(
    spans: &[InlineSpan],
    theme: GuiTheme,
) -> Vec<InlineBackgroundDecoration> {
    const NOTION_HIGHLIGHT_PADDING_X_PX: f32 = 1.0;
    let mut decorations: Vec<InlineBackgroundDecoration> = Vec::new();
    let mut offset = 0usize;
    for span in spans {
        let range = offset..offset + span.text.len();
        offset = range.end;
        if range.is_empty() {
            continue;
        }
        let style = inline_mark_visual_style(&span.marks, theme, theme.text);
        let Some(background_color) = style.background_color else {
            continue;
        };
        let horizontal_padding_px = if style.code {
            NOTION_INLINE_CODE_PADDING_X_PX
        } else {
            NOTION_HIGHLIGHT_PADDING_X_PX
        };
        if let Some(previous) = decorations.last_mut()
            && previous.range.end == range.start
            && previous.background_color == background_color
            && previous.horizontal_padding_px == horizontal_padding_px
        {
            previous.range.end = range.end;
        } else {
            decorations.push(InlineBackgroundDecoration {
                range,
                background_color,
                horizontal_padding_px,
            });
        }
    }
    decorations
}

pub(super) fn inline_background_quads(
    spans: &[InlineSpan],
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    theme: GuiTheme,
) -> Vec<gpui::PaintQuad> {
    inline_background_decorations(spans, theme)
        .into_iter()
        .flat_map(|decoration| {
            platform_range_segment_bounds(
                lines,
                bounds,
                line_height,
                text,
                decoration.range,
                TextAlign::Left,
            )
            .into_iter()
            .map(move |segment| {
                fill(
                    notion_inline_background_bounds(segment, decoration.horizontal_padding_px),
                    rgb(decoration.background_color),
                )
                .corner_radii(px(NOTION_INLINE_CODE_RADIUS_PX))
            })
        })
        .collect()
}

#[cfg(test)]
pub(super) fn notion_inline_code_background_bounds(segment: Bounds<Pixels>) -> Bounds<Pixels> {
    notion_inline_background_bounds(segment, NOTION_INLINE_CODE_PADDING_X_PX)
}

fn notion_inline_background_bounds(
    segment: Bounds<Pixels>,
    horizontal_padding_px: f32,
) -> Bounds<Pixels> {
    let left = f32::from(segment.left()) - horizontal_padding_px;
    let right = f32::from(segment.right()) + horizontal_padding_px;
    let top = f32::from(segment.top()) + NOTION_INLINE_CODE_PADDING_Y_PX;
    let bottom = f32::from(segment.bottom()) - NOTION_INLINE_CODE_PADDING_Y_PX;
    Bounds::from_corners(point(px(left), px(top)), point(px(right), px(bottom)))
}

pub(super) fn text_selection_background(theme: GuiTheme) -> u32 {
    (theme.focused << 8) | 0x26
}
