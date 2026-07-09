use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, ParentElement, Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::input::{
    begin_table_cell_range_selection_from_mouse, update_table_cell_range_selection_from_mouse,
};
use cditor_core::ids::BlockId;
#[cfg(test)]
use cditor_core::rich_text::TablePayload;
use cditor_runtime::{TableCellPosition, TableVisibleCell};

use super::selection::{
    TableAxis, TableAxisSelection, TableCellRangeSelection, cell_selected, column_handle_selected,
    row_handle_selected,
};
use super::style::{
    TABLE_ACTIVE_CELL_BORDER_WIDTH_PX, TABLE_AXIS_HANDLE_OFFSET_PX, TABLE_AXIS_HANDLE_SIZE_PX,
    V1_TABLE_CELL_PADDING_X_PX, V1_TABLE_CELL_PADDING_Y_PX, table_active_border_color,
    table_cell_background, table_cell_border_color, table_cell_line_height,
    table_selected_cell_background,
};

pub(super) fn render_table_cell(
    cell: &TableVisibleCell,
    content: AnyElement,
    theme: GuiTheme,
    focused_cell: Option<TableCellPosition>,
    table_selection: Option<TableAxisSelection>,
    table_range_selection: Option<TableCellRangeSelection>,
    row_track_sizes: &[f32],
    column_track_sizes: &[f32],
    view: Entity<CditorV2View>,
    block_id: BlockId,
) -> AnyElement {
    let row_index = cell.position.row;
    let cell_index = cell.position.col;
    let active = is_active_cell(focused_cell, row_index, cell_index);
    let selected = cell_selected(
        table_selection,
        table_range_selection,
        block_id,
        row_index,
        cell_index,
    );
    let row_selected = row_handle_selected(table_selection, block_id, row_index);
    let column_selected = column_handle_selected(table_selection, block_id, cell_index);
    let row_handle_active = row_selected && cell.x_px <= 0.0;
    let column_handle_active = column_selected && cell.y_px <= 0.0;
    let focus_view = view.clone();
    let range_hover_view = view.clone();
    let row_reorder_sizes = row_track_sizes.to_vec();
    let column_reorder_sizes = column_track_sizes.to_vec();
    div()
        .absolute()
        .left(px(cell.x_px))
        .top(px(cell.y_px))
        .w(px(cell.width_px))
        .h(px(cell.height_px))
        .child(
            div()
                .relative()
                .group("table-cell-axis")
                .w_full()
                .h_full()
                .min_h(table_cell_line_height())
                .px(px(V1_TABLE_CELL_PADDING_X_PX))
                .py(px(V1_TABLE_CELL_PADDING_Y_PX))
                .border_r_1()
                .border_color(rgb(table_cell_border_color(theme, selected)))
                .bg(rgb(if selected {
                    table_selected_cell_background(theme)
                } else {
                    table_cell_background(theme, cell.header, cell.background_color.as_deref())
                }))
                .cursor_text()
                .on_mouse_down(gpui::MouseButton::Left, move |event, window, cx| {
                    begin_table_cell_range_selection_from_mouse(
                        &focus_view,
                        block_id,
                        row_index,
                        cell_index,
                        event,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                })
                .on_mouse_move(move |event, _window, cx| {
                    update_table_cell_range_selection_from_mouse(
                        &range_hover_view,
                        block_id,
                        row_index,
                        cell_index,
                        event,
                        cx,
                    );
                })
                .child(content)
                .when(active, |this| {
                    this.child(
                        div()
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .w_full()
                            .h_full()
                            .border(px(TABLE_ACTIVE_CELL_BORDER_WIDTH_PX))
                            .border_color(rgb(table_active_border_color(theme))),
                    )
                })
                .child(render_table_axis_handle(
                    TableAxis::Column,
                    block_id,
                    cell_index,
                    column_handle_active,
                    column_reorder_sizes,
                    theme,
                    view.clone(),
                ))
                .child(render_table_axis_handle(
                    TableAxis::Row,
                    block_id,
                    row_index,
                    row_handle_active,
                    row_reorder_sizes,
                    theme,
                    view,
                )),
        )
        .into_any_element()
}

fn render_table_axis_handle(
    axis: TableAxis,
    block_id: BlockId,
    index: usize,
    selected: bool,
    track_sizes_px: Vec<f32>,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let color = if selected {
        theme.action_accent
    } else {
        theme.gutter_foreground
    };
    let background = if selected {
        theme.action_background
    } else {
        theme.gutter_background
    };
    div()
        .absolute()
        .w(px(TABLE_AXIS_HANDLE_SIZE_PX))
        .h(px(TABLE_AXIS_HANDLE_SIZE_PX))
        .rounded(px(5.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(background))
        .cursor_pointer()
        .when(!selected, |this| {
            this.opacity(0.0)
                .group_hover("table-cell-axis", |style| style.opacity(1.0))
        })
        .when(selected, |this| this.opacity(1.0))
        .when(axis == TableAxis::Column, |this| {
            this.top(px(TABLE_AXIS_HANDLE_OFFSET_PX)).left(px(8.0))
        })
        .when(axis == TableAxis::Row, |this| {
            this.left(px(TABLE_AXIS_HANDLE_OFFSET_PX)).top(px(8.0))
        })
        .hover(move |style| {
            style
                .bg(rgb(theme.action_hover_background))
                .text_color(rgb(theme.action_accent))
        })
        .on_mouse_down(gpui::MouseButton::Left, move |event, window, cx| {
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
        .child(render_table_axis_handle_icon(color))
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
        .children((0..2).map(move |_| {
            div().flex().gap(px(1.5)).children(
                (0..2).map(move |_| div().w(px(2.0)).h(px(2.0)).rounded(px(2.0)).bg(rgb(color))),
            )
        }))
        .into_any_element()
}

#[cfg(test)]
pub(super) fn is_header_cell(table: &TablePayload, row_index: usize, cell_index: usize) -> bool {
    is_header_row(table, row_index) || cell_index < table.header_cols
}

#[cfg(test)]
pub(super) fn is_header_row(table: &TablePayload, row_index: usize) -> bool {
    row_index < table.header_rows.max(usize::from(table.header_rows == 0))
}

pub(super) fn is_active_cell(
    focused_cell: Option<TableCellPosition>,
    row: usize,
    col: usize,
) -> bool {
    focused_cell == Some(TableCellPosition { row, col })
}
