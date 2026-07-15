use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div,
    px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use cditor_runtime::TableViewState;

use super::menu::{
    TABLE_MENU_ROW_HEIGHT_PX, TABLE_MENU_WIDTH_PX, TableMenuAction, TableMenuUiState,
};
use super::selection::TableCellSelection;
use super::toolbar::{TableToolbarEditorOrigin, render_table_background_submenu};

const TABLE_CELL_MENU_PADDING_PX: f32 = 6.0;
const TABLE_CELL_MENU_GAP_PX: f32 = 6.0;
const TABLE_CELL_MENU_COLOR_WIDTH_PX: f32 = 184.0;
const TABLE_CELL_MENU_COLOR_HEIGHT_PX: f32 = 302.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct TableCellMenuAnchor {
    left: f32,
    top: f32,
}

pub(crate) fn render_table_cell_menu(
    selection: TableCellSelection,
    table_view: &TableViewState,
    origin: TableToolbarEditorOrigin,
    menu_ui: &TableMenuUiState,
    readonly: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> Option<AnyElement> {
    let anchor = table_cell_menu_anchor(selection, table_view)?;
    let panel_height = TABLE_CELL_MENU_PADDING_PX * 2.0 + TABLE_MENU_ROW_HEIGHT_PX * 2.0;
    let color_open = menu_ui.color_submenu_open;
    let container_width = if color_open {
        TABLE_MENU_WIDTH_PX + TABLE_CELL_MENU_GAP_PX + TABLE_CELL_MENU_COLOR_WIDTH_PX
    } else {
        TABLE_MENU_WIDTH_PX
    };
    let container_height = if color_open {
        panel_height.max(TABLE_CELL_MENU_COLOR_HEIGHT_PX)
    } else {
        panel_height
    };

    let mut container = div()
        .id(("table-cell-menu", selection.block_id))
        .absolute()
        .left(px(origin.x_px + anchor.left))
        .top(px(origin.y_px + anchor.top))
        .w(px(container_width))
        .h(px(container_height))
        .on_mouse_down_out({
            let view = view.clone();
            move |_event, _window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.dismiss_table_menu_from_gui(cx);
                });
            }
        })
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .relative()
                .w(px(TABLE_MENU_WIDTH_PX))
                .h(px(panel_height))
                .p(px(TABLE_CELL_MENU_PADDING_PX))
                .flex()
                .flex_col()
                .rounded(px(8.0))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.panel))
                .shadow_lg()
                .occlude()
                .child(render_cell_menu_row(
                    TableMenuAction::BackgroundColor,
                    "颜色",
                    readonly,
                    theme,
                    view.clone(),
                ))
                .child(render_cell_menu_row(
                    TableMenuAction::ClearContents,
                    "清除内容",
                    readonly,
                    theme,
                    view.clone(),
                )),
        );

    if color_open {
        container = container.child(render_table_background_submenu(
            theme,
            view,
            TABLE_MENU_WIDTH_PX + TABLE_CELL_MENU_GAP_PX,
            0.0,
        ));
    }
    Some(container.into_any_element())
}

fn render_cell_menu_row(
    action: TableMenuAction,
    label: &'static str,
    readonly: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    div()
        .id(("table-cell-menu-action", cell_menu_action_index(action)))
        .h(px(TABLE_MENU_ROW_HEIGHT_PX))
        .w_full()
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(9.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .when(readonly, |row| row.opacity(0.5))
        .when(!readonly, |row| {
            row.hover(move |style| style.bg(rgb(theme.hover_surface)))
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    let _ = view.update(cx, |view, cx| match action {
                        TableMenuAction::BackgroundColor => {
                            view.set_table_background_submenu_open_from_gui(true, cx)
                        }
                        TableMenuAction::ClearContents => {
                            view.apply_selected_table_menu_action_from_gui(action, cx)
                        }
                        _ => false,
                    });
                    cx.stop_propagation();
                })
        })
        .child(
            div()
                .w(px(18.0))
                .flex_none()
                .text_size(px(15.0))
                .text_color(rgb(theme.text))
                .child(action.icon()),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(13.0))
                .text_color(rgb(theme.text))
                .child(label),
        )
        .when(action == TableMenuAction::BackgroundColor, |row| {
            row.child(
                div()
                    .text_size(px(16.0))
                    .text_color(rgb(theme.muted))
                    .child("›"),
            )
        })
        .into_any_element()
}

fn table_cell_menu_anchor(
    selection: TableCellSelection,
    table_view: &TableViewState,
) -> Option<TableCellMenuAnchor> {
    let cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position.row == selection.row && cell.position.col == selection.col)?;
    let panel_height = TABLE_CELL_MENU_PADDING_PX * 2.0 + TABLE_MENU_ROW_HEIGHT_PX * 2.0;
    Some(TableCellMenuAnchor {
        left: cell.x_px
            + table_view.horizontal_scroll_offset_px
            + cell.width_px
            + TABLE_CELL_MENU_GAP_PX,
        top: (cell.y_px + cell.height_px / 2.0 - panel_height / 2.0).max(0.0),
    })
}

const fn cell_menu_action_index(action: TableMenuAction) -> usize {
    match action {
        TableMenuAction::BackgroundColor => 0,
        TableMenuAction::ClearContents => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{TableCellAlign, TablePayload};
    use cditor_runtime::{TableCellPosition, TableVisibleCell};

    use super::*;

    #[test]
    fn cell_menu_is_anchored_to_the_scrolled_cell_right_edge() {
        let table_view = TableViewState {
            table: TablePayload::default(),
            visible_cells: vec![TableVisibleCell {
                position: TableCellPosition { row: 1, col: 2 },
                row_span: 1,
                col_span: 1,
                x_px: 240.0,
                y_px: 36.0,
                width_px: 120.0,
                height_px: 36.0,
                header: false,
                spans: Vec::new(),
                background_color: None,
                align: TableCellAlign::Left,
            }],
            width_px: 360.0,
            height_px: 72.0,
            column_widths_px: vec![120.0; 3],
            row_heights_px: vec![36.0; 2],
            horizontal_scroll_offset_px: -40.0,
            focused_cell: None,
            focused_cell_offset: None,
            focused_cell_selection_range: None,
            row_count: 2,
            col_count: 3,
        };

        let anchor = table_cell_menu_anchor(TableCellSelection::new(7, 1, 2), &table_view)
            .expect("cell menu anchor");
        assert_eq!(anchor.left, 326.0);
        assert_eq!(anchor.top, 19.0);
        assert_eq!(
            table_cell_menu_anchor(TableCellSelection::new(7, 8, 9), &table_view),
            None
        );
    }
}
