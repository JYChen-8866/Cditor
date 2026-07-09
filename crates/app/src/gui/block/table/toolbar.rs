use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div,
    px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
#[cfg(test)]
use cditor_core::rich_text::TableCellAlign;
use cditor_runtime::TableViewState;

use super::menu::{
    TABLE_MENU_ROW_HEIGHT_PX, TABLE_MENU_WIDTH_PX, TableMenuAction, filter_table_menu_items,
    table_axis_menu_items, table_menu_action_enabled, table_menu_panel_height, table_menu_position,
    table_range_menu_items,
};
use super::selection::{TableAxis, TableAxisSelection, TableCellRangeSelection};

pub(super) fn render_table_axis_toolbar(
    selection: TableAxisSelection,
    table_view: &TableViewState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let items = filter_table_menu_items(&table_axis_menu_items(selection), "");
    let (anchor_x, anchor_y) = toolbar_position(selection, table_view);
    let menu_position = table_menu_position(
        anchor_x,
        anchor_y,
        0.0,
        items.len(),
        table_view.width_px + TABLE_MENU_WIDTH_PX + 16.0,
        table_view.height_px + table_menu_panel_height(items.len()) + 48.0,
    );
    div()
        .absolute()
        .left(px(menu_position.x))
        .top(px(menu_position.y))
        .flex()
        .flex_col()
        .w(px(TABLE_MENU_WIDTH_PX))
        .h(px(menu_position.height))
        .py(px(4.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(theme.code_toolbar_border))
        .bg(rgb(theme.code_toolbar_background))
        .shadow_lg()
        .overflow_hidden()
        .children(
            items
                .into_iter()
                .map(|item| render_table_menu_row(item.action, item.label, theme, view.clone())),
        )
        .into_any_element()
}

pub(super) fn render_table_range_toolbar(
    selection: TableCellRangeSelection,
    table_view: &TableViewState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let items = filter_table_menu_items(&table_range_menu_items(selection), "");
    let (anchor_x, anchor_y) = range_toolbar_position(selection, table_view);
    let menu_position = table_menu_position(
        anchor_x,
        anchor_y,
        0.0,
        items.len(),
        table_view.width_px + TABLE_MENU_WIDTH_PX + 16.0,
        table_view.height_px + table_menu_panel_height(items.len()) + 48.0,
    );
    div()
        .absolute()
        .left(px(menu_position.x))
        .top(px(menu_position.y))
        .flex()
        .flex_col()
        .w(px(TABLE_MENU_WIDTH_PX))
        .h(px(menu_position.height))
        .py(px(4.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(theme.code_toolbar_border))
        .bg(rgb(theme.code_toolbar_background))
        .shadow_lg()
        .overflow_hidden()
        .children(
            items
                .into_iter()
                .map(|item| render_table_menu_row(item.action, item.label, theme, view.clone())),
        )
        .into_any_element()
}

fn render_table_menu_row(
    action: TableMenuAction,
    label: &'static str,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let enabled = table_menu_action_enabled(action);
    div()
        .h(px(TABLE_MENU_ROW_HEIGHT_PX))
        .px(px(10.0))
        .flex()
        .items_center()
        .text_size(px(13.0))
        .text_color(rgb(theme.code_toolbar_text))
        .when(!enabled, |this| this.opacity(0.45))
        .when(enabled, |this| {
            this.cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.code_toolbar_hover)))
        })
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            if enabled {
                let _ = view.update(cx, |view, cx| {
                    view.apply_selected_table_menu_action_from_gui(action, cx);
                });
            }
            cx.stop_propagation();
        })
        .child(label)
        .into_any_element()
}

fn toolbar_position(selection: TableAxisSelection, table_view: &TableViewState) -> (f32, f32) {
    const TOOLBAR_GAP_PX: f32 = 34.0;
    match selection.axis {
        TableAxis::Row => {
            let y = table_view
                .visible_cells
                .iter()
                .find(|cell| cell.position.row == selection.index)
                .map(|cell| cell.y_px)
                .unwrap_or(0.0);
            (0.0, (y - TOOLBAR_GAP_PX).max(-TOOLBAR_GAP_PX))
        }
        TableAxis::Column => {
            let x = table_view
                .visible_cells
                .iter()
                .find(|cell| cell.position.col == selection.index)
                .map(|cell| cell.x_px)
                .unwrap_or(0.0);
            (x, -TOOLBAR_GAP_PX)
        }
    }
}

fn range_toolbar_position(
    selection: TableCellRangeSelection,
    table_view: &TableViewState,
) -> (f32, f32) {
    const TOOLBAR_GAP_PX: f32 = 34.0;
    table_view
        .visible_cells
        .iter()
        .find(|cell| {
            cell.position.row == selection.range.start_row
                && cell.position.col == selection.range.start_col
        })
        .map(|cell| (cell.x_px, (cell.y_px - TOOLBAR_GAP_PX).max(-TOOLBAR_GAP_PX)))
        .unwrap_or((0.0, -TOOLBAR_GAP_PX))
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::TablePayload;

    use super::*;

    #[test]
    fn toolbar_position_anchors_to_selected_column_or_row() {
        let table_view = TableViewState {
            table: TablePayload::default(),
            row_count: 2,
            col_count: 2,
            width_px: 240.0,
            height_px: 72.0,
            visible_cells: vec![cditor_runtime::TableVisibleCell {
                position: cditor_runtime::TableCellPosition { row: 1, col: 1 },
                row_span: 1,
                col_span: 1,
                x_px: 120.0,
                y_px: 36.0,
                width_px: 120.0,
                height_px: 36.0,
                header: false,
                align: TableCellAlign::Left,
                background_color: None,
                spans: Vec::new(),
            }],
            focused_cell: None,
            focused_cell_offset: None,
        };

        assert_eq!(
            toolbar_position(
                TableAxisSelection::new(7, TableAxis::Column, 1),
                &table_view
            ),
            (120.0, -34.0)
        );
        assert_eq!(
            toolbar_position(TableAxisSelection::new(7, TableAxis::Row, 1), &table_view),
            (0.0, 2.0)
        );
    }
}
