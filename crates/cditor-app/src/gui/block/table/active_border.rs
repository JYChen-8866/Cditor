use std::ops::Range;

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;

use super::chrome::TableOverlayRect;
use super::style::{
    TABLE_ACTIVE_CELL_BORDER_WIDTH_PX, TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX,
    TABLE_CELL_GUTTER_SEGMENT_GAP_PX, table_active_border_color,
};
use super::toolbar::TableToolbarEditorOrigin;

pub(super) fn render_active_cell_border(
    rect: TableOverlayRect,
    origin: TableToolbarEditorOrigin,
    split_top: bool,
    split_left: bool,
    theme: GuiTheme,
) -> AnyElement {
    let width = rect.width;
    let height = rect.height;
    let color = table_active_border_color(theme);
    let top = segmented_edge_ranges(width, split_top);
    let bottom = segmented_edge_ranges(width, false);
    let left = segmented_edge_ranges(height, split_left);
    let right = segmented_edge_ranges(height, true);

    div()
        .absolute()
        .left(px(origin.x_px + rect.x))
        .top(px(origin.y_px + rect.y))
        .w(px(width))
        .h(px(height))
        .children(
            top.into_iter()
                .map(move |range| horizontal_border_segment(range, 0.0, color)),
        )
        .children(
            bottom.into_iter().map(move |range| {
                horizontal_border_segment(range, trailing_edge_start(height), color)
            }),
        )
        .children(
            left.into_iter()
                .map(move |range| vertical_border_segment(range, 0.0, color)),
        )
        .children(
            right.into_iter().map(move |range| {
                vertical_border_segment(range, trailing_edge_start(width), color)
            }),
        )
        .into_any_element()
}

fn trailing_edge_start(length: f32) -> f32 {
    (length - TABLE_ACTIVE_CELL_BORDER_WIDTH_PX).max(0.0)
}

fn horizontal_border_segment(range: Range<f32>, top: f32, color: u32) -> AnyElement {
    div()
        .absolute()
        .left(px(range.start))
        .top(px(top))
        .w(px((range.end - range.start).max(0.0)))
        .h(px(TABLE_ACTIVE_CELL_BORDER_WIDTH_PX))
        .bg(rgb(color))
        .into_any_element()
}

fn vertical_border_segment(range: Range<f32>, left: f32, color: u32) -> AnyElement {
    div()
        .absolute()
        .left(px(left))
        .top(px(range.start))
        .w(px(TABLE_ACTIVE_CELL_BORDER_WIDTH_PX))
        .h(px((range.end - range.start).max(0.0)))
        .bg(rgb(color))
        .into_any_element()
}

fn segmented_edge_ranges(length: f32, split: bool) -> Vec<Range<f32>> {
    let length = length.max(0.0);
    if !split {
        return vec![0.0..length];
    }
    let break_length =
        TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX + TABLE_CELL_GUTTER_SEGMENT_GAP_PX * 2.0;
    if length <= break_length {
        return Vec::new();
    }
    let break_start = (length - break_length) / 2.0;
    let break_end = break_start + break_length;
    vec![0.0..break_start, break_end..length]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_border_is_split_around_the_gutter_instead_of_drawn_under_it() {
        assert_eq!(
            segmented_edge_ranges(120.0, true),
            vec![0.0..50.0, 70.0..120.0]
        );
        assert_eq!(TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX, 14.0);
        assert_eq!(TABLE_CELL_GUTTER_SEGMENT_GAP_PX, 3.0);
    }

    #[test]
    fn unsplit_and_too_short_edges_have_stable_ranges() {
        assert_eq!(segmented_edge_ranges(36.0, false), vec![0.0..36.0]);
        assert!(segmented_edge_ranges(20.0, true).is_empty());
    }

    #[test]
    fn trailing_edges_stay_inside_the_shared_overlay_rect() {
        assert_eq!(trailing_edge_start(120.0), 118.0);
        assert_eq!(trailing_edge_start(36.0), 34.0);
        assert_eq!(
            trailing_edge_start(36.0) + TABLE_ACTIVE_CELL_BORDER_WIDTH_PX,
            36.0
        );
        assert_eq!(segmented_edge_ranges(36.0, true)[1].end, 36.0);
    }
}
