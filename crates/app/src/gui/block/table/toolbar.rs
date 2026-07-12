use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div,
    px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::chrome::{
    BLOCK_ROW_GAP_PX, BLOCK_SHELL_OUTER_PADDING_X_PX, BlockChromeStyle,
};
#[cfg(test)]
use cditor_core::rich_text::TableCellAlign;
use cditor_runtime::TableViewState;
use cditor_runtime::ViewBlockSnapshot;

use super::menu::{
    TABLE_MENU_ROW_HEIGHT_PX, TABLE_MENU_WIDTH_PX, TableMenuAction, filter_table_menu_items,
    table_axis_menu_items, table_menu_action_enabled, table_menu_panel_height, table_menu_position,
};
use super::selection::{TableAxis, TableAxisSelection};

const BLOCK_SHELL_OUTER_PADDING_Y_PX: f32 = 4.0;
const BLOCK_CONTENT_BORDER_WIDTH_PX: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TableToolbarEditorOrigin {
    pub x_px: f32,
    pub y_px: f32,
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
            + BLOCK_CONTENT_BORDER_WIDTH_PX
            + chrome.content_padding_left_px
            + chrome.prefix_width_px,
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
        .left(px(origin.x_px + menu_position.x))
        .top(px(origin.y_px + menu_position.y))
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
        .on_mouse_down_out({
            let view = view.clone();
            move |_event, _window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.dismiss_table_menu_from_gui(cx);
                });
            }
        })
        .children(items.into_iter().map(|item| {
            render_table_menu_row(
                item.action,
                item.label,
                selection,
                table_view,
                theme,
                view.clone(),
            )
        }))
        .into_any_element()
}

fn render_table_menu_row(
    action: TableMenuAction,
    label: &'static str,
    selection: TableAxisSelection,
    table_view: &TableViewState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let enabled = table_menu_action_enabled(action, selection, table_view);
    let text_color = table_menu_action_color(action, theme);
    div()
        .h(px(TABLE_MENU_ROW_HEIGHT_PX))
        .px(px(10.0))
        .flex()
        .items_center()
        .text_size(px(13.0))
        .text_color(rgb(text_color))
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

    #[test]
    fn toolbar_position_anchors_to_selected_column_or_row() {
        let table_view = TableViewState {
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

    #[test]
    fn table_toolbar_editor_origin_tracks_block_shell_projection() {
        let origin =
            table_toolbar_editor_origin(&table_block_with_depth(2), 120.0, GuiTheme::light());

        assert_eq!(
            origin,
            TableToolbarEditorOrigin {
                x_px: 89.0,
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
                x_px: 89.0,
                y_px: 129.0,
            }
        );
    }
}
