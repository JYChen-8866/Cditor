use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div,
    px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use cditor_core::ids::BlockId;
use cditor_runtime::{TableCellPosition, TableViewState};

use super::selection::{TableAxis, TableAxisSelection, TableCellRangeSelection};
use super::style::{
    TABLE_AXIS_COLUMN_HANDLE_TOP_PX, TABLE_AXIS_HANDLE_SIZE_PX, TABLE_AXIS_OUTLINE_THICKNESS_PX,
    TABLE_AXIS_ROW_HANDLE_LEFT_PX, TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX,
    TABLE_CELL_GUTTER_SIZE_PX, TABLE_CELL_GUTTER_THICKNESS_PX, table_active_border_color,
    table_axis_handle_background, table_axis_handle_foreground,
};

const TABLE_AXIS_HANDLE_DOT_ROWS: usize = 3;
const TABLE_AXIS_HANDLE_DOT_COLUMNS: usize = 2;
use super::toolbar::TableToolbarEditorOrigin;

pub(crate) fn render_table_axis_overlays(
    block_id: BlockId,
    table_view: &TableViewState,
    selection: Option<TableAxisSelection>,
    range_selection: Option<TableCellRangeSelection>,
    focused_cell: Option<TableCellPosition>,
    row_track_sizes: &[f32],
    column_track_sizes: &[f32],
    origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> Vec<AnyElement> {
    let mut overlays = Vec::new();
    overlays.extend(render_table_axis_handles(
        block_id,
        table_view,
        selection,
        row_track_sizes,
        column_track_sizes,
        origin,
        theme,
        view.clone(),
    ));
    if let Some(selection) = selection.filter(|selection| selection.block_id == block_id)
        && let Some(rect) = table_axis_selection_outline_rect(table_view, selection)
    {
        overlays.push(render_table_axis_outline(rect, origin, theme));
    }
    if let Some(range_sel) = range_selection.filter(|s| s.block_id == block_id && s.is_multi_cell())
        && let Some(rect) = table_range_selection_outline_rect(table_view, range_sel)
    {
        overlays.push(render_table_axis_outline(rect, origin, theme));
    }
    if let Some(focused_cell) = focused_cell
        && let Some(cell_rect) = table_cell_rect(table_view, focused_cell.row, focused_cell.col)
    {
        let gutters = active_cell_gutter_geometries(
            focused_cell,
            cell_rect,
            table_view.horizontal_scroll_offset_px,
        );
        overlays.extend(gutters.into_iter().map(|gutter| {
            render_active_cell_gutter(gutter, origin, theme, view.clone(), block_id)
        }));
    }
    overlays
}

fn render_table_axis_handles(
    block_id: BlockId,
    table_view: &TableViewState,
    selection: Option<TableAxisSelection>,
    row_track_sizes: &[f32],
    column_track_sizes: &[f32],
    origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> Vec<AnyElement> {
    let mut handles = Vec::new();
    for row in 0..table_view.row_count {
        if let Some(rect) = table_axis_track_rect(table_view, TableAxis::Row, row) {
            handles.push(render_table_axis_handle(
                block_id,
                TableAxis::Row,
                row,
                rect,
                selection.is_some_and(|selection| selection.selects_row_handle(block_id, row)),
                row_track_sizes.to_vec(),
                origin,
                theme,
                view.clone(),
            ));
        }
    }
    for col in 0..table_view.col_count {
        if let Some(rect) = table_axis_track_rect(table_view, TableAxis::Column, col) {
            handles.push(render_table_axis_handle(
                block_id,
                TableAxis::Column,
                col,
                rect,
                selection.is_some_and(|selection| selection.selects_column_handle(block_id, col)),
                column_track_sizes.to_vec(),
                origin,
                theme,
                view.clone(),
            ));
        }
    }
    handles
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TableOverlayRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

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

fn table_cell_rect(
    table_view: &TableViewState,
    row: usize,
    col: usize,
) -> Option<TableOverlayRect> {
    table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position.row == row && cell.position.col == col)
        .map(|cell| TableOverlayRect {
            x: table_viewport_x(table_view, cell.x_px),
            y: cell.y_px,
            width: cell.width_px,
            height: cell.height_px,
        })
}

fn table_axis_track_rect(
    table_view: &TableViewState,
    axis: TableAxis,
    index: usize,
) -> Option<TableOverlayRect> {
    match axis {
        TableAxis::Row => table_view
            .visible_cells
            .iter()
            .find(|cell| cell.position.row == index)
            .map(|cell| TableOverlayRect {
                x: table_view.horizontal_scroll_offset_px,
                y: cell.y_px,
                width: table_view.width_px,
                height: cell.height_px,
            }),
        TableAxis::Column => table_view
            .visible_cells
            .iter()
            .find(|cell| cell.position.col == index)
            .map(|cell| TableOverlayRect {
                x: table_viewport_x(table_view, cell.x_px),
                y: 0.0,
                width: cell.width_px,
                height: table_view.height_px,
            }),
    }
}

fn table_viewport_x(table_view: &TableViewState, table_local_x: f32) -> f32 {
    table_local_x + table_view.horizontal_scroll_offset_px
}

fn active_cell_gutter_geometries(
    focused_cell: TableCellPosition,
    cell_rect: TableOverlayRect,
    table_left_px: f32,
) -> Vec<ActiveCellGutterGeometry> {
    const HITBOX_LONG_EDGE_PX: f32 = 24.0;
    const INDICATOR_LONG_EDGE_PX: f32 = 14.0;

    let mut gutters = Vec::with_capacity(if focused_cell.row == 0 { 2 } else { 1 });
    if focused_cell.row == 0 {
        let hitbox = TableOverlayRect {
            x: cell_rect.x + cell_rect.width / 2.0 - HITBOX_LONG_EDGE_PX / 2.0,
            y: cell_rect.y - TABLE_CELL_GUTTER_SIZE_PX / 2.0,
            width: HITBOX_LONG_EDGE_PX,
            height: TABLE_CELL_GUTTER_SIZE_PX,
        };
        gutters.push(ActiveCellGutterGeometry {
            axis: TableAxis::Column,
            index: focused_cell.col,
            orientation: TableCellGutterOrientation::Horizontal,
            hitbox,
            indicator: TableOverlayRect {
                x: (hitbox.width - INDICATOR_LONG_EDGE_PX) / 2.0,
                y: (hitbox.height - TABLE_CELL_GUTTER_THICKNESS_PX) / 2.0,
                width: INDICATOR_LONG_EDGE_PX,
                height: TABLE_CELL_GUTTER_THICKNESS_PX,
            },
        });
    }

    let hitbox = TableOverlayRect {
        // Every row, including the first one, keeps its row gutter on the
        // table's outer left border rather than an internal column boundary.
        x: table_left_px - TABLE_CELL_GUTTER_SIZE_PX / 2.0,
        y: cell_rect.y + cell_rect.height / 2.0 - HITBOX_LONG_EDGE_PX / 2.0,
        width: TABLE_CELL_GUTTER_SIZE_PX,
        height: HITBOX_LONG_EDGE_PX,
    };
    gutters.push(ActiveCellGutterGeometry {
        axis: TableAxis::Row,
        index: focused_cell.row,
        orientation: TableCellGutterOrientation::Vertical,
        hitbox,
        indicator: TableOverlayRect {
            x: (hitbox.width - TABLE_CELL_GUTTER_THICKNESS_PX) / 2.0,
            y: (hitbox.height - INDICATOR_LONG_EDGE_PX) / 2.0,
            width: TABLE_CELL_GUTTER_THICKNESS_PX,
            height: INDICATOR_LONG_EDGE_PX,
        },
    });
    gutters
}

fn table_axis_selection_rect(
    table_view: &TableViewState,
    selection: TableAxisSelection,
) -> Option<TableOverlayRect> {
    table_axis_track_rect(table_view, selection.axis, selection.index)
}

fn table_axis_selection_outline_rect(
    table_view: &TableViewState,
    selection: TableAxisSelection,
) -> Option<TableOverlayRect> {
    let rect = table_axis_selection_rect(table_view, selection)?;
    // Use inner stroke: the outline border is drawn inside the cell bounds.
    // This avoids the last-row clipping issue where expanding outward by `half`
    // gets clamped to table_view.height_px, making the bottom border invisible.
    let left = rect.x.max(0.0);
    let right = (rect.x + rect.width).min(table_view.width_px);
    Some(TableOverlayRect {
        x: left,
        y: rect.y.max(0.0),
        width: (right - left).max(0.0),
        height: rect.height.min(table_view.height_px - rect.y.max(0.0)),
    })
}

fn table_range_selection_outline_rect(
    table_view: &TableViewState,
    selection: TableCellRangeSelection,
) -> Option<TableOverlayRect> {
    let top_left = table_cell_rect(
        table_view,
        selection.range.start_row,
        selection.range.start_col,
    )?;
    let bottom_right =
        table_cell_rect(table_view, selection.range.end_row, selection.range.end_col)?;
    // Use inner stroke: the outline border is drawn inside the selection bounds.
    // This avoids the last-row clipping issue where expanding outward would get
    // clamped to table_view.height_px, making the bottom border invisible.
    let x = top_left.x.max(0.0);
    let y = top_left.y.max(0.0);
    let right = (bottom_right.x + bottom_right.width).min(table_view.width_px);
    let bottom = (bottom_right.y + bottom_right.height).min(table_view.height_px);
    Some(TableOverlayRect {
        x,
        y,
        width: (right - x).max(0.0),
        height: (bottom - y).max(0.0),
    })
}

#[cfg(test)]
fn table_overlay_left_in_editor(rect: TableOverlayRect, origin: TableToolbarEditorOrigin) -> f32 {
    origin.x_px + rect.x
}

fn render_table_axis_handle(
    block_id: BlockId,
    axis: TableAxis,
    index: usize,
    rect: TableOverlayRect,
    selected: bool,
    track_sizes_px: Vec<f32>,
    origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let (width, height) = match (axis, selected) {
        (TableAxis::Row, true) => (
            TABLE_AXIS_HANDLE_SIZE_PX,
            TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX,
        ),
        (TableAxis::Column, true) => (
            TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX,
            TABLE_AXIS_HANDLE_SIZE_PX,
        ),
        _ => (TABLE_AXIS_HANDLE_SIZE_PX, TABLE_AXIS_HANDLE_SIZE_PX),
    };
    let (left, top) = match axis {
        TableAxis::Row => (
            TABLE_AXIS_ROW_HANDLE_LEFT_PX,
            rect.y + rect.height / 2.0 - height / 2.0,
        ),
        TableAxis::Column => (
            rect.x + rect.width / 2.0 - width / 2.0,
            TABLE_AXIS_COLUMN_HANDLE_TOP_PX,
        ),
    };
    div()
        .absolute()
        .left(px(origin.x_px + left))
        .top(px(origin.y_px + top))
        .w(px(width))
        .h(px(height))
        .rounded(px(5.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(table_axis_handle_background(theme, selected)))
        .cursor_pointer()
        .when(!selected, |this| this.opacity(0.0))
        .when(selected, |this| this.opacity(1.0))
        .hover(move |style| style.opacity(1.0).bg(rgb(theme.action_hover_background)))
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.select_table_axis_from_gui(block_id, axis, index, window, cx);
                view.start_table_reorder_from_gui(
                    block_id,
                    axis,
                    index,
                    track_sizes_px.clone(),
                    event.position,
                    window,
                    cx,
                );
            });
            cx.stop_propagation();
        })
        .child(render_table_axis_handle_icon(table_axis_handle_foreground(
            theme, selected,
        )))
        .into_any_element()
}

fn render_table_axis_handle_icon(color: u32) -> AnyElement {
    div()
        .w(px(10.0))
        .h(px(10.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(1.5))
        .children((0..TABLE_AXIS_HANDLE_DOT_ROWS).map(move |_| {
            div().flex().gap(px(1.5)).children(
                (0..TABLE_AXIS_HANDLE_DOT_COLUMNS)
                    .map(move |_| div().w(px(2.0)).h(px(2.0)).rounded(px(2.0)).bg(rgb(color))),
            )
        }))
        .into_any_element()
}

fn render_table_axis_outline(
    rect: TableOverlayRect,
    origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
) -> AnyElement {
    div()
        .absolute()
        .left(px(origin.x_px + rect.x))
        .top(px(origin.y_px + rect.y))
        .w(px(rect.width))
        .h(px(rect.height))
        .border(px(TABLE_AXIS_OUTLINE_THICKNESS_PX))
        .border_color(rgb(table_active_border_color(theme)))
        .into_any_element()
}

fn render_active_cell_gutter(
    geometry: ActiveCellGutterGeometry,
    origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    block_id: BlockId,
) -> AnyElement {
    div()
        .absolute()
        .left(px(origin.x_px + geometry.hitbox.x))
        .top(px(origin.y_px + geometry.hitbox.y))
        .w(px(geometry.hitbox.width))
        .h(px(geometry.hitbox.height))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.select_table_axis_from_gui(
                    block_id,
                    geometry.axis,
                    geometry.index,
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
                .rounded(px(TABLE_CELL_GUTTER_THICKNESS_PX)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{TableCellAlign, TablePayload};
    use cditor_runtime::{TableCellPosition, TableVisibleCell};

    use super::*;

    #[test]
    fn table_axis_handle_uses_notion_six_dot_grip() {
        assert_eq!(
            TABLE_AXIS_HANDLE_DOT_ROWS * TABLE_AXIS_HANDLE_DOT_COLUMNS,
            6
        );
    }

    #[test]
    fn table_overlay_rects_use_cell_geometry_directly() {
        let table_view = table_view_with_two_by_two_cells();

        assert_eq!(
            table_cell_rect(&table_view, 1, 1),
            Some(TableOverlayRect {
                x: 120.0,
                y: 36.0,
                width: 120.0,
                height: 36.0,
            })
        );
        assert_eq!(
            table_axis_track_rect(&table_view, TableAxis::Row, 1),
            Some(TableOverlayRect {
                x: 0.0,
                y: 36.0,
                width: 240.0,
                height: 36.0,
            })
        );
        assert_eq!(
            table_axis_track_rect(&table_view, TableAxis::Column, 1),
            Some(TableOverlayRect {
                x: 120.0,
                y: 0.0,
                width: 120.0,
                height: 72.0,
            })
        );
    }

    #[test]
    fn table_axis_selection_rect_uses_selected_axis_geometry() {
        let table_view = table_view_with_two_by_two_cells();

        assert_eq!(
            table_axis_selection_rect(
                &table_view,
                TableAxisSelection::new(7, TableAxis::Column, 0),
            ),
            Some(TableOverlayRect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 72.0,
            })
        );
        assert_eq!(
            table_axis_selection_rect(&table_view, TableAxisSelection::new(7, TableAxis::Row, 9)),
            None
        );
    }

    #[test]
    fn table_axis_selection_outline_stays_inside_table_edges() {
        let table_view = table_view_with_two_by_two_cells();

        // Row 1: inner stroke means the outline stays exactly within cell bounds
        assert_eq!(
            table_axis_selection_outline_rect(
                &table_view,
                TableAxisSelection::new(7, TableAxis::Row, 1),
            ),
            Some(TableOverlayRect {
                x: 0.0,
                y: 36.0,
                width: 240.0,
                height: 36.0,
            })
        );
        // Column 1: inner stroke stays within column bounds
        assert_eq!(
            table_axis_selection_outline_rect(
                &table_view,
                TableAxisSelection::new(7, TableAxis::Column, 1),
            ),
            Some(TableOverlayRect {
                x: 120.0,
                y: 0.0,
                width: 120.0,
                height: 72.0,
            })
        );
    }

    #[test]
    fn table_axis_chrome_follows_horizontal_content_scroll() {
        let mut table_view = table_view_with_two_by_two_cells();
        table_view.horizontal_scroll_offset_px = -80.0;

        assert_eq!(
            table_cell_rect(&table_view, 0, 1),
            Some(TableOverlayRect {
                x: 40.0,
                y: 0.0,
                width: 120.0,
                height: 36.0,
            })
        );
        assert_eq!(
            table_axis_track_rect(&table_view, TableAxis::Column, 1),
            Some(TableOverlayRect {
                x: 40.0,
                y: 0.0,
                width: 120.0,
                height: 72.0,
            })
        );
        assert_eq!(
            table_axis_selection_outline_rect(
                &table_view,
                TableAxisSelection::new(7, TableAxis::Column, 0),
            ),
            Some(TableOverlayRect {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 72.0,
            })
        );
    }

    #[test]
    fn table_axis_overlay_editor_position_adds_content_origin_once() {
        let table_view = table_view_with_two_by_two_cells();
        let origin = TableToolbarEditorOrigin {
            x_px: 89.0,
            y_px: 129.0,
        };
        let rect = table_axis_track_rect(&table_view, TableAxis::Column, 0).unwrap();

        assert_eq!(rect.x, 0.0);
        assert_eq!(table_overlay_left_in_editor(rect, origin), 89.0);
    }

    #[test]
    fn first_row_cell_uses_both_top_column_and_left_row_gutters() {
        let table_view = table_view_with_two_by_two_cells();
        let cell = table_cell_rect(&table_view, 0, 1).unwrap();
        let gutters = active_cell_gutter_geometries(
            TableCellPosition { row: 0, col: 1 },
            cell,
            table_view.horizontal_scroll_offset_px,
        );
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
            column_gutter.hitbox.y + column_gutter.hitbox.height / 2.0,
            cell.y
        );
        assert_eq!(
            column_gutter.hitbox.y
                + column_gutter.indicator.y
                + column_gutter.indicator.height / 2.0,
            cell.y
        );
        assert_eq!(row_gutter.axis, TableAxis::Row);
        assert_eq!(row_gutter.index, 0);
        assert_eq!(row_gutter.orientation, TableCellGutterOrientation::Vertical);
        assert_eq!(
            row_gutter.hitbox.x + row_gutter.hitbox.width / 2.0,
            table_view.horizontal_scroll_offset_px
        );
    }

    #[test]
    fn later_rows_use_only_a_vertical_row_gutter_on_the_table_left_border() {
        let mut table_view = table_view_with_two_by_two_cells();
        table_view.horizontal_scroll_offset_px = -24.0;
        let cell = table_cell_rect(&table_view, 1, 1).unwrap();
        let gutters = active_cell_gutter_geometries(
            TableCellPosition { row: 1, col: 1 },
            cell,
            table_view.horizontal_scroll_offset_px,
        );
        assert_eq!(gutters.len(), 1);
        let gutter = gutters[0];

        assert_eq!(gutter.axis, TableAxis::Row);
        assert_eq!(gutter.index, 1);
        assert_eq!(gutter.orientation, TableCellGutterOrientation::Vertical);
        assert_ne!(cell.x, table_view.horizontal_scroll_offset_px);
        assert_eq!(
            gutter.hitbox.x + gutter.hitbox.width / 2.0,
            table_view.horizontal_scroll_offset_px
        );
        assert_eq!(
            gutter.hitbox.x + gutter.indicator.x + gutter.indicator.width / 2.0,
            table_view.horizontal_scroll_offset_px
        );
    }

    fn table_view_with_two_by_two_cells() -> TableViewState {
        TableViewState {
            table: TablePayload::default(),
            row_count: 2,
            col_count: 2,
            width_px: 240.0,
            height_px: 72.0,
            column_widths_px: vec![120.0, 120.0],
            row_heights_px: vec![36.0, 36.0],
            horizontal_scroll_offset_px: 0.0,
            visible_cells: vec![
                visible_cell(0, 0, 0.0, 0.0),
                visible_cell(0, 1, 120.0, 0.0),
                visible_cell(1, 0, 0.0, 36.0),
                visible_cell(1, 1, 120.0, 36.0),
            ],
            focused_cell: None,
            focused_cell_offset: None,
            focused_cell_selection_range: None,
        }
    }

    fn visible_cell(row: usize, col: usize, x_px: f32, y_px: f32) -> TableVisibleCell {
        TableVisibleCell {
            position: TableCellPosition { row, col },
            row_span: 1,
            col_span: 1,
            x_px,
            y_px,
            width_px: 120.0,
            height_px: 36.0,
            header: false,
            align: TableCellAlign::Left,
            background_color: None,
            spans: Vec::new(),
        }
    }
}
