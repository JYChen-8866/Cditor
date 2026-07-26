use cditor_runtime::TableViewState;

use super::super::selection::{TableAxis, TableAxisSelection, TableCellRangeSelection};
#[cfg(test)]
use super::super::toolbar::TableToolbarEditorOrigin;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::features::table) struct TableOverlayRect {
    pub(in crate::features::table) x: f32,
    pub(in crate::features::table) y: f32,
    pub(in crate::features::table) width: f32,
    pub(in crate::features::table) height: f32,
}

pub(super) fn table_cell_rect(
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

pub(in crate::features::table) fn table_content_cell_rect(
    table_view: &TableViewState,
    row: usize,
    col: usize,
) -> Option<TableOverlayRect> {
    table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position.row == row && cell.position.col == col)
        .map(|cell| TableOverlayRect {
            x: cell.x_px,
            y: cell.y_px,
            width: cell.width_px,
            height: cell.height_px,
        })
}

pub(super) fn table_axis_track_rect(
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

pub(super) fn table_axis_selection_rect(
    table_view: &TableViewState,
    selection: TableAxisSelection,
) -> Option<TableOverlayRect> {
    table_axis_track_rect(table_view, selection.axis, selection.index)
}

pub(super) fn table_axis_selection_outline_rect(
    table_view: &TableViewState,
    selection: TableAxisSelection,
) -> Option<TableOverlayRect> {
    let rect = table_axis_selection_rect(table_view, selection)?;
    let left = rect.x.max(0.0);
    let right = (rect.x + rect.width).min(table_view.width_px);
    Some(TableOverlayRect {
        x: left,
        y: rect.y.max(0.0),
        width: (right - left).max(0.0),
        height: rect.height.min(table_view.height_px - rect.y.max(0.0)),
    })
}

pub(super) fn table_range_selection_outline_rect(
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
pub(super) fn table_overlay_left_in_editor(
    rect: TableOverlayRect,
    origin: TableToolbarEditorOrigin,
) -> f32 {
    origin.x_px + rect.x
}
