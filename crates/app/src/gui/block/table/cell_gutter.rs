use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div,
    px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use cditor_core::ids::BlockId;
use cditor_runtime::TableCellPosition;

use super::axis_grip::{render_table_axis_handle_icon, table_axis_handle_dimensions};
use super::chrome::TableOverlayRect;
use super::selection::TableAxis;
use super::style::{
    TABLE_AXIS_HANDLE_RADIUS_PX, TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX,
    TABLE_CELL_GUTTER_SIZE_PX, TABLE_CELL_GUTTER_THICKNESS_PX, table_axis_handle_foreground,
    table_axis_handle_hover_background,
};
use super::toolbar::TableToolbarEditorOrigin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableCellGutterOrientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveCellGutterGeometry {
    axis: TableAxis,
    index: usize,
    orientation: TableCellGutterOrientation,
    hitbox: TableOverlayRect,
    indicator: TableOverlayRect,
}

pub(super) struct ActiveCellGutterElements {
    pub(super) top_edge: Vec<AnyElement>,
    pub(super) left_edge: Vec<AnyElement>,
}

pub(super) fn render_active_cell_gutters(
    focused_cell: TableCellPosition,
    active_border_rect: TableOverlayRect,
    table_left_px: f32,
    row_track_sizes: &[f32],
    column_track_sizes: &[f32],
    top_origin: TableToolbarEditorOrigin,
    left_origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    block_id: BlockId,
) -> ActiveCellGutterElements {
    let mut elements = ActiveCellGutterElements {
        top_edge: Vec::new(),
        left_edge: Vec::new(),
    };
    for geometry in active_cell_gutter_geometries(focused_cell, active_border_rect, table_left_px) {
        let (origin, target) = match geometry.axis {
            TableAxis::Column => (top_origin, &mut elements.top_edge),
            TableAxis::Row => (left_origin, &mut elements.left_edge),
        };
        target.push(render_active_cell_gutter(
            geometry,
            match geometry.axis {
                TableAxis::Row => row_track_sizes.to_vec(),
                TableAxis::Column => column_track_sizes.to_vec(),
            },
            origin,
            theme,
            view.clone(),
            block_id,
        ));
    }
    elements
}

fn active_cell_gutter_geometries(
    focused_cell: TableCellPosition,
    active_border_rect: TableOverlayRect,
    table_left_px: f32,
) -> Vec<ActiveCellGutterGeometry> {
    const HITBOX_LONG_EDGE_PX: f32 = 24.0;

    let mut gutters = Vec::with_capacity(if focused_cell.row == 0 { 2 } else { 1 });
    if focused_cell.row == 0 {
        let hitbox = TableOverlayRect {
            x: active_border_rect.x + active_border_rect.width / 2.0 - HITBOX_LONG_EDGE_PX / 2.0,
            y: active_border_rect.y - TABLE_CELL_GUTTER_SIZE_PX / 2.0,
            width: HITBOX_LONG_EDGE_PX,
            height: TABLE_CELL_GUTTER_SIZE_PX,
        };
        gutters.push(ActiveCellGutterGeometry {
            axis: TableAxis::Column,
            index: focused_cell.col,
            orientation: TableCellGutterOrientation::Horizontal,
            hitbox,
            indicator: TableOverlayRect {
                x: (hitbox.width - TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX) / 2.0,
                // The gutter replaces the inward segment of the active border.
                y: hitbox.height / 2.0,
                width: TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX,
                height: TABLE_CELL_GUTTER_THICKNESS_PX,
            },
        });
    }

    let hitbox = TableOverlayRect {
        // Row gutters stay on the table's outer edge, not a column boundary.
        x: table_left_px - TABLE_CELL_GUTTER_SIZE_PX / 2.0,
        y: active_border_rect.y + active_border_rect.height / 2.0 - HITBOX_LONG_EDGE_PX / 2.0,
        width: TABLE_CELL_GUTTER_SIZE_PX,
        height: HITBOX_LONG_EDGE_PX,
    };
    gutters.push(ActiveCellGutterGeometry {
        axis: TableAxis::Row,
        index: focused_cell.row,
        orientation: TableCellGutterOrientation::Vertical,
        hitbox,
        indicator: TableOverlayRect {
            x: hitbox.width / 2.0,
            y: (hitbox.height - TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX) / 2.0,
            width: TABLE_CELL_GUTTER_THICKNESS_PX,
            height: TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX,
        },
    });
    gutters
}

fn render_active_cell_gutter(
    geometry: ActiveCellGutterGeometry,
    track_sizes_px: Vec<f32>,
    origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    block_id: BlockId,
) -> AnyElement {
    const HOVER_GROUP: &str = "active-cell-axis-gutter";
    let hover_gutter = hovered_cell_gutter_geometry(geometry);
    div()
        .group(HOVER_GROUP)
        .absolute()
        .left(px(origin.x_px + geometry.hitbox.x))
        .top(px(origin.y_px + geometry.hitbox.y))
        .w(px(geometry.hitbox.width))
        .h(px(geometry.hitbox.height))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.start_table_reorder_from_gui(
                    block_id,
                    geometry.axis,
                    geometry.index,
                    track_sizes_px.clone(),
                    event.position,
                    window,
                    cx,
                );
            });
            cx.stop_propagation();
        })
        .child(
            div()
                .absolute()
                .left(px(geometry.indicator.x))
                .top(px(geometry.indicator.y))
                .w(px(geometry.indicator.width))
                .h(px(geometry.indicator.height))
                .bg(rgb(theme.muted))
                .rounded(px(TABLE_CELL_GUTTER_THICKNESS_PX))
                .group_hover(HOVER_GROUP, |style| style.opacity(0.0)),
        )
        .child(
            div()
                .absolute()
                .left(px(hover_gutter.x))
                .top(px(hover_gutter.y))
                .w(px(hover_gutter.width))
                .h(px(hover_gutter.height))
                .rounded(px(TABLE_AXIS_HANDLE_RADIUS_PX))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(table_axis_handle_hover_background(theme, true)))
                .opacity(0.0)
                .group_hover(HOVER_GROUP, |style| style.opacity(1.0))
                .child(render_table_axis_handle_icon(
                    geometry.axis,
                    table_axis_handle_foreground(theme, true),
                )),
        )
        .into_any_element()
}

fn hovered_cell_gutter_geometry(geometry: ActiveCellGutterGeometry) -> TableOverlayRect {
    let (width, height) = table_axis_handle_dimensions(geometry.axis, true);
    TableOverlayRect {
        x: (geometry.hitbox.width - width) / 2.0,
        y: (geometry.hitbox.height - height) / 2.0,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_row_cell_has_top_and_left_gutters_on_the_active_border() {
        let active_border = TableOverlayRect {
            x: 120.0,
            y: 0.0,
            width: 120.0,
            height: 36.0,
        };
        let gutters =
            active_cell_gutter_geometries(TableCellPosition { row: 0, col: 1 }, active_border, 0.0);

        assert_eq!(gutters.len(), 2);
        let column_gutter = gutters[0];
        let row_gutter = gutters[1];
        assert_eq!(column_gutter.axis, TableAxis::Column);
        assert_eq!(column_gutter.index, 1);
        assert_eq!(
            column_gutter.orientation,
            TableCellGutterOrientation::Horizontal
        );
        assert_eq!(
            column_gutter.hitbox.y + column_gutter.indicator.y,
            active_border.y
        );
        assert_eq!(row_gutter.axis, TableAxis::Row);
        assert_eq!(row_gutter.index, 0);
        assert_eq!(row_gutter.orientation, TableCellGutterOrientation::Vertical);
        assert_eq!(row_gutter.hitbox.x + row_gutter.indicator.x, 0.0);
    }

    #[test]
    fn later_rows_only_have_a_left_gutter_on_the_active_border() {
        let active_border = TableOverlayRect {
            x: 96.0,
            y: 36.0,
            width: 120.0,
            height: 36.0,
        };
        let gutters = active_cell_gutter_geometries(
            TableCellPosition { row: 1, col: 1 },
            active_border,
            -24.0,
        );

        assert_eq!(gutters.len(), 1);
        let gutter = gutters[0];
        assert_eq!(gutter.axis, TableAxis::Row);
        assert_eq!(gutter.index, 1);
        assert_eq!(gutter.orientation, TableCellGutterOrientation::Vertical);
        assert_eq!(gutter.hitbox.x + gutter.indicator.x, -24.0);
        assert_eq!(
            gutter.hitbox.x + gutter.indicator.x + gutter.indicator.width,
            -24.0 + TABLE_CELL_GUTTER_THICKNESS_PX
        );
    }

    #[test]
    fn hovered_short_bars_expand_into_directional_axis_gutters() {
        let gutters = active_cell_gutter_geometries(
            TableCellPosition { row: 0, col: 1 },
            TableOverlayRect {
                x: 120.0,
                y: 0.0,
                width: 120.0,
                height: 36.0,
            },
            0.0,
        );

        assert_eq!(
            hovered_cell_gutter_geometry(gutters[0]),
            TableOverlayRect {
                x: 1.0,
                y: 0.0,
                width: 22.0,
                height: 14.0,
            }
        );
        assert_eq!(
            hovered_cell_gutter_geometry(gutters[1]),
            TableOverlayRect {
                x: 0.0,
                y: 1.0,
                width: 14.0,
                height: 22.0,
            }
        );
    }
}
