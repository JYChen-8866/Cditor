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
    TABLE_CELL_GUTTER_SIZE_PX, TABLE_CELL_GUTTER_THICKNESS_PX, table_active_border_color,
    table_axis_handle_background, table_axis_handle_foreground, table_axis_handle_hover_background,
};
use super::toolbar::TableToolbarEditorOrigin;

#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveCellMenuHandleGeometry {
    row: usize,
    col: usize,
    hitbox: TableOverlayRect,
    indicator: TableOverlayRect,
}

pub(super) fn render_active_cell_menu_handle(
    focused_cell: TableCellPosition,
    active_border_rect: TableOverlayRect,
    selected: bool,
    origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    block_id: BlockId,
) -> AnyElement {
    const HOVER_GROUP: &str = "active-cell-menu-gutter";
    let geometry = active_cell_menu_handle_geometry(focused_cell, active_border_rect);
    let indicator = if selected {
        let gutter = selected_cell_menu_gutter_geometry(geometry.hitbox);
        div()
            .absolute()
            .left(px(gutter.x))
            .top(px(gutter.y))
            .w(px(gutter.width))
            .h(px(gutter.height))
            .rounded(px(TABLE_AXIS_HANDLE_RADIUS_PX))
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(table_axis_handle_background(theme, true)))
            .child(render_table_axis_handle_icon(
                TableAxis::Row,
                table_axis_handle_foreground(theme, true),
            ))
            .into_any_element()
    } else {
        let gutter = selected_cell_menu_gutter_geometry(geometry.hitbox);
        div()
            .w_full()
            .h_full()
            .child(
                div()
                    .absolute()
                    .left(px(geometry.indicator.x))
                    .top(px(geometry.indicator.y))
                    .w(px(geometry.indicator.width))
                    .h(px(geometry.indicator.height))
                    .bg(rgb(table_active_border_color(theme)))
                    .rounded(px(TABLE_CELL_GUTTER_THICKNESS_PX))
                    .group_hover(HOVER_GROUP, |style| style.opacity(0.0)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(gutter.x))
                    .top(px(gutter.y))
                    .w(px(gutter.width))
                    .h(px(gutter.height))
                    .rounded(px(TABLE_AXIS_HANDLE_RADIUS_PX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(table_axis_handle_hover_background(theme, true)))
                    .opacity(0.0)
                    .group_hover(HOVER_GROUP, |style| style.opacity(1.0))
                    .child(render_table_axis_handle_icon(
                        TableAxis::Row,
                        table_axis_handle_foreground(theme, true),
                    )),
            )
            .into_any_element()
    };
    div()
        .group(HOVER_GROUP)
        .absolute()
        .left(px(origin.x_px + geometry.hitbox.x))
        .top(px(origin.y_px + geometry.hitbox.y))
        .w(px(geometry.hitbox.width))
        .h(px(geometry.hitbox.height))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_up(MouseButton::Left, move |_event, window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.open_table_cell_menu_from_gui(
                    block_id,
                    geometry.row,
                    geometry.col,
                    window,
                    cx,
                );
            });
            cx.stop_propagation();
        })
        .child(indicator)
        .into_any_element()
}

fn selected_cell_menu_gutter_geometry(hitbox: TableOverlayRect) -> TableOverlayRect {
    let (width, height) = table_axis_handle_dimensions(TableAxis::Row, true);
    TableOverlayRect {
        x: (hitbox.width - width) / 2.0,
        y: (hitbox.height - height) / 2.0,
        width,
        height,
    }
}

fn active_cell_menu_handle_geometry(
    focused_cell: TableCellPosition,
    active_border_rect: TableOverlayRect,
) -> ActiveCellMenuHandleGeometry {
    const HITBOX_LONG_EDGE_PX: f32 = 24.0;
    let hitbox = TableOverlayRect {
        x: active_border_rect.x + active_border_rect.width - TABLE_CELL_GUTTER_SIZE_PX / 2.0,
        y: active_border_rect.y + active_border_rect.height / 2.0 - HITBOX_LONG_EDGE_PX / 2.0,
        width: TABLE_CELL_GUTTER_SIZE_PX,
        height: HITBOX_LONG_EDGE_PX,
    };
    ActiveCellMenuHandleGeometry {
        row: focused_cell.row,
        col: focused_cell.col,
        hitbox,
        indicator: TableOverlayRect {
            // The short bar occupies the inward half of the border; the hover
            // gutter then expands around the same exact border center.
            x: hitbox.width / 2.0 - TABLE_CELL_GUTTER_THICKNESS_PX,
            y: (hitbox.height - TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX) / 2.0,
            width: TABLE_CELL_GUTTER_THICKNESS_PX,
            height: TABLE_CELL_GUTTER_INDICATOR_LONG_EDGE_PX,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_menu_handle_is_centered_on_the_active_cell_right_border() {
        let geometry = active_cell_menu_handle_geometry(
            TableCellPosition { row: 2, col: 3 },
            TableOverlayRect {
                x: 120.0,
                y: 36.0,
                width: 120.0,
                height: 36.0,
            },
        );

        assert_eq!((geometry.row, geometry.col), (2, 3));
        assert_eq!(geometry.hitbox.x + geometry.hitbox.width / 2.0, 240.0);
        assert_eq!(geometry.hitbox.y + geometry.hitbox.height / 2.0, 54.0);
        assert_eq!(
            geometry.hitbox.x + geometry.indicator.x,
            240.0 - TABLE_CELL_GUTTER_THICKNESS_PX
        );
        assert_eq!(
            geometry.hitbox.x + geometry.indicator.x + geometry.indicator.width,
            240.0
        );
        assert_eq!(geometry.indicator.width, TABLE_CELL_GUTTER_THICKNESS_PX);
        assert_eq!(geometry.indicator.height, 14.0);
    }

    #[test]
    fn clicked_cell_handle_replaces_the_bar_with_a_centered_gutter() {
        let gutter = selected_cell_menu_gutter_geometry(TableOverlayRect {
            x: 0.0,
            y: 0.0,
            width: 14.0,
            height: 24.0,
        });

        assert_eq!(
            gutter,
            TableOverlayRect {
                x: 0.0,
                y: 1.0,
                width: 14.0,
                height: 22.0,
            }
        );
        let theme = GuiTheme::light();
        assert_eq!(
            table_axis_handle_background(theme, true),
            table_active_border_color(theme)
        );
    }
}
