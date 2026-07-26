use cditor_core::rich_text::RichBlockKind;
use gpui::{Bounds, PaintQuad, Pixels, Size, fill, point, px, rgb};

use super::{RichTextTypography, metrics::line_height_for};
use crate::theme::GuiTheme;

pub(super) fn deferred_placeholder_quads(
    bounds: Bounds<Pixels>,
    kind: &RichBlockKind,
    typography: RichTextTypography,
    theme: GuiTheme,
) -> Vec<PaintQuad> {
    let line_height = f32::from(line_height_for(kind, typography));
    let line_count = (f32::from(bounds.size.height) / line_height.max(1.0))
        .ceil()
        .clamp(1.0, 3.0) as usize;
    (0..line_count)
        .map(|index| {
            let width_ratio = if index + 1 == line_count { 0.42 } else { 0.68 };
            fill(
                Bounds::new(
                    point(
                        bounds.left(),
                        bounds.top() + px(index as f32 * line_height + line_height * 0.3),
                    ),
                    Size {
                        width: bounds.size.width * width_ratio,
                        height: px(4.0),
                    },
                ),
                rgb(theme.skeleton),
            )
        })
        .collect()
}
