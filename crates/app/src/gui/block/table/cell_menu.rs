use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div,
    px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::menu_metrics::{
    MenuViewportBounds, SECONDARY_MENU_WIDTH_PX, secondary_menu_geometry,
};
use cditor_runtime::TableViewState;

use super::menu::{
    TABLE_MENU_ROW_HEIGHT_PX, TABLE_MENU_WIDTH_PX, TableMenuAction, TableMenuUiState,
};
use super::selection::TableCellSelection;
use super::toolbar::{TableToolbarEditorOrigin, render_table_background_submenu};

const TABLE_CELL_MENU_PADDING_PX: f32 = 6.0;
const TABLE_CELL_MENU_GAP_PX: f32 = 6.0;
const TABLE_CELL_MENU_VIEWPORT_MARGIN_PX: f32 = 8.0;
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
    viewport: MenuViewportBounds,
    menu_ui: &TableMenuUiState,
    readonly: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> Option<AnyElement> {
    let panel_height = TABLE_CELL_MENU_PADDING_PX * 2.0 + TABLE_MENU_ROW_HEIGHT_PX * 2.0;
    let color_open = menu_ui.color_submenu_open;
    let anchor = table_cell_menu_anchor(
        selection,
        table_view,
        origin.x_px,
        origin.y_px,
        viewport,
        TABLE_MENU_WIDTH_PX,
    )?;
    let primary_left = origin.x_px + anchor.left;
    let primary_top = origin.y_px + anchor.top;
    let secondary = color_open.then(|| {
        secondary_menu_geometry(
            primary_left,
            primary_top,
            TABLE_MENU_WIDTH_PX,
            panel_height,
            SECONDARY_MENU_WIDTH_PX,
            TABLE_CELL_MENU_COLOR_HEIGHT_PX,
            viewport,
            TABLE_CELL_MENU_GAP_PX,
            TABLE_CELL_MENU_VIEWPORT_MARGIN_PX,
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
        .map(|menu| (primary_top + panel_height).max(menu.top + TABLE_CELL_MENU_COLOR_HEIGHT_PX))
        .unwrap_or(primary_top + panel_height);
    let container_width = container_right - container_left;
    let container_height = container_bottom - container_top;

    let mut container = div()
        .id(("table-cell-menu", selection.block_id))
        .absolute()
        .left(px(container_left))
        .top(px(container_top))
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
                .absolute()
                .left(px(primary_left - container_left))
                .top(px(primary_top - container_top))
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

    if let Some(secondary) = secondary {
        container = container.child(render_table_background_submenu(
            theme,
            view,
            secondary.left - container_left,
            secondary.top - container_top,
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
    origin_x_px: f32,
    origin_y_px: f32,
    viewport: MenuViewportBounds,
    container_width_px: f32,
) -> Option<TableCellMenuAnchor> {
    let cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position.row == selection.row && cell.position.col == selection.col)?;
    let panel_height = TABLE_CELL_MENU_PADDING_PX * 2.0 + TABLE_MENU_ROW_HEIGHT_PX * 2.0;
    let cell_left = cell.x_px + table_view.horizontal_scroll_offset_px;
    let cell_right = cell_left + cell.width_px;
    let right_candidate = origin_x_px + cell_right + TABLE_CELL_MENU_GAP_PX;
    let left_candidate = origin_x_px + cell_right - TABLE_CELL_MENU_GAP_PX - container_width_px;
    let viewport_min_left = viewport.left + TABLE_CELL_MENU_VIEWPORT_MARGIN_PX;
    let viewport_max_left =
        (viewport.right - container_width_px - TABLE_CELL_MENU_VIEWPORT_MARGIN_PX)
            .max(viewport_min_left);
    let global_left = if right_candidate + container_width_px
        <= viewport.right - TABLE_CELL_MENU_VIEWPORT_MARGIN_PX
    {
        right_candidate
    } else if left_candidate >= viewport_min_left {
        left_candidate
    } else {
        right_candidate.clamp(viewport_min_left, viewport_max_left)
    };
    let centered_top = origin_y_px + cell.y_px + cell.height_px / 2.0 - panel_height / 2.0;
    let viewport_min_top = viewport.top + TABLE_CELL_MENU_VIEWPORT_MARGIN_PX;
    let viewport_max_top =
        (viewport.bottom - panel_height - TABLE_CELL_MENU_VIEWPORT_MARGIN_PX).max(viewport_min_top);
    let global_top = centered_top.clamp(viewport_min_top, viewport_max_top);
    Some(TableCellMenuAnchor {
        left: global_left - origin_x_px,
        top: global_top - origin_y_px,
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

        let anchor = table_cell_menu_anchor(
            TableCellSelection::new(7, 1, 2),
            &table_view,
            100.0,
            0.0,
            test_viewport(800.0, 800.0),
            TABLE_MENU_WIDTH_PX,
        )
        .expect("cell menu anchor");
        assert_eq!(anchor.left, 326.0);
        assert_eq!(anchor.top, 19.0);
        assert_eq!(
            table_cell_menu_anchor(
                TableCellSelection::new(7, 8, 9),
                &table_view,
                100.0,
                0.0,
                test_viewport(800.0, 800.0),
                TABLE_MENU_WIDTH_PX,
            ),
            None
        );
    }

    #[test]
    fn cell_menu_flips_left_when_the_right_side_would_leave_the_viewport() {
        let table_view = table_view_with_visible_cell(75.0, 685.0, 0.0);

        let anchor = table_cell_menu_anchor(
            TableCellSelection::new(7, 0, 0),
            &table_view,
            260.0,
            0.0,
            test_viewport(1_200.0, 800.0),
            TABLE_MENU_WIDTH_PX,
        )
        .expect("cell menu anchor");
        let global_left = 260.0 + anchor.left;

        assert_eq!(
            global_left,
            260.0 + 75.0 + 685.0 - TABLE_CELL_MENU_GAP_PX - TABLE_MENU_WIDTH_PX
        );
        assert_eq!(
            global_left + TABLE_MENU_WIDTH_PX + TABLE_CELL_MENU_GAP_PX,
            260.0 + 75.0 + 685.0
        );
        assert!(global_left + TABLE_MENU_WIDTH_PX <= 1200.0);
    }

    #[test]
    fn color_submenu_really_moves_to_the_primary_menu_left() {
        let table_view = table_view_with_visible_cell(300.0, 200.0, 0.0);
        let primary = table_cell_menu_anchor(
            TableCellSelection::new(7, 0, 0),
            &table_view,
            100.0,
            0.0,
            test_viewport(1_000.0, 800.0),
            TABLE_MENU_WIDTH_PX,
        )
        .expect("cell menu anchor");
        let primary_left = 100.0 + primary.left;
        let secondary = secondary_menu_geometry(
            primary_left,
            primary.top,
            TABLE_MENU_WIDTH_PX,
            TABLE_CELL_MENU_PADDING_PX * 2.0 + TABLE_MENU_ROW_HEIGHT_PX * 2.0,
            SECONDARY_MENU_WIDTH_PX,
            TABLE_CELL_MENU_COLOR_HEIGHT_PX,
            test_viewport(1_000.0, 800.0),
            TABLE_CELL_MENU_GAP_PX,
            TABLE_CELL_MENU_VIEWPORT_MARGIN_PX,
        );

        assert_eq!(
            secondary.placement,
            crate::gui::menu_metrics::SecondaryMenuPlacement::Left
        );
        assert_eq!(
            secondary.left + SECONDARY_MENU_WIDTH_PX + TABLE_CELL_MENU_GAP_PX,
            primary_left
        );
    }

    #[test]
    fn primary_menu_clamps_to_a_shifted_document_overlay_viewport() {
        let table_view = table_view_with_visible_cell(680.0, 170.0, 0.0);
        let viewport = MenuViewportBounds {
            left: -170.0,
            top: -32.0,
            right: 1_030.0,
            bottom: 768.0,
        };

        let anchor = table_cell_menu_anchor(
            TableCellSelection::new(7, 0, 0),
            &table_view,
            0.0,
            0.0,
            viewport,
            TABLE_MENU_WIDTH_PX,
        )
        .expect("cell menu anchor");

        assert!(anchor.left >= viewport.left + TABLE_CELL_MENU_VIEWPORT_MARGIN_PX);
        assert!(
            anchor.left + TABLE_MENU_WIDTH_PX
                <= viewport.right - TABLE_CELL_MENU_VIEWPORT_MARGIN_PX
        );
    }

    #[test]
    fn primary_menu_clamps_vertically_to_shifted_editor_viewport() {
        let table_view = table_view_with_visible_cell(100.0, 120.0, 0.0);
        let viewport = MenuViewportBounds {
            left: -170.0,
            top: 208.0,
            right: 1_030.0,
            bottom: 708.0,
        };
        let origin_y = 680.0;

        let anchor = table_cell_menu_anchor(
            TableCellSelection::new(7, 0, 0),
            &table_view,
            0.0,
            origin_y,
            viewport,
            TABLE_MENU_WIDTH_PX,
        )
        .expect("cell menu anchor");
        let global_top = origin_y + anchor.top;
        let panel_height = TABLE_CELL_MENU_PADDING_PX * 2.0 + TABLE_MENU_ROW_HEIGHT_PX * 2.0;

        assert!(global_top >= viewport.top + TABLE_CELL_MENU_VIEWPORT_MARGIN_PX);
        assert!(global_top + panel_height <= viewport.bottom - TABLE_CELL_MENU_VIEWPORT_MARGIN_PX);
    }

    fn test_viewport(width: f32, height: f32) -> MenuViewportBounds {
        MenuViewportBounds {
            left: 0.0,
            top: 0.0,
            right: width,
            bottom: height,
        }
    }

    fn table_view_with_visible_cell(
        cell_x_px: f32,
        cell_width_px: f32,
        horizontal_scroll_offset_px: f32,
    ) -> TableViewState {
        TableViewState {
            table: TablePayload::default(),
            visible_cells: vec![TableVisibleCell {
                position: TableCellPosition { row: 0, col: 0 },
                row_span: 1,
                col_span: 1,
                x_px: cell_x_px,
                y_px: 36.0,
                width_px: cell_width_px,
                height_px: 36.0,
                header: false,
                spans: Vec::new(),
                background_color: None,
                align: TableCellAlign::Left,
            }],
            width_px: cell_x_px + cell_width_px,
            height_px: 72.0,
            column_widths_px: vec![cell_width_px],
            row_heights_px: vec![36.0],
            horizontal_scroll_offset_px,
            focused_cell: None,
            focused_cell_offset: None,
            focused_cell_selection_range: None,
            row_count: 1,
            col_count: 1,
        }
    }
}
