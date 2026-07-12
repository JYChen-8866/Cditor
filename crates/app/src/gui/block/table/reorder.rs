use cditor_core::ids::BlockId;
use cditor_runtime::TableViewState;

use super::selection::TableAxis;

pub(crate) type TableReorderPreview = (BlockId, TableAxis, usize, usize);

pub(crate) fn table_axis_track_sizes(table_view: &TableViewState, axis: TableAxis) -> Vec<f32> {
    match axis {
        TableAxis::Row => table_view.row_heights_px.clone(),
        TableAxis::Column => table_view.column_widths_px.clone(),
    }
}

pub(crate) fn table_reorder_indicator_edge_px_for_preview(
    block_id: BlockId,
    table_view: &TableViewState,
    preview: Option<TableReorderPreview>,
) -> Option<(TableAxis, f32)> {
    let (preview_block_id, axis, _from, target) = preview?;
    if preview_block_id != block_id {
        return None;
    }
    table_reorder_indicator_edge_px(table_view, axis, target).map(|edge_px| (axis, edge_px))
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
    let sizes = match axis {
        TableAxis::Row => &table_view.row_heights_px,
        TableAxis::Column => &table_view.column_widths_px,
    };
    (index < sizes.len()).then(|| {
        sizes[..index].iter().sum::<f32>()
            + if axis == TableAxis::Column {
                table_view.horizontal_scroll_offset_px
            } else {
                0.0
            }
    })
}

fn table_axis_track_size(
    table_view: &TableViewState,
    axis: TableAxis,
    index: usize,
) -> Option<f32> {
    match axis {
        TableAxis::Row => table_view.row_heights_px.get(index).copied(),
        TableAxis::Column => table_view.column_widths_px.get(index).copied(),
    }
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
            column_widths_px: vec![120.0, 180.0],
            row_heights_px: vec![36.0, 48.0],
            horizontal_scroll_offset_px: 0.0,
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
