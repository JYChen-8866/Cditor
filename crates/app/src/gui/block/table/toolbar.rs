use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
#[cfg(test)]
use crate::gui::block::chrome::{
    BLOCK_GUTTER_WIDTH_PX, BLOCK_INDENT_STEP_PX, BLOCK_PREFIX_WIDTH_PX,
};
use crate::gui::block::chrome::{
    BLOCK_ROW_GAP_PX, BLOCK_SHELL_OUTER_PADDING_X_PX, BlockChromeStyle,
};
use crate::gui::input::SingleLineTextInputElement;
#[cfg(test)]
use cditor_core::rich_text::TableCellAlign;
use cditor_runtime::{TableViewState, ViewBlockSnapshot};

use super::menu::{
    TABLE_MENU_PADDING_PX, TABLE_MENU_ROW_HEIGHT_PX, TABLE_MENU_SEARCH_FONT_SIZE_PX,
    TABLE_MENU_SEARCH_GAP_PX, TABLE_MENU_SEARCH_HEIGHT_PX, TABLE_MENU_WIDTH_PX,
    TableBackgroundColor, TableMenuAction, TableMenuUiState, filter_table_menu_items,
    table_axis_header_enabled, table_axis_menu_items, table_menu_action_enabled,
    table_menu_panel_height, table_menu_position,
};
use super::selection::{TableAxis, TableAxisSelection};
use super::style::{
    TABLE_AXIS_COLUMN_HANDLE_TOP_PX, TABLE_AXIS_HANDLE_SIZE_PX, TABLE_AXIS_ROW_HANDLE_LEFT_PX,
    TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX,
};

const BLOCK_SHELL_OUTER_PADDING_Y_PX: f32 = 4.0;
const BLOCK_CONTENT_BORDER_WIDTH_PX: f32 = 1.0;
const TABLE_COLOR_SUBMENU_WIDTH_PX: f32 = 184.0;
const TABLE_COLOR_SUBMENU_GAP_PX: f32 = 6.0;
const TABLE_COLOR_SUBMENU_PADDING_PX: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TableToolbarEditorOrigin {
    pub x_px: f32,
    pub y_px: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TableMenuAnchor {
    left: f32,
    top: f32,
    height: f32,
}

pub(crate) fn table_toolbar_editor_origin(
    block: &ViewBlockSnapshot,
    block_top_px: f32,
    theme: GuiTheme,
) -> TableToolbarEditorOrigin {
    table_content_editor_origin(block, block_top_px, theme)
}

pub(crate) fn table_content_editor_origin(
    block: &ViewBlockSnapshot,
    block_top_px: f32,
    theme: GuiTheme,
) -> TableToolbarEditorOrigin {
    let chrome = BlockChromeStyle::from_snapshot(block, theme);
    TableToolbarEditorOrigin {
        x_px: BLOCK_SHELL_OUTER_PADDING_X_PX
            + chrome.indent_px
            + chrome.gutter_width_px
            + BLOCK_ROW_GAP_PX
            + chrome.marker_lane_width_px
            + BLOCK_CONTENT_BORDER_WIDTH_PX
            + chrome.content_padding_left_px
            + chrome.content_prefix_width_px,
        y_px: block_top_px
            + BLOCK_SHELL_OUTER_PADDING_Y_PX
            + BLOCK_CONTENT_BORDER_WIDTH_PX
            + chrome.content_padding_y_px,
    }
}

pub(crate) fn render_table_axis_toolbar(
    selection: TableAxisSelection,
    table_view: &TableViewState,
    origin: TableToolbarEditorOrigin,
    menu_ui: &TableMenuUiState,
    readonly: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
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
    let empty = items.is_empty();
    let mut primary_panel = div()
        .id(("table-axis-menu-primary", selection.block_id))
        .relative()
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
                readonly,
                theme,
                view.clone(),
            )
        }));
    }

    let submenu_top = table_background_submenu_top();
    let submenu_height = table_background_submenu_height();
    let container_width = if menu_ui.color_submenu_open {
        table_background_submenu_left() + TABLE_COLOR_SUBMENU_WIDTH_PX
    } else {
        TABLE_MENU_WIDTH_PX
    };
    let container_height = if menu_ui.color_submenu_open {
        menu_position.height.max(submenu_top + submenu_height)
    } else {
        menu_position.height
    };
    let mut container = div()
        .id(("table-axis-menu", selection.block_id))
        .absolute()
        .left(px(origin.x_px + menu_position.x))
        .top(px(origin.y_px + menu_position.y))
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
        .child(primary_panel);

    if menu_ui.color_submenu_open {
        container = container.child(render_table_background_submenu(theme, view));
    }
    container.into_any_element()
}

fn render_table_menu_search(
    menu_ui: &TableMenuUiState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
) -> AnyElement {
    div()
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
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_table_menu_row(
    action: TableMenuAction,
    label: &'static str,
    selection: TableAxisSelection,
    table_view: &TableViewState,
    readonly: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let enabled = !readonly && table_menu_action_enabled(action, selection, table_view);
    let active =
        action == TableMenuAction::ToggleHeader && table_axis_header_enabled(selection, table_view);
    let text_color = table_menu_action_color(action, theme);
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
        .when(enabled, |this| {
            this.cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.hover_surface)))
        })
        .on_mouse_move({
            let view = view.clone();
            move |_event, _window, cx| {
                if enabled {
                    let _ = view.update(cx, |view, cx| {
                        view.set_table_background_submenu_open_from_gui(
                            action == TableMenuAction::BackgroundColor,
                            cx,
                        );
                    });
                }
            }
        })
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            if enabled {
                let _ = view.update(cx, |view, cx| {
                    view.apply_selected_table_menu_action_from_gui(action, cx);
                });
            }
            cx.stop_propagation();
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
        .child(div().flex_1().child(label))
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

fn render_table_background_submenu(theme: GuiTheme, view: Entity<CditorV2View>) -> AnyElement {
    div()
        .id("table-background-submenu")
        .absolute()
        .left(px(table_background_submenu_left()))
        .top(px(table_background_submenu_top()))
        .w(px(TABLE_COLOR_SUBMENU_WIDTH_PX))
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
                    let _ = row_view.update(cx, |view, cx| {
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

const fn table_background_submenu_left() -> f32 {
    TABLE_MENU_WIDTH_PX + TABLE_COLOR_SUBMENU_GAP_PX
}

const fn table_background_submenu_top() -> f32 {
    TABLE_MENU_PADDING_PX
        + TABLE_MENU_SEARCH_HEIGHT_PX
        + TABLE_MENU_SEARCH_GAP_PX
        + TABLE_MENU_ROW_HEIGHT_PX
}

const fn table_background_submenu_height() -> f32 {
    TABLE_COLOR_SUBMENU_PADDING_PX * 2.0
        + TABLE_MENU_ROW_HEIGHT_PX * TableBackgroundColor::ALL.len() as f32
}

fn table_menu_action_color(action: TableMenuAction, theme: GuiTheme) -> u32 {
    if matches!(
        action,
        TableMenuAction::DeleteRow | TableMenuAction::DeleteColumn
    ) {
        theme.danger
    } else {
        theme.text
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
mod tests {
    use cditor_core::block::{BlockChromeSnapshot, BlockListInfo, BlockPrefixSnapshot};
    use cditor_core::layout::BlockLayoutMeta;
    use cditor_core::rich_text::{BlockAttrs, BlockPayloadView, RichBlockKind, TablePayload};

    use super::*;

    #[test]
    fn destructive_table_actions_use_danger_text() {
        let theme = GuiTheme::light();
        assert_eq!(
            table_menu_action_color(TableMenuAction::DeleteRow, theme),
            theme.danger
        );
        assert_eq!(
            table_menu_action_color(TableMenuAction::InsertRowAbove, theme),
            theme.text
        );
    }

    fn table_block_with_depth(depth: u8) -> ViewBlockSnapshot {
        ViewBlockSnapshot {
            block_id: 7,
            visible_index: 0,
            depth: depth as u16,
            chrome: BlockChromeSnapshot {
                list_info: BlockListInfo::with_depth(depth.into()),
                prefix: BlockPrefixSnapshot::None,
                has_children: false,
                collapsed: false,
            },
            kind: RichBlockKind::Table,
            attrs: BlockAttrs::default(),
            payload: BlockPayloadView::Placeholder {
                estimated_height: 96.0,
            },
            layout: BlockLayoutMeta::new(1, 96.0),
            selected: false,
            selection_range: None,
            selection_overlay: false,
            focused: false,
            caret_offset: None,
            marked_range: None,
            table_view: None,
            focused_table_cell: None,
            focused_table_cell_offset: None,
            pinned: false,
            placeholder: false,
        }
    }

    fn depth_two_table_editor_x_px() -> f32 {
        BLOCK_SHELL_OUTER_PADDING_X_PX
            + 2.0 * BLOCK_INDENT_STEP_PX
            + BLOCK_GUTTER_WIDTH_PX
            + BLOCK_ROW_GAP_PX
            + BLOCK_PREFIX_WIDTH_PX
            + BLOCK_CONTENT_BORDER_WIDTH_PX
    }

    #[test]
    fn menu_anchor_left_matches_selected_gutter_left_edge() {
        let table_view = table_view_with_two_by_two_cells();

        assert_eq!(
            table_menu_anchor(
                TableAxisSelection::new(7, TableAxis::Column, 1),
                &table_view
            ),
            TableMenuAnchor {
                left: 169.0,
                top: -15.0,
                height: 16.0,
            }
        );
        assert_eq!(
            table_menu_anchor(TableAxisSelection::new(7, TableAxis::Row, 1), &table_view),
            TableMenuAnchor {
                left: -28.0,
                top: 43.0,
                height: 22.0,
            }
        );

        let mut scrolled = table_view;
        scrolled.horizontal_scroll_offset_px = -80.0;
        assert_eq!(
            table_menu_anchor(TableAxisSelection::new(7, TableAxis::Column, 1), &scrolled).left,
            89.0
        );
    }

    #[test]
    fn table_color_submenu_has_a_visible_gap_and_stays_inside_menu_container() {
        assert_eq!(
            table_background_submenu_left() - TABLE_MENU_WIDTH_PX,
            TABLE_COLOR_SUBMENU_GAP_PX
        );
        assert_eq!(
            table_background_submenu_height(),
            TABLE_COLOR_SUBMENU_PADDING_PX * 2.0
                + TABLE_MENU_ROW_HEIGHT_PX * TableBackgroundColor::ALL.len() as f32
        );
    }

    #[test]
    fn table_toolbar_editor_origin_tracks_block_shell_projection() {
        let origin =
            table_toolbar_editor_origin(&table_block_with_depth(2), 120.0, GuiTheme::light());

        assert_eq!(
            origin,
            TableToolbarEditorOrigin {
                x_px: depth_two_table_editor_x_px(),
                y_px: 129.0,
            }
        );
    }

    #[test]
    fn table_content_editor_origin_matches_toolbar_origin() {
        let origin =
            table_content_editor_origin(&table_block_with_depth(2), 120.0, GuiTheme::light());

        assert_eq!(
            origin,
            TableToolbarEditorOrigin {
                x_px: depth_two_table_editor_x_px(),
                y_px: 129.0,
            }
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
            focused_cell_selection_range: None,
        }
    }
}
