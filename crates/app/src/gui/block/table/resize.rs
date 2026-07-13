use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, ParentElement, Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use cditor_core::ids::BlockId;
use cditor_runtime::TableViewState;

use super::selection::TableAxis;
use super::style::{TABLE_RESIZE_HANDLE_LINE_THICKNESS_PX, TABLE_RESIZE_HANDLE_THICKNESS_PX};
use super::toolbar::TableToolbarEditorOrigin;

pub(crate) type TableResizePreview = (BlockId, TableAxis, usize, f32);

#[derive(Debug, Clone, Copy, PartialEq)]
struct TableResizeTrack {
    axis: TableAxis,
    index: usize,
    start_px: f32,
    size_px: f32,
    extent_px: f32,
}

impl TableResizeTrack {
    fn edge_px(self) -> f32 {
        self.start_px + self.size_px
    }
}

pub(crate) fn render_table_resize_overlays(
    block_id: BlockId,
    table_view: &TableViewState,
    origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> Vec<AnyElement> {
    column_resize_tracks(table_view)
        .into_iter()
        .map(|track| render_table_resize_handle(block_id, track, origin, theme, view.clone()))
        .collect()
}

fn render_table_resize_handle(
    block_id: BlockId,
    track: TableResizeTrack,
    origin: TableToolbarEditorOrigin,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let color = theme.action_accent;
    let start_view = view.clone();
    div()
        .absolute()
        .when(track.axis == TableAxis::Column, |this| {
            this.left(px(
                origin.x_px + track.edge_px() - TABLE_RESIZE_HANDLE_THICKNESS_PX / 2.0
            ))
            .top(px(origin.y_px))
            .w(px(TABLE_RESIZE_HANDLE_THICKNESS_PX))
            .h(px(track.extent_px))
            .cursor_col_resize()
        })
        .when(track.axis == TableAxis::Row, |this| {
            this.left(px(origin.x_px))
                .top(px(
                    origin.y_px + track.edge_px() - TABLE_RESIZE_HANDLE_THICKNESS_PX / 2.0
                ))
                .w(px(track.extent_px))
                .h(px(TABLE_RESIZE_HANDLE_THICKNESS_PX))
                .cursor_row_resize()
        })
        .opacity(table_resize_handle_idle_opacity(track.axis))
        .hover(|style| style.opacity(1.0))
        .on_mouse_down(gpui::MouseButton::Left, move |event, window, cx| {
            let _ = start_view.update(cx, |view, cx| {
                view.start_table_resize_from_gui(
                    block_id,
                    track.axis,
                    track.index,
                    track.size_px,
                    event.position,
                    window,
                    cx,
                );
            });
            cx.stop_propagation();
        })
        .child(resize_handle_line(track, color))
        .into_any_element()
}

fn table_resize_handle_idle_opacity(_axis: TableAxis) -> f32 {
    0.0
}

fn resize_handle_line(track: TableResizeTrack, color: u32) -> AnyElement {
    div()
        .absolute()
        .bg(rgb(color))
        .rounded(px(TABLE_RESIZE_HANDLE_LINE_THICKNESS_PX))
        .when(track.axis == TableAxis::Column, |this| {
            this.left(px((TABLE_RESIZE_HANDLE_THICKNESS_PX
                - TABLE_RESIZE_HANDLE_LINE_THICKNESS_PX)
                / 2.0))
                .top(px(0.0))
                .w(px(TABLE_RESIZE_HANDLE_LINE_THICKNESS_PX))
                .h_full()
        })
        .when(track.axis == TableAxis::Row, |this| {
            this.left(px(0.0))
                .top(px((TABLE_RESIZE_HANDLE_THICKNESS_PX
                    - TABLE_RESIZE_HANDLE_LINE_THICKNESS_PX)
                    / 2.0))
                .w_full()
                .h(px(TABLE_RESIZE_HANDLE_LINE_THICKNESS_PX))
        })
        .into_any_element()
}

fn column_resize_tracks(table_view: &TableViewState) -> Vec<TableResizeTrack> {
    (0..table_view.col_count)
        .filter_map(|index| resize_track(table_view, TableAxis::Column, index))
        .collect()
}

#[allow(dead_code)]
fn row_resize_tracks(table_view: &TableViewState) -> Vec<TableResizeTrack> {
    (0..table_view.row_count)
        .filter_map(|index| resize_track(table_view, TableAxis::Row, index))
        .collect()
}

fn resize_track(
    table_view: &TableViewState,
    axis: TableAxis,
    index: usize,
) -> Option<TableResizeTrack> {
    let sizes = match axis {
        TableAxis::Column => &table_view.column_widths_px,
        TableAxis::Row => &table_view.row_heights_px,
    };
    let size_px = *sizes.get(index)?;
    let start_px = sizes[..index].iter().sum::<f32>()
        + if axis == TableAxis::Column {
            table_view.horizontal_scroll_offset_px
        } else {
            0.0
        };
    Some(TableResizeTrack {
        axis,
        index,
        start_px,
        size_px,
        extent_px: match axis {
            TableAxis::Column => table_view.height_px,
            TableAxis::Row => table_view.width_px,
        },
    })
}

pub(crate) fn table_resize_indicator_edge_px(
    table_view: &TableViewState,
    axis: TableAxis,
    index: usize,
    preview_size_px: f32,
) -> Option<f32> {
    resize_track(table_view, axis, index).map(|track| track.start_px + preview_size_px.max(0.0))
}

#[cfg(test)]
mod tests {
    use cditor_runtime::{TableCellPosition, TableVisibleCell};

    use super::*;

    #[test]
    fn table_resize_tracks_follow_visible_cell_geometry() {
        let table_view = table_view_with_two_by_two_cells();

        let columns = column_resize_tracks(&table_view);
        let rows = row_resize_tracks(&table_view);

        assert_eq!(
            columns[0],
            TableResizeTrack {
                axis: TableAxis::Column,
                index: 0,
                start_px: 0.0,
                size_px: 120.0,
                extent_px: 72.0
            }
        );
        assert_eq!(columns[1].edge_px(), 240.0);
        assert_eq!(rows[0].edge_px(), 36.0);
        assert_eq!(rows[1].extent_px, 240.0);
    }

    #[test]
    fn table_resize_preview_indicator_uses_track_start_plus_preview_size() {
        let table_view = table_view_with_two_by_two_cells();

        assert_eq!(
            table_resize_indicator_edge_px(&table_view, TableAxis::Column, 1, 180.0).unwrap(),
            300.0
        );
        assert_eq!(
            table_resize_indicator_edge_px(&table_view, TableAxis::Row, 1, 48.0).unwrap(),
            84.0
        );
    }

    #[test]
    fn column_resize_handles_are_hidden_before_hover() {
        assert_eq!(table_resize_handle_idle_opacity(TableAxis::Column), 0.0);
        assert_eq!(table_resize_handle_idle_opacity(TableAxis::Row), 0.0);
    }

    #[test]
    fn merged_cells_do_not_hide_underlying_column_resize_tracks() {
        let mut table_view = table_view_with_two_by_two_cells();
        table_view.visible_cells = vec![TableVisibleCell {
            position: TableCellPosition { row: 0, col: 0 },
            row_span: 2,
            col_span: 2,
            x_px: 0.0,
            y_px: 0.0,
            width_px: 240.0,
            height_px: 72.0,
            header: false,
            spans: Vec::new(),
            align: cditor_core::rich_text::TableCellAlign::Left,
            background_color: None,
        }];

        let columns = column_resize_tracks(&table_view);

        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].edge_px(), 120.0);
        assert_eq!(columns[1].edge_px(), 240.0);
    }

    fn table_view_with_two_by_two_cells() -> TableViewState {
        TableViewState {
            table: Default::default(),
            row_count: 2,
            col_count: 2,
            width_px: 240.0,
            height_px: 72.0,
            column_widths_px: vec![120.0, 120.0],
            row_heights_px: vec![36.0, 36.0],
            horizontal_scroll_offset_px: 0.0,
            focused_cell: None,
            focused_cell_offset: None,
            focused_cell_selection_range: None,
            visible_cells: vec![
                visible_cell(0, 0, 0.0, 0.0),
                visible_cell(0, 1, 120.0, 0.0),
                visible_cell(1, 0, 0.0, 36.0),
                visible_cell(1, 1, 120.0, 36.0),
            ],
        }
    }

    fn visible_cell(row: usize, col: usize, x_px: f32, y_px: f32) -> TableVisibleCell {
        TableVisibleCell {
            position: TableCellPosition { row, col },
            x_px,
            y_px,
            width_px: 120.0,
            height_px: 36.0,
            row_span: 1,
            col_span: 1,
            header: false,
            spans: Vec::new(),
            align: cditor_core::rich_text::TableCellAlign::Left,
            background_color: None,
        }
    }
}
