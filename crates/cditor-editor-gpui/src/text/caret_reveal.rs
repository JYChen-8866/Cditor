use gpui::{Bounds, Pixels, ScrollHandle, Window, px};

const CARET_REVEAL_TOP_MARGIN_PX: f32 = 8.0;
const CARET_REVEAL_BOTTOM_MARGIN_PX: f32 = 24.0;

#[derive(Clone, Copy)]
struct VerticalRange {
    top_px: f32,
    bottom_px: f32,
}

#[derive(Clone, Copy)]
struct CaretRevealMargins {
    top_px: f32,
    bottom_px: f32,
}

pub(super) fn reveal_caret_in_scroll_handle(
    scroll_handle: &ScrollHandle,
    caret_bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let viewport_bounds = scroll_handle.bounds();
    let viewport_height_px = f32::from(viewport_bounds.size.height);
    if viewport_height_px <= 0.5 {
        return;
    }
    let current_offset_y = f32::from(scroll_handle.offset().y);
    let max_offset_y = f32::from(scroll_handle.max_offset().y);
    let next_offset_y = vertical_scroll_offset_to_reveal_caret(
        current_offset_y,
        max_offset_y,
        VerticalRange {
            top_px: f32::from(viewport_bounds.top()),
            bottom_px: f32::from(viewport_bounds.bottom()),
        },
        VerticalRange {
            top_px: f32::from(caret_bounds.top()),
            bottom_px: f32::from(caret_bounds.bottom()),
        },
        CaretRevealMargins {
            top_px: CARET_REVEAL_TOP_MARGIN_PX,
            bottom_px: CARET_REVEAL_BOTTOM_MARGIN_PX,
        },
    );
    if (next_offset_y - current_offset_y).abs() <= 0.5 {
        return;
    }
    let mut offset = scroll_handle.offset();
    offset.y = px(next_offset_y);
    scroll_handle.set_offset(offset);
    window.refresh();
}

fn vertical_scroll_offset_to_reveal_caret(
    current_offset_y: f32,
    max_offset_y: f32,
    viewport: VerticalRange,
    caret: VerticalRange,
    margins: CaretRevealMargins,
) -> f32 {
    if max_offset_y <= 0.5 || viewport.bottom_px <= viewport.top_px {
        return current_offset_y.clamp(-max_offset_y.max(0.0), 0.0);
    }
    let viewport_height_px = viewport.bottom_px - viewport.top_px;
    let top_margin_px = margins.top_px.max(0.0).min(viewport_height_px / 2.0);
    let bottom_margin_px = margins
        .bottom_px
        .max(0.0)
        .min(viewport_height_px - top_margin_px);
    let visible_top_px = viewport.top_px + top_margin_px;
    let visible_bottom_px = viewport.bottom_px - bottom_margin_px;
    let delta_y = if caret.top_px < visible_top_px {
        visible_top_px - caret.top_px
    } else if caret.bottom_px > visible_bottom_px {
        visible_bottom_px - caret.bottom_px
    } else {
        0.0
    };
    (current_offset_y + delta_y).clamp(-max_offset_y, 0.0)
}

#[cfg(test)]
mod tests {
    use super::{CaretRevealMargins, VerticalRange, vertical_scroll_offset_to_reveal_caret};

    const VIEWPORT: VerticalRange = VerticalRange {
        top_px: 100.0,
        bottom_px: 420.0,
    };
    const MARGINS: CaretRevealMargins = CaretRevealMargins {
        top_px: 8.0,
        bottom_px: 24.0,
    };

    const fn caret(top_px: f32, bottom_px: f32) -> VerticalRange {
        VerticalRange { top_px, bottom_px }
    }

    #[test]
    fn focused_caret_scrolls_only_the_internal_viewport_and_stays_padded() {
        assert_eq!(
            vertical_scroll_offset_to_reveal_caret(
                0.0,
                400.0,
                VIEWPORT,
                caret(430.0, 454.0),
                MARGINS
            ),
            -58.0
        );
        assert_eq!(
            vertical_scroll_offset_to_reveal_caret(
                -200.0,
                400.0,
                VIEWPORT,
                caret(50.0, 74.0),
                MARGINS
            ),
            -142.0
        );
        assert_eq!(
            vertical_scroll_offset_to_reveal_caret(
                -120.0,
                400.0,
                VIEWPORT,
                caret(180.0, 204.0),
                MARGINS
            ),
            -120.0
        );
    }

    #[test]
    fn caret_auto_scroll_clamps_to_the_internal_content_range() {
        assert_eq!(
            vertical_scroll_offset_to_reveal_caret(
                -390.0,
                400.0,
                VIEWPORT,
                caret(900.0, 924.0),
                MARGINS
            ),
            -400.0
        );
        assert_eq!(
            vertical_scroll_offset_to_reveal_caret(
                -10.0,
                400.0,
                VIEWPORT,
                caret(0.0, 24.0),
                MARGINS
            ),
            0.0
        );
    }
}
