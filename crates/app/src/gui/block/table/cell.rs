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

use super::selection::{TableAxisSelection, TableCellRangeSelection};
use super::style::{
    TABLE_ACTIVE_CELL_BORDER_WIDTH_PX, TABLE_ACTIVE_CELL_RADIUS_PX, V1_TABLE_CELL_PADDING_X_PX,
    V1_TABLE_CELL_PADDING_Y_PX, table_active_border_color, table_cell_background,
    table_cell_border_color, table_cell_hover_background, table_cell_line_height,
    table_selected_cell_background,
};

pub(super) fn render_table_cell(
    cell: &TableVisibleCell,
    content: AnyElement,
    theme: GuiTheme,
    focused_cell: Option<TableCellPosition>,
    table_selection: Option<TableAxisSelection>,
    table_range_selection: Option<TableCellRangeSelection>,
    view: Entity<CditorV2View>,
    block_id: BlockId,
) -> AnyElement {
    let row_index = cell.position.row;
    let cell_index = cell.position.col;
    let active = is_active_cell(focused_cell, row_index, cell_index);
    let multi_cell_range = table_range_selection.is_some_and(|s| s.is_multi_cell());
    let range_selected = table_range_selection
        .map(|selection| selection.selects_cell(block_id, row_index, cell_index))
        .unwrap_or(false);
    let axis_selected = table_selection
        .map(|selection| selection.selects_cell(block_id, row_index, cell_index))
        .unwrap_or(false);
    let selected = range_selected || axis_selected;
    let hover_background = table_cell_hover_background(theme, cell.header);
    let focus_view = view.clone();
    let range_hover_view = view.clone();
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
                .border_b_1()
                .border_color(rgb(table_cell_border_color(theme, range_selected)))
                .bg(rgb(if selected {
                    if range_selected {
                        table_selected_cell_background(theme)
                    } else {
                        table_cell_background(theme, cell.header, cell.background_color.as_deref())
                    }
                } else {
                    table_cell_background(theme, cell.header, cell.background_color.as_deref())
                }))
                .when(!active && !selected, |this| {
                    this.hover(move |style| style.bg(rgb(hover_background)))
                })
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
                .when(active && !multi_cell_range, |this| {
                    this.child(
                        div()
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .w_full()
                            .h_full()
                            .rounded(px(TABLE_ACTIVE_CELL_RADIUS_PX))
                            .border(px(TABLE_ACTIVE_CELL_BORDER_WIDTH_PX))
                            .border_color(rgb(table_active_border_color(theme))),
                    )
                }),
        )
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
