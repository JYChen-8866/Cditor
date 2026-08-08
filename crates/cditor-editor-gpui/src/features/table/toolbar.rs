use crate::editor_view::CditorV2View;
use crate::input::SingleLineTextInputElement;
use crate::input::platform_adapter::{mobile_manual_focus, on_text_activation};
use crate::menu_metrics::{MenuViewportBounds, SECONDARY_MENU_WIDTH_PX, secondary_menu_geometry};
use crate::theme::GuiTheme;
use cditor_runtime::TableViewState;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, div, px, rgb,
};

use super::menu::{
    TABLE_MENU_PADDING_PX, TABLE_MENU_ROW_HEIGHT_PX, TABLE_MENU_SEARCH_FONT_SIZE_PX,
    TABLE_MENU_SEARCH_GAP_PX, TABLE_MENU_SEARCH_HEIGHT_PX, TABLE_MENU_VIEWPORT_MARGIN_PX,
    TABLE_MENU_WIDTH_PX, TableBackgroundColor, TableMenuAction, TableMenuUiState,
    TableRowInsertDirection, filter_table_menu_items, table_axis_header_enabled,
    table_axis_menu_items, table_menu_action_enabled, table_menu_panel_height, table_menu_position,
};
use super::selection::{TableAxis, TableAxisSelection};
use super::style::{
    TABLE_AXIS_COLUMN_HANDLE_TOP_PX, TABLE_AXIS_HANDLE_SIZE_PX, TABLE_AXIS_ROW_HANDLE_LEFT_PX,
    TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX,
};
#[path = "toolbar/origin.rs"]
mod origin;
pub(crate) use origin::{
    TableToolbarEditorOrigin, table_content_editor_origin, table_projected_viewport_width_px,
    table_toolbar_editor_origin,
};
const TABLE_COLOR_SUBMENU_GAP_PX: f32 = 6.0;
const TABLE_COLOR_SUBMENU_PADDING_PX: f32 = 6.0;
#[derive(Debug, Clone, Copy, PartialEq)]
struct TableMenuAnchor {
    left: f32,
    top: f32,
    height: f32,
}
#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
pub(crate) fn render_table_axis_toolbar(
    selection: TableAxisSelection,
    table_view: &TableViewState,
    origin: TableToolbarEditorOrigin,
    menu_ui: &TableMenuUiState,
    readonly: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    viewport: MenuViewportBounds,
) -> AnyElement {
    let items = filter_table_menu_items(&table_axis_menu_items(selection), menu_ui.query.as_str());
    let anchor = table_menu_anchor(selection, table_view);
    let menu_position = table_menu_position(
        anchor.left,
        anchor.top,
        anchor.height,
        items.len(),
        table_view.width_px + TABLE_MENU_WIDTH_PX + 16.0,
        table_view.height_px + table_menu_panel_height(items.len()) + 48.0,
    );
    let primary_left = (origin.x_px + menu_position.x).clamp(
        viewport.left + TABLE_MENU_VIEWPORT_MARGIN_PX,
        (viewport.right - TABLE_MENU_VIEWPORT_MARGIN_PX - TABLE_MENU_WIDTH_PX)
            .max(viewport.left + TABLE_MENU_VIEWPORT_MARGIN_PX),
    );
    let primary_top = (origin.y_px + menu_position.y).clamp(
        viewport.top + TABLE_MENU_VIEWPORT_MARGIN_PX,
        (viewport.bottom - TABLE_MENU_VIEWPORT_MARGIN_PX - menu_position.height)
            .max(viewport.top + TABLE_MENU_VIEWPORT_MARGIN_PX),
    );
    let empty = items.is_empty();
    let mut primary_panel = div()
        .id(("table-axis-menu-primary", selection.block_id))
        .absolute()
        .w(px(TABLE_MENU_WIDTH_PX))
        .h(px(menu_position.height))
        .p(px(TABLE_MENU_PADDING_PX))
        .flex()
        .flex_col()
        .rounded(px(9.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.panel))
        .shadow_lg()
        .occlude()
        .child(render_table_menu_search(
            selection.block_id,
            menu_ui,
            theme,
            view.clone(),
            focus,
        ))
        .child(div().h(px(TABLE_MENU_SEARCH_GAP_PX)).flex_none());

    if empty {
        primary_panel = primary_panel.child(
            div()
                .h(px(TABLE_MENU_ROW_HEIGHT_PX))
                .px(px(8.0))
                .flex()
                .items_center()
                .text_size(px(13.0))
                .text_color(rgb(theme.muted))
                .child("没有匹配的操作"),
        );
    } else {
        primary_panel = primary_panel.children(items.into_iter().map(|item| {
            render_table_menu_row(
                item.action,
                item.label,
                selection,
                table_view,
                menu_ui,
                readonly,
                theme,
                view.clone(),
            )
        }));
    }

    let submenu_height = table_background_submenu_height();
    let secondary = menu_ui.color_submenu_open.then(|| {
        secondary_menu_geometry(
            primary_left,
            primary_top,
            TABLE_MENU_WIDTH_PX,
            menu_position.height,
            SECONDARY_MENU_WIDTH_PX,
            submenu_height,
            viewport,
            TABLE_COLOR_SUBMENU_GAP_PX,
            TABLE_MENU_VIEWPORT_MARGIN_PX,
        )
    });
    let container_left = secondary
        .map(|menu| primary_left.min(menu.left))
        .unwrap_or(primary_left);
    let container_top = secondary
        .map(|menu| primary_top.min(menu.top))
        .unwrap_or(primary_top);
    let container_right = secondary
        .map(|menu| (primary_left + TABLE_MENU_WIDTH_PX).max(menu.left + SECONDARY_MENU_WIDTH_PX))
        .unwrap_or(primary_left + TABLE_MENU_WIDTH_PX);
    let container_bottom = secondary
        .map(|menu| (primary_top + menu_position.height).max(menu.top + submenu_height))
        .unwrap_or(primary_top + menu_position.height);
    let mut container = div()
        .id(("table-axis-menu", selection.block_id))
        .absolute()
        .left(px(container_left))
        .top(px(container_top))
        .w(px(container_right - container_left))
        .h(px(container_bottom - container_top))
        .on_mouse_down_out({
            let view = view.clone();
            move |_event, _window, cx| {
                view.update(cx, |view, cx| {
                    view.dismiss_table_menu_from_gui(cx);
                });
            }
        })
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            primary_panel
                .left(px(primary_left - container_left))
                .top(px(primary_top - container_top)),
        );

    if let Some(secondary) = secondary {
        container = container.child(render_table_background_submenu(
            theme,
            view,
            secondary.left - container_left,
            secondary.top - container_top,
        ));
    }
    container.into_any_element()
}

fn render_table_menu_search(
    block_id: cditor_core::ids::BlockId,
    menu_ui: &TableMenuUiState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
) -> AnyElement {
    let activate_view = view.clone();
    let input = mobile_manual_focus(
        div()
            .id(("table-menu-search", block_id))
            .h(px(TABLE_MENU_SEARCH_HEIGHT_PX))
            .w_full()
            .px(px(8.0))
            .flex_none()
            .flex()
            .items_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(rgb(theme.table_active_border))
            .bg(rgb(theme.surface))
            .track_focus(&focus)
            .child(SingleLineTextInputElement {
                handler: view,
                focus,
                value: menu_ui.query.clone(),
                placeholder: Some("搜索操作...".to_owned()),
                caret_offset: Some(menu_ui.caret_offset),
                marked_range: menu_ui.marked_range.clone(),
                text_color: theme.text,
                placeholder_color: theme.muted,
                caret_color: theme.focused,
                font_size: px(TABLE_MENU_SEARCH_FONT_SIZE_PX),
            }),
    );
    on_text_activation(input, move |_event, _window, cx| {
        activate_view.update(cx, |view, cx| {
            view.request_table_menu_query_focus_from_gui(block_id, cx);
        });
        cx.stop_propagation();
    })
    .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_table_menu_row(
    action: TableMenuAction,
    label: &'static str,
    selection: TableAxisSelection,
    table_view: &TableViewState,
    menu_ui: &TableMenuUiState,
    readonly: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let enabled = !readonly && table_menu_action_enabled(action, selection, table_view);
    let active =
        action == TableMenuAction::ToggleHeader && table_axis_header_enabled(selection, table_view);
    let row_insert = matches!(
        action,
        TableMenuAction::InsertRowAbove | TableMenuAction::InsertRowBelow
    );
    let text_color = table_menu_action_color(action, theme);
    let action_view = view.clone();
    let insert_view = view.clone();
    div()
        .id(("table-menu-action", table_menu_action_index(action)))
        .h(px(TABLE_MENU_ROW_HEIGHT_PX))
        .w_full()
        .px(px(7.0))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(4.0))
        .text_size(px(13.0))
        .text_color(rgb(if enabled { text_color } else { theme.muted }))
        .when(!enabled, |this| this.opacity(0.45))
        .when(enabled && !row_insert, |this| {
            this.cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.hover_surface)))
        })
        .when(enabled && row_insert, |this| {
            this.hover(move |style| style.bg(rgb(theme.hover_surface)))
        })
        .on_mouse_move({
            let view = view.clone();
            move |_event, _window, cx| {
                if enabled {
                    view.update(cx, |view, cx| {
                        view.set_table_background_submenu_open_from_gui(
                            action == TableMenuAction::BackgroundColor,
                            cx,
                        );
                    });
                }
            }
        })
        .when(!row_insert, |row| {
            row.on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                if enabled {
                    action_view.update(cx, |view, cx| {
                        view.apply_selected_table_menu_action_from_gui(action, cx);
                    });
                }
                cx.stop_propagation();
            })
        })
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap(px(8.0))
                .when(row_insert && enabled, |area| {
                    area.cursor_pointer().on_mouse_down(
                        MouseButton::Left,
                        move |_event, _window, cx| {
                            insert_view.update(cx, |view, cx| {
                                view.apply_selected_table_menu_action_from_gui(action, cx);
                            });
                            cx.stop_propagation();
                        },
                    )
                })
                .child(
                    div()
                        .w(px(20.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(action.icon()),
                )
                .child(label),
        )
        .when(action == TableMenuAction::ToggleHeader, |row| {
            row.child(render_header_toggle(active, enabled, theme))
        })
        .when(action == TableMenuAction::BackgroundColor, |row| {
            row.child(
                div()
                    .text_size(px(16.0))
                    .text_color(rgb(theme.muted))
                    .child("›"),
            )
        })
        .when(
            matches!(
                action,
                TableMenuAction::InsertRowAbove | TableMenuAction::InsertRowBelow
            ),
            |row| {
                row.child(render_row_insert_count_control(
                    action,
                    row_insert_direction(action),
                    menu_ui.row_insert_count(row_insert_direction(action)),
                    enabled,
                    theme,
                    view.clone(),
                ))
            },
        )
        .when_some(action.shortcut(), |row, shortcut| {
            row.child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.muted))
                    .child(shortcut),
            )
        })
        .into_any_element()
}

fn render_row_insert_count_control(
    action: TableMenuAction,
    direction: TableRowInsertDirection,
    count: usize,
    enabled: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let minus_view = view.clone();
    let plus_view = view;
    div()
        .w(px(72.0))
        .h(px(23.0))
        .flex_none()
        .flex()
        .items_center()
        .rounded(px(5.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        .when(!enabled, |control| control.opacity(0.55))
        .child(
            div()
                .id((
                    "table-row-insert-count-decrement",
                    table_menu_action_index(action),
                ))
                .w(px(22.0))
                .h_full()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_l(px(4.0))
                .text_size(px(13.0))
                .text_color(rgb(theme.muted))
                .when(enabled && count > 1, |button| {
                    button
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgb(theme.hover_surface)))
                })
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    if enabled {
                        minus_view.update(cx, |view, cx| {
                            view.adjust_table_row_insert_count_from_gui(direction, -1, cx);
                        });
                    }
                    cx.stop_propagation();
                })
                .child("−"),
        )
        .child(
            div()
                .h_full()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .border_l_1()
                .border_r_1()
                .border_color(rgb(theme.border))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(count.to_string()),
        )
        .child(
            div()
                .id((
                    "table-row-insert-count-increment",
                    table_menu_action_index(action),
                ))
                .w(px(22.0))
                .h_full()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_r(px(4.0))
                .text_size(px(13.0))
                .text_color(rgb(theme.muted))
                .when(
                    enabled && count < super::menu::TABLE_MENU_MAX_ROW_INSERT_COUNT,
                    |button| {
                        button
                            .cursor_pointer()
                            .hover(move |style| style.bg(rgb(theme.hover_surface)))
                    },
                )
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    if enabled {
                        plus_view.update(cx, |view, cx| {
                            view.adjust_table_row_insert_count_from_gui(direction, 1, cx);
                        });
                    }
                    cx.stop_propagation();
                })
                .child("+"),
        )
        .into_any_element()
}

const fn row_insert_direction(action: TableMenuAction) -> TableRowInsertDirection {
    match action {
        TableMenuAction::InsertRowAbove => TableRowInsertDirection::Above,
        TableMenuAction::InsertRowBelow => TableRowInsertDirection::Below,
        _ => panic!("row insert direction requested for a non-row-insert action"),
    }
}

fn render_header_toggle(active: bool, enabled: bool, theme: GuiTheme) -> AnyElement {
    div()
        .relative()
        .w(px(29.0))
        .h(px(17.0))
        .flex_none()
        .rounded_full()
        .bg(rgb(if active {
            theme.table_active_border
        } else {
            theme.border
        }))
        .when(!enabled, |toggle| toggle.opacity(0.65))
        .child(
            div()
                .absolute()
                .top(px(2.0))
                .left(px(if active { 14.0 } else { 2.0 }))
                .size(px(13.0))
                .rounded_full()
                .bg(rgb(theme.surface)),
        )
        .into_any_element()
}

pub(super) fn render_table_background_submenu(
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    left: f32,
    top: f32,
) -> AnyElement {
    div()
        .id("table-background-submenu")
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(SECONDARY_MENU_WIDTH_PX))
        .p(px(TABLE_COLOR_SUBMENU_PADDING_PX))
        .flex()
        .flex_col()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.panel))
        .shadow_lg()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .children(TableBackgroundColor::ALL.into_iter().map(|color| {
            let row_view = view.clone();
            div()
                .id(("table-background-color", background_color_index(color)))
                .h(px(TABLE_MENU_ROW_HEIGHT_PX))
                .w_full()
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(9.0))
                .rounded(px(4.0))
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.hover_surface)))
                .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                    row_view.update(cx, |view, cx| {
                        view.set_selected_table_background_from_gui(color, cx);
                    });
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .size(px(22.0))
                        .flex_none()
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(theme.border))
                        .bg(rgb(color.swatch(theme.panel))),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.0))
                        .text_color(rgb(theme.text))
                        .child(color.label()),
                )
                .into_any_element()
        }))
        .into_any_element()
}

const fn table_background_submenu_height() -> f32 {
    TABLE_COLOR_SUBMENU_PADDING_PX * 2.0
        + TABLE_MENU_ROW_HEIGHT_PX * TableBackgroundColor::ALL.len() as f32
}

fn table_menu_action_color(action: TableMenuAction, theme: GuiTheme) -> u32 {
    match action {
        TableMenuAction::DeleteRow | TableMenuAction::DeleteColumn => theme.danger,
        _ => theme.text,
    }
}

const fn table_menu_action_index(action: TableMenuAction) -> usize {
    match action {
        TableMenuAction::ToggleHeader => 0,
        TableMenuAction::BackgroundColor => 1,
        TableMenuAction::InsertRowAbove => 2,
        TableMenuAction::InsertRowBelow => 3,
        TableMenuAction::DeleteRow => 4,
        TableMenuAction::DuplicateRow => 5,
        TableMenuAction::InsertColumnLeft => 6,
        TableMenuAction::InsertColumnRight => 7,
        TableMenuAction::DeleteColumn => 8,
        TableMenuAction::DuplicateColumn => 9,
        TableMenuAction::ClearContents => 10,
    }
}

const fn background_color_index(color: TableBackgroundColor) -> usize {
    match color {
        TableBackgroundColor::Default => 0,
        TableBackgroundColor::Gray => 1,
        TableBackgroundColor::Brown => 2,
        TableBackgroundColor::Orange => 3,
        TableBackgroundColor::Yellow => 4,
        TableBackgroundColor::Green => 5,
        TableBackgroundColor::Blue => 6,
        TableBackgroundColor::Purple => 7,
        TableBackgroundColor::Pink => 8,
        TableBackgroundColor::Red => 9,
    }
}

fn table_menu_anchor(
    selection: TableAxisSelection,
    table_view: &TableViewState,
) -> TableMenuAnchor {
    match selection.axis {
        TableAxis::Row => {
            let (y, row_height) = table_view
                .visible_cells
                .iter()
                .find(|cell| cell.position.row == selection.index)
                .map(|cell| (cell.y_px, cell.height_px))
                .unwrap_or((0.0, TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX));
            TableMenuAnchor {
                left: TABLE_AXIS_ROW_HANDLE_LEFT_PX,
                top: y + row_height / 2.0 - TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX / 2.0,
                height: TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX,
            }
        }
        TableAxis::Column => {
            let (x, column_width) = table_view
                .visible_cells
                .iter()
                .find(|cell| cell.position.col == selection.index)
                .map(|cell| (cell.x_px, cell.width_px))
                .unwrap_or((0.0, TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX));
            TableMenuAnchor {
                left: x + table_view.horizontal_scroll_offset_px + column_width / 2.0
                    - TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX / 2.0,
                top: TABLE_AXIS_COLUMN_HANDLE_TOP_PX,
                height: TABLE_AXIS_HANDLE_SIZE_PX,
            }
        }
    }
}

#[cfg(test)]
#[path = "toolbar/tests.rs"]
mod tests;
