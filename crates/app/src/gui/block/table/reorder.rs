use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, IntoElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_runtime::TableViewState;

use super::selection::TableAxis;
use super::style::TABLE_RESIZE_INDICATOR_THICKNESS_PX;

pub(crate) type TableReorderPreview = (BlockId, TableAxis, usize, usize);

pub(super) fn table_axis_track_sizes(table_view: &TableViewState, axis: TableAxis) -> Vec<f32> {
    let count = match axis {
        TableAxis::Row => table_view.row_count,
        TableAxis::Column => table_view.col_count,
    };
    (0..count)
        .map(|index| table_axis_track_size(table_view, axis, index).unwrap_or(0.0))
        .collect()
}

pub(super) fn render_table_reorder_indicator(
    block_id: BlockId,
    table_view: &TableViewState,
    preview: Option<TableReorderPreview>,
    theme: GuiTheme,
) -> Option<AnyElement> {
    let (preview_block_id, axis, _from, target) = preview?;
    if preview_block_id != block_id {
        return None;
    }
    let edge_px = table_reorder_indicator_edge_px(table_view, axis, target)?;
    Some(
        div()
            .absolute()
            .bg(rgb(theme.action_accent))
            .rounded(px(TABLE_RESIZE_INDICATOR_THICKNESS_PX))
            .when(axis == TableAxis::Column, |this| {
                this.left(px(edge_px - TABLE_RESIZE_INDICATOR_THICKNESS_PX / 2.0))
                    .top(px(0.0))
                    .w(px(TABLE_RESIZE_INDICATOR_THICKNESS_PX))
                    .h(px(table_view.height_px))
            })
            .when(axis == TableAxis::Row, |this| {
                this.left(px(0.0))
                    .top(px(edge_px - TABLE_RESIZE_INDICATOR_THICKNESS_PX / 2.0))
                    .w(px(table_view.width_px))
                    .h(px(TABLE_RESIZE_INDICATOR_THICKNESS_PX))
            })
            .into_any_element(),
    )
}

fn table_reorder_indicator_edge_px(
    table_view: &TableViewState,
    axis: TableAxis,
    target: usize,
) -> Option<f32> {
    let start = table_axis_track_start(table_view, axis, target)?;
    let size = table_axis_track_size(table_view, axis, target)?;
    Some(start + size)
}

fn table_axis_track_start(
    table_view: &TableViewState,
    axis: TableAxis,
    index: usize,
) -> Option<f32> {
    table_view
        .visible_cells
        .iter()
        .find(|cell| match axis {
            TableAxis::Row => cell.position.row == index && cell.row_span == 1,
            TableAxis::Column => cell.position.col == index && cell.col_span == 1,
        })
        .map(|cell| match axis {
            TableAxis::Row => cell.y_px,
            TableAxis::Column => cell.x_px,
        })
}

fn table_axis_track_size(
    table_view: &TableViewState,
    axis: TableAxis,
    index: usize,
) -> Option<f32> {
    table_view
        .visible_cells
        .iter()
        .find(|cell| match axis {
            TableAxis::Row => cell.position.row == index && cell.row_span == 1,
            TableAxis::Column => cell.position.col == index && cell.col_span == 1,
        })
        .map(|cell| match axis {
            TableAxis::Row => cell.height_px,
            TableAxis::Column => cell.width_px,
        })
}

#[cfg(test)]
mod tests {
    use cditor_runtime::{TableCellPosition, TableVisibleCell};

    use super::*;

    #[test]
    fn table_axis_track_sizes_follow_visible_geometry() {
        let table_view = table_view_with_two_by_two_cells();

        assert_eq!(
            table_axis_track_sizes(&table_view, TableAxis::Column),
            vec![120.0, 180.0]
        );
        assert_eq!(
            table_axis_track_sizes(&table_view, TableAxis::Row),
            vec![36.0, 48.0]
        );
    }

    #[test]
    fn table_reorder_indicator_uses_target_trailing_edge() {
        let table_view = table_view_with_two_by_two_cells();

        assert_eq!(
            table_reorder_indicator_edge_px(&table_view, TableAxis::Column, 0),
            Some(120.0)
        );
        assert_eq!(
            table_reorder_indicator_edge_px(&table_view, TableAxis::Row, 1),
            Some(84.0)
        );
    }

    fn table_view_with_two_by_two_cells() -> TableViewState {
        TableViewState {
            table: Default::default(),
            row_count: 2,
            col_count: 2,
            width_px: 300.0,
            height_px: 84.0,
            focused_cell: None,
            focused_cell_offset: None,
            visible_cells: vec![
                visible_cell(0, 0, 0.0, 0.0, 120.0, 36.0),
                visible_cell(0, 1, 120.0, 0.0, 180.0, 36.0),
                visible_cell(1, 0, 0.0, 36.0, 120.0, 48.0),
                visible_cell(1, 1, 120.0, 36.0, 180.0, 48.0),
            ],
        }
    }

    fn visible_cell(
        row: usize,
        col: usize,
        x_px: f32,
        y_px: f32,
        width_px: f32,
        height_px: f32,
    ) -> TableVisibleCell {
        TableVisibleCell {
            position: TableCellPosition { row, col },
            x_px,
            y_px,
            width_px,
            height_px,
            row_span: 1,
            col_span: 1,
            header: false,
            spans: Vec::new(),
            align: cditor_core::rich_text::TableCellAlign::Left,
            background_color: None,
        }
    }
}
