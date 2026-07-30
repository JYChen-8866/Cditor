use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div,
    px, rgb,
};

use crate::editor_view::CditorV2View;
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_runtime::{TableCellPosition, TableViewState};

use super::axis_grip::{render_table_axis_handle_icon, table_axis_handle_dimensions};
use super::cell_gutter::render_active_cell_gutters;
use super::selection::{TableAxis, TableAxisSelection, TableCellRangeSelection};
use super::style::{
    TABLE_AXIS_COLUMN_HANDLE_TOP_PX, TABLE_AXIS_HANDLE_RADIUS_PX, TABLE_AXIS_HANDLE_SIZE_PX,
    TABLE_AXIS_OUTLINE_THICKNESS_PX, TABLE_AXIS_ROW_HANDLE_LEFT_PX, table_active_border_color,
    table_axis_handle_background, table_axis_handle_foreground, table_axis_handle_hover_background,
};

use super::toolbar::TableToolbarEditorOrigin;

#[path = "chrome/geometry.rs"]
mod geometry;
pub(super) use geometry::{TableOverlayRect, table_content_cell_rect};
use geometry::{
    table_axis_selection_outline_rect, table_axis_track_rect, table_cell_rect,
    table_range_selection_outline_rect,
};
#[cfg(test)]
use geometry::{table_axis_selection_rect, table_overlay_left_in_editor};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TableChromeOrigins {
    pub(crate) viewport: TableToolbarEditorOrigin,
    pub(crate) top_edge: TableToolbarEditorOrigin,
    pub(crate) scrolling_top_edge: TableToolbarEditorOrigin,
    pub(crate) left_edge: TableToolbarEditorOrigin,
}

#[derive(Default)]
pub(crate) struct TableChromeOverlays {
    pub(crate) viewport: Vec<AnyElement>,
    pub(crate) top_edge: Vec<AnyElement>,
    pub(crate) left_edge: Vec<AnyElement>,
}

#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
pub(crate) fn render_table_axis_overlays(
    block_id: BlockId,
    table_view: &TableViewState,
    selection: Option<TableAxisSelection>,
    range_selection: Option<TableCellRangeSelection>,
    focused_cell: Option<TableCellPosition>,
    row_track_sizes: &[f32],
    column_track_sizes: &[f32],
    origins: TableChromeOrigins,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> TableChromeOverlays {
    let mut overlays = TableChromeOverlays::default();
    let cell_owned_handles =
        active_cell_axis_handle_ownership(block_id, focused_cell, selection, range_selection);
    let (row_handles, column_handles) = render_table_axis_handles(
        block_id,
        table_view,
        selection,
        cell_owned_handles,
        row_track_sizes,
        column_track_sizes,
        origins.left_edge,
        origins.top_edge,
        theme,
        view.clone(),
    );
    overlays.left_edge.extend(row_handles);
    overlays.top_edge.extend(column_handles);
    if let Some(focused_cell) = cell_owned_handles
        && let Some(cell_rect) = table_cell_rect(table_view, focused_cell.row, focused_cell.col)
    {
        let gutters = render_active_cell_gutters(
            focused_cell,
            cell_rect,
            table_view.horizontal_scroll_offset_px,
            row_track_sizes,
            column_track_sizes,
            origins.scrolling_top_edge,
            origins.left_edge,
            theme,
            view.clone(),
            block_id,
        );
        overlays.top_edge.extend(gutters.top_edge);
        overlays.left_edge.extend(gutters.left_edge);
    }
    if let Some(selection) = selection.filter(|selection| selection.block_id == block_id)
        && let Some(rect) = table_axis_selection_outline_rect(table_view, selection)
    {
        overlays
            .viewport
            .push(render_table_axis_outline(rect, origins.viewport, theme));
    }
    if let Some(range_sel) = range_selection.filter(|s| s.block_id == block_id && s.is_multi_cell())
        && let Some(rect) = table_range_selection_outline_rect(table_view, range_sel)
    {
        overlays
            .viewport
            .push(render_table_axis_outline(rect, origins.viewport, theme));
    }
    overlays
}

#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
fn render_table_axis_handles(
    block_id: BlockId,
    table_view: &TableViewState,
    selection: Option<TableAxisSelection>,
    cell_owned_handles: Option<TableCellPosition>,
    row_track_sizes: &[f32],
    column_track_sizes: &[f32],
    row_origin: TableToolbarEditorOrigin,
    column_origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> (Vec<AnyElement>, Vec<AnyElement>) {
    let mut row_handles = Vec::new();
    let mut column_handles = Vec::new();
    for row in 0..table_view.row_count {
        if active_cell_owns_axis_handle(cell_owned_handles, TableAxis::Row, row) {
            continue;
        }
        if let Some(rect) = table_axis_track_rect(table_view, TableAxis::Row, row) {
            row_handles.push(render_table_axis_handle(
                block_id,
                TableAxis::Row,
                row,
                rect,
                selection.is_some_and(|selection| selection.selects_row_handle(block_id, row)),
                row_track_sizes.to_vec(),
                row_origin,
                theme,
                view.clone(),
            ));
        }
    }
    for col in 0..table_view.col_count {
        if active_cell_owns_axis_handle(cell_owned_handles, TableAxis::Column, col) {
            continue;
        }
        if let Some(rect) = table_axis_track_rect(table_view, TableAxis::Column, col) {
            column_handles.push(render_table_axis_handle(
                block_id,
                TableAxis::Column,
                col,
                rect,
                selection.is_some_and(|selection| selection.selects_column_handle(block_id, col)),
                column_track_sizes.to_vec(),
                column_origin,
                theme,
                view.clone(),
            ));
        }
    }
    (row_handles, column_handles)
}

fn active_cell_owns_axis_handle(
    ownership: Option<TableCellPosition>,
    axis: TableAxis,
    index: usize,
) -> bool {
    ownership.is_some_and(|cell| match axis {
        TableAxis::Row => cell.row == index,
        TableAxis::Column => cell.row == 0 && cell.col == index,
    })
}

fn active_cell_axis_handle_ownership(
    block_id: BlockId,
    focused_cell: Option<TableCellPosition>,
    axis_selection: Option<TableAxisSelection>,
    range_selection: Option<TableCellRangeSelection>,
) -> Option<TableCellPosition> {
    let axis_selection_owns_table =
        axis_selection.is_some_and(|selection| selection.block_id == block_id);
    let multi_cell_selection_owns_table = range_selection
        .is_some_and(|selection| selection.block_id == block_id && selection.is_multi_cell());
    (!axis_selection_owns_table && !multi_cell_selection_owns_table)
        .then_some(focused_cell)
        .flatten()
}

pub(crate) fn table_chrome_viewport_origins(vertical_offset_y: f32) -> TableChromeOrigins {
    let gutter_extent = TABLE_AXIS_HANDLE_SIZE_PX / 2.0;
    TableChromeOrigins {
        viewport: TableToolbarEditorOrigin {
            x_px: 0.0,
            y_px: vertical_offset_y,
        },
        top_edge: TableToolbarEditorOrigin {
            x_px: 0.0,
            y_px: gutter_extent,
        },
        scrolling_top_edge: TableToolbarEditorOrigin {
            x_px: 0.0,
            y_px: gutter_extent + vertical_offset_y,
        },
        left_edge: TableToolbarEditorOrigin {
            x_px: gutter_extent,
            y_px: vertical_offset_y,
        },
    }
}

pub(crate) fn render_table_chrome_viewport(
    editor_origin: TableToolbarEditorOrigin,
    viewport_width_px: f32,
    viewport_height_px: f32,
    overlays: TableChromeOverlays,
) -> AnyElement {
    let gutter_extent = TABLE_AXIS_HANDLE_SIZE_PX / 2.0;
    div()
        .absolute()
        .left(px(editor_origin.x_px))
        .top(px(editor_origin.y_px))
        .w(px(viewport_width_px))
        .h(px(viewport_height_px))
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .w(px(viewport_width_px))
                .h(px(viewport_height_px))
                .overflow_hidden()
                .children(overlays.viewport),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .top(px(-gutter_extent))
                .w(px(viewport_width_px))
                .h(px(gutter_extent + viewport_height_px))
                .overflow_hidden()
                .children(overlays.top_edge),
        )
        .child(
            div()
                .absolute()
                .left(px(-gutter_extent))
                .top_0()
                .w(px(gutter_extent + viewport_width_px))
                .h(px(viewport_height_px))
                .overflow_hidden()
                .children(overlays.left_edge),
        )
        .into_any_element()
}

#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
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
    let (width, height) = table_axis_handle_dimensions(axis, selected);
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
    let hover_background = table_axis_handle_hover_background(theme, selected);
    div()
        .absolute()
        .left(px(origin.x_px + left))
        .top(px(origin.y_px + top))
        .w(px(width))
        .h(px(height))
        .rounded(px(TABLE_AXIS_HANDLE_RADIUS_PX))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(table_axis_handle_background(theme, selected)))
        .cursor_pointer()
        .when(!selected, |this| this.opacity(0.0))
        .when(selected, |this| this.opacity(1.0))
        .hover(move |style| style.opacity(1.0).bg(rgb(hover_background)))
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            view.update(cx, |view, cx| {
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
        .child(render_table_axis_handle_icon(
            axis,
            table_axis_handle_foreground(theme, selected),
        ))
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

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{TableCellAlign, TablePayload};
    use cditor_runtime::{TableCellPosition, TableVisibleCell};

    use super::*;

    #[test]
    fn active_cell_owns_only_the_axis_handles_it_renders() {
        let first_row_cell = TableCellPosition { row: 0, col: 2 };
        let later_row_cell = TableCellPosition { row: 3, col: 2 };

        let first_row_ownership =
            active_cell_axis_handle_ownership(7, Some(first_row_cell), None, None);
        assert!(active_cell_owns_axis_handle(
            first_row_ownership,
            TableAxis::Row,
            0
        ));
        assert!(active_cell_owns_axis_handle(
            first_row_ownership,
            TableAxis::Column,
            2
        ));
        assert!(!active_cell_owns_axis_handle(
            first_row_ownership,
            TableAxis::Column,
            1
        ));

        let later_row_ownership =
            active_cell_axis_handle_ownership(7, Some(later_row_cell), None, None);
        assert!(active_cell_owns_axis_handle(
            later_row_ownership,
            TableAxis::Row,
            3
        ));
        assert!(!active_cell_owns_axis_handle(
            later_row_ownership,
            TableAxis::Column,
            2
        ));
    }

    #[test]
    fn table_selection_reclaims_axis_handle_ownership_from_the_active_cell() {
        let focused_cell = Some(TableCellPosition { row: 0, col: 1 });
        let axis_selection = TableAxisSelection::new(7, TableAxis::Column, 1);
        let range_selection = TableCellRangeSelection::new(7, 0, 0, 1, 1);

        assert_eq!(
            active_cell_axis_handle_ownership(7, focused_cell, Some(axis_selection), None),
            None
        );
        assert_eq!(
            active_cell_axis_handle_ownership(7, focused_cell, None, Some(range_selection)),
            None
        );
        assert_eq!(
            active_cell_axis_handle_ownership(
                8,
                focused_cell,
                Some(axis_selection),
                Some(range_selection),
            ),
            focused_cell
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
    fn table_chrome_viewport_clips_scrolled_cells_but_keeps_edge_gutters() {
        let origins = table_chrome_viewport_origins(0.0);

        assert_eq!(origins.viewport.x_px, 0.0);
        assert_eq!(origins.top_edge.y_px, 7.0);
        assert_eq!(origins.left_edge.x_px, 7.0);
        assert_eq!(origins.left_edge.x_px + TABLE_AXIS_ROW_HANDLE_LEFT_PX, 0.0);
        assert_eq!(origins.top_edge.y_px + TABLE_AXIS_COLUMN_HANDLE_TOP_PX, 0.0);
        assert!(origins.viewport.x_px - 80.0 < 0.0);
    }

    #[test]
    fn table_chrome_projects_vertical_scroll_without_moving_column_handles() {
        let origins = table_chrome_viewport_origins(-72.0);

        assert_eq!(origins.viewport.y_px, -72.0);
        assert_eq!(origins.left_edge.y_px, -72.0);
        assert_eq!(origins.top_edge.y_px, 7.0);
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
            table_content_cell_rect(&table_view, 0, 1),
            Some(TableOverlayRect {
                x: 120.0,
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
    fn last_row_active_border_uses_the_unclipped_projected_cell_rect() {
        let table_view = table_view_with_two_by_two_cells();
        let rect = table_cell_rect(&table_view, 1, 1).unwrap();

        assert_eq!(rect.y, 36.0);
        assert_eq!(rect.y + rect.height, table_view.height_px);
    }

    fn table_view_with_two_by_two_cells() -> TableViewState {
        TableViewState {
            table: TablePayload::default().into(),
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
            focused_cell_affinity: None,
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
            spans: Default::default(),
        }
    }
}
