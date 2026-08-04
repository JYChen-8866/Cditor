use std::borrow::Cow;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, ParentElement, Styled, div, px, rgb,
};

use crate::editor_view::CditorV2View;
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_runtime::TableViewState;

use super::selection::TableAxis;
use super::style::{TABLE_RESIZE_HANDLE_LINE_THICKNESS_PX, TABLE_RESIZE_HANDLE_THICKNESS_PX};
use super::toolbar::TableToolbarEditorOrigin;

pub(crate) type TableResizePreview = (BlockId, TableAxis, usize, f32);

pub(crate) fn table_view_with_resize_preview<'a>(
    block_id: BlockId,
    table_view: &'a TableViewState,
    preview: Option<TableResizePreview>,
) -> Cow<'a, TableViewState> {
    let Some((preview_block_id, axis, index, size_px)) = preview else {
        return Cow::Borrowed(table_view);
    };
    if preview_block_id != block_id {
        return Cow::Borrowed(table_view);
    }

    let track_sizes = match axis {
        TableAxis::Column => &table_view.column_widths_px,
        TableAxis::Row => &table_view.row_heights_px,
    };
    if index >= track_sizes.len() {
        return Cow::Borrowed(table_view);
    }

    let mut preview_view = table_view.clone();
    match axis {
        TableAxis::Column => preview_view.column_widths_px[index] = size_px.max(0.0),
        TableAxis::Row => preview_view.row_heights_px[index] = size_px.max(0.0),
    }
    preview_view.width_px = preview_view.column_widths_px.iter().sum();
    preview_view.height_px = preview_view.row_heights_px.iter().sum();

    let x_offsets = prefix_offsets(&preview_view.column_widths_px);
    let y_offsets = prefix_offsets(&preview_view.row_heights_px);
    for cell in &mut preview_view.visible_cells {
        let row = cell.position.row;
        let col = cell.position.col;
        cell.x_px = x_offsets.get(col).copied().unwrap_or(cell.x_px);
        cell.y_px = y_offsets.get(row).copied().unwrap_or(cell.y_px);
        cell.width_px = span_size(&preview_view.column_widths_px, col, cell.col_span);
        cell.height_px = span_size(&preview_view.row_heights_px, row, cell.row_span);
    }
    Cow::Owned(preview_view)
}

fn prefix_offsets(track_sizes: &[f32]) -> Vec<f32> {
    let mut offset = 0.0;
    track_sizes
        .iter()
        .map(|size| {
            let current = offset;
            offset += size;
            current
        })
        .collect()
}

fn span_size(track_sizes: &[f32], start: usize, span: usize) -> f32 {
    track_sizes
        .get(start..start.saturating_add(span))
        .unwrap_or_default()
        .iter()
        .sum()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TableResizeTrack {
    axis: TableAxis,
    index: usize,
    start_px: f32,
    size_px: f32,
    cross_start_px: f32,
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
    let start_view = view;
    div()
        .absolute()
        .when(track.axis == TableAxis::Column, |this| {
            this.left(px(
                origin.x_px + track.edge_px() - TABLE_RESIZE_HANDLE_THICKNESS_PX / 2.0
            ))
            .top(px(origin.y_px + track.cross_start_px))
            .w(px(TABLE_RESIZE_HANDLE_THICKNESS_PX))
            .h(px(track.extent_px))
            .cursor_col_resize()
        })
        .when(track.axis == TableAxis::Row, |this| {
            this.left(px(origin.x_px + track.cross_start_px))
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
            start_view.update(cx, |view, cx| {
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
    let exclusion = focused_cell_resize_exclusion(table_view);
    (0..table_view.col_count)
        .filter_map(|index| resize_track(table_view, TableAxis::Column, index))
        .flat_map(|track| {
            let excluded_range = exclusion
                .filter(|exclusion| exclusion.includes_column_edge(track.index))
                .map(|exclusion| exclusion.start_px..exclusion.end_px);
            resize_track_segments(track, excluded_range)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FocusedCellResizeExclusion {
    first_column_edge: usize,
    last_column_edge: usize,
    start_px: f32,
    end_px: f32,
}

impl FocusedCellResizeExclusion {
    fn includes_column_edge(self, index: usize) -> bool {
        index >= self.first_column_edge && index <= self.last_column_edge
    }
}

fn focused_cell_resize_exclusion(
    table_view: &TableViewState,
) -> Option<FocusedCellResizeExclusion> {
    let focused = table_view.focused_cell?;
    let cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == focused)?;
    Some(FocusedCellResizeExclusion {
        first_column_edge: focused.col,
        last_column_edge: focused.col.checked_add(cell.col_span.saturating_sub(1))?,
        start_px: cell.y_px.max(0.0),
        end_px: (cell.y_px + cell.height_px).min(table_view.height_px),
    })
}

fn resize_track_segments(
    track: TableResizeTrack,
    excluded_range: Option<std::ops::Range<f32>>,
) -> Vec<TableResizeTrack> {
    let Some(excluded_range) = excluded_range else {
        return vec![track];
    };
    let track_end = track.cross_start_px + track.extent_px;
    let excluded_start = excluded_range.start.clamp(track.cross_start_px, track_end);
    let excluded_end = excluded_range.end.clamp(excluded_start, track_end);
    let mut segments = Vec::with_capacity(2);
    if excluded_start > track.cross_start_px {
        segments.push(TableResizeTrack {
            extent_px: excluded_start - track.cross_start_px,
            ..track
        });
    }
    if excluded_end < track_end {
        segments.push(TableResizeTrack {
            cross_start_px: excluded_end,
            extent_px: track_end - excluded_end,
            ..track
        });
    }
    segments
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
        cross_start_px: 0.0,
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
                cross_start_px: 0.0,
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
    fn column_resize_preview_reflows_table_and_following_cells() {
        let table_view = table_view_with_two_by_two_cells();

        let preview =
            table_view_with_resize_preview(7, &table_view, Some((7, TableAxis::Column, 0, 180.0)));

        assert_eq!(preview.column_widths_px, vec![180.0, 120.0]);
        assert_eq!(preview.width_px, 300.0);
        assert_eq!(preview.visible_cells[0].width_px, 180.0);
        assert_eq!(preview.visible_cells[1].x_px, 180.0);
        assert_eq!(table_view.width_px, 240.0, "preview must stay UI-transient");
    }

    #[test]
    fn row_resize_preview_reflows_table_and_following_cells() {
        let table_view = table_view_with_two_by_two_cells();

        let preview =
            table_view_with_resize_preview(7, &table_view, Some((7, TableAxis::Row, 0, 52.0)));

        assert_eq!(preview.row_heights_px, vec![52.0, 36.0]);
        assert_eq!(preview.height_px, 88.0);
        assert_eq!(preview.visible_cells[0].height_px, 52.0);
        assert_eq!(preview.visible_cells[2].y_px, 52.0);
    }

    #[test]
    fn resize_preview_recomputes_merged_cell_spans() {
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
            spans: Default::default(),
            align: cditor_core::rich_text::TableCellAlign::Left,
            background_color: None,
        }];

        let column_preview =
            table_view_with_resize_preview(7, &table_view, Some((7, TableAxis::Column, 1, 160.0)));
        assert_eq!(column_preview.visible_cells[0].width_px, 280.0);

        let row_preview =
            table_view_with_resize_preview(7, &table_view, Some((7, TableAxis::Row, 1, 48.0)));
        assert_eq!(row_preview.visible_cells[0].height_px, 84.0);
    }

    #[test]
    fn unrelated_or_invalid_resize_preview_reuses_projection() {
        let table_view = table_view_with_two_by_two_cells();

        let unrelated =
            table_view_with_resize_preview(7, &table_view, Some((8, TableAxis::Column, 0, 180.0)));
        let invalid =
            table_view_with_resize_preview(7, &table_view, Some((7, TableAxis::Column, 9, 180.0)));

        assert!(matches!(unrelated, Cow::Borrowed(_)));
        assert!(matches!(invalid, Cow::Borrowed(_)));
        assert_eq!(unrelated.as_ref(), &table_view);
        assert_eq!(invalid.as_ref(), &table_view);
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
            spans: Default::default(),
            align: cditor_core::rich_text::TableCellAlign::Left,
            background_color: None,
        }];

        let columns = column_resize_tracks(&table_view);

        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].edge_px(), 120.0);
        assert_eq!(columns[1].edge_px(), 240.0);
    }

    #[test]
    fn focused_cell_removes_resize_hitboxes_only_from_its_own_rows() {
        let mut table_view = table_view_with_two_by_two_cells();
        table_view.focused_cell = Some(TableCellPosition { row: 0, col: 0 });
        let columns = column_resize_tracks(&table_view);

        let focused_edge_segments = columns
            .iter()
            .filter(|track| track.index == 0)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(focused_edge_segments.len(), 1);
        assert_eq!(focused_edge_segments[0].cross_start_px, 36.0);
        assert_eq!(focused_edge_segments[0].extent_px, 36.0);
        assert!(columns.iter().any(|track| {
            track.index == 1 && track.cross_start_px == 0.0 && track.extent_px == 72.0
        }));
    }

    #[test]
    fn focused_merged_cell_excludes_every_boundary_inside_its_surface() {
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
            spans: Default::default(),
            align: cditor_core::rich_text::TableCellAlign::Left,
            background_color: None,
        }];
        table_view.focused_cell = Some(TableCellPosition { row: 0, col: 0 });

        assert!(column_resize_tracks(&table_view).is_empty());
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
            focused_cell_affinity: None,
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
            spans: Default::default(),
            align: cditor_core::rich_text::TableCellAlign::Left,
            background_color: None,
        }
    }
}
