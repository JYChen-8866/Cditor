use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, ScrollHandle,
    Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::table::TableToolbarEditorOrigin;
use cditor_core::ids::BlockId;
#[cfg(test)]
use cditor_core::layout::TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX;
use cditor_runtime::TableViewState;

const TABLE_HSCROLLBAR_HEIGHT_PX: f32 = 8.0;
const TABLE_HSCROLLBAR_MIN_THUMB_PX: f32 = 32.0;
const TABLE_HSCROLLBAR_TOP_GAP_PX: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TableViewportMeasurement {
    pub viewport_width_px: f32,
}

/// Geometry of the horizontal scrollbar thumb, in viewport-local pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TableHScrollThumb {
    pub width_px: f32,
    pub left_px: f32,
}

pub(crate) fn table_viewport_measurement_from_handle(
    handle: &ScrollHandle,
) -> Option<TableViewportMeasurement> {
    let viewport_width_px = f32::from(handle.bounds().size.width);
    (viewport_width_px > 0.5).then_some(TableViewportMeasurement { viewport_width_px })
}

pub(crate) fn render_table_horizontal_scrollbar(
    block_id: BlockId,
    table_view: &TableViewState,
    origin: TableToolbarEditorOrigin,
    measurement: TableViewportMeasurement,
    offset_x: f32,
    table_gutter_px: f32,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> Option<AnyElement> {
    let track_width_px = table_hscroll_track_width(measurement.viewport_width_px, table_gutter_px);
    let max_offset_x = table_hscroll_scroll_max(table_view.width_px, track_width_px);
    let thumb = table_hscroll_thumb(track_width_px, table_view.width_px, max_offset_x, offset_x)?;
    let thumb_travel_px = table_hscroll_thumb_travel(track_width_px, thumb.width_px);
    let top_px = origin.y_px + table_view.height_px + TABLE_HSCROLLBAR_TOP_GAP_PX;

    Some(
        div()
            .absolute()
            .left(px(origin.x_px))
            .top(px(top_px))
            .w(px(track_width_px))
            .h(px(TABLE_HSCROLLBAR_HEIGHT_PX))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(thumb.left_px))
                    .w(px(thumb.width_px))
                    .h(px(TABLE_HSCROLLBAR_HEIGHT_PX))
                    .rounded(px(TABLE_HSCROLLBAR_HEIGHT_PX / 2.0))
                    .cursor_pointer()
                    .bg(rgb(theme.scrollbar))
                    .hover(move |style| style.bg(rgb(theme.scrollbar_hover)))
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.start_table_hscroll_drag_from_gui(
                                block_id,
                                event.position,
                                max_offset_x,
                                thumb_travel_px,
                                window,
                                cx,
                            );
                        });
                        cx.stop_propagation();
                    }),
            )
            .into_any_element(),
    )
}

pub(crate) fn table_hscroll_track_width(viewport_width: f32, table_gutter_px: f32) -> f32 {
    (viewport_width - table_gutter_px).max(0.0)
}

pub(crate) fn table_hscroll_scroll_max(content_width: f32, track_width: f32) -> f32 {
    (content_width - track_width).max(0.0)
}

pub(crate) fn table_hscroll_thumb(
    track_width: f32,
    content_width: f32,
    max_offset_x: f32,
    offset_x: f32,
) -> Option<TableHScrollThumb> {
    if track_width <= 0.0 || max_offset_x <= 0.5 || content_width <= track_width {
        return None;
    }
    let visible_fraction = (track_width / content_width).clamp(0.0, 1.0);
    let width_px = (track_width * visible_fraction).max(TABLE_HSCROLLBAR_MIN_THUMB_PX);
    let travel = table_hscroll_thumb_travel(track_width, width_px);
    let scrolled = (-offset_x).clamp(0.0, max_offset_x);
    let progress = scrolled / max_offset_x;
    let left_px = travel * progress;
    Some(TableHScrollThumb { width_px, left_px })
}

#[cfg(test)]
pub(crate) fn table_hscroll_block_height(table_height: f32) -> f32 {
    table_height + TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX as f32
}

pub(crate) fn table_hscroll_thumb_travel(track_width: f32, thumb_width: f32) -> f32 {
    (track_width - thumb_width).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_hscrollbar_uses_stable_viewport_measurement_not_handle_max_offset() {
        let track_width = table_hscroll_track_width(628.0, 28.0);
        let max_offset = table_hscroll_scroll_max(1200.0, track_width);
        let thumb = table_hscroll_thumb(track_width, 1200.0, max_offset, 0.0).unwrap();

        assert_eq!(track_width, 600.0);
        assert_eq!(max_offset, 600.0);
        assert_eq!(thumb.width_px, 300.0);
        assert_eq!(thumb.left_px, 0.0);
    }

    #[test]
    fn table_hscrollbar_thumb_survives_zero_handle_measurement_when_cached() {
        let cached = TableViewportMeasurement {
            viewport_width_px: 628.0,
        };
        let track_width = table_hscroll_track_width(cached.viewport_width_px, 28.0);
        let max_offset = table_hscroll_scroll_max(1200.0, track_width);
        let thumb = table_hscroll_thumb(track_width, 1200.0, max_offset, -300.0).unwrap();

        assert_eq!(track_width, 600.0);
        assert_eq!(thumb.width_px, 300.0);
        assert_eq!(thumb.left_px, 150.0);
    }
}
