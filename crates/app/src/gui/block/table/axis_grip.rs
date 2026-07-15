use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use super::selection::TableAxis;
use super::style::{TABLE_AXIS_HANDLE_SIZE_PX, TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX};

pub(super) const TABLE_COLUMN_HANDLE_DOT_ROWS: usize = 2;
pub(super) const TABLE_COLUMN_HANDLE_DOT_COLUMNS: usize = 3;
pub(super) const TABLE_ROW_HANDLE_DOT_ROWS: usize = 3;
pub(super) const TABLE_ROW_HANDLE_DOT_COLUMNS: usize = 2;

pub(super) fn table_axis_handle_dimensions(axis: TableAxis, expanded: bool) -> (f32, f32) {
    match (axis, expanded) {
        (TableAxis::Row, true) => (
            TABLE_AXIS_HANDLE_SIZE_PX,
            TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX,
        ),
        (TableAxis::Column, true) => (
            TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX,
            TABLE_AXIS_HANDLE_SIZE_PX,
        ),
        _ => (TABLE_AXIS_HANDLE_SIZE_PX, TABLE_AXIS_HANDLE_SIZE_PX),
    }
}

pub(super) fn render_table_axis_handle_icon(axis: TableAxis, color: u32) -> AnyElement {
    let (rows, columns, width, height) = match axis {
        TableAxis::Column => (
            TABLE_COLUMN_HANDLE_DOT_ROWS,
            TABLE_COLUMN_HANDLE_DOT_COLUMNS,
            10.0,
            8.0,
        ),
        TableAxis::Row => (
            TABLE_ROW_HANDLE_DOT_ROWS,
            TABLE_ROW_HANDLE_DOT_COLUMNS,
            8.0,
            10.0,
        ),
    };
    div()
        .w(px(width))
        .h(px(height))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(1.5))
        .children((0..rows).map(move |_| {
            div().flex().gap(px(1.5)).children(
                (0..columns)
                    .map(move |_| div().w(px(2.0)).h(px(2.0)).rounded(px(2.0)).bg(rgb(color))),
            )
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_axis_grips_use_directional_six_dot_shapes() {
        assert_eq!(
            TABLE_COLUMN_HANDLE_DOT_ROWS * TABLE_COLUMN_HANDLE_DOT_COLUMNS,
            6
        );
        assert_eq!(TABLE_ROW_HANDLE_DOT_ROWS * TABLE_ROW_HANDLE_DOT_COLUMNS, 6);
        assert!(TABLE_COLUMN_HANDLE_DOT_COLUMNS > TABLE_COLUMN_HANDLE_DOT_ROWS);
        assert!(TABLE_ROW_HANDLE_DOT_ROWS > TABLE_ROW_HANDLE_DOT_COLUMNS);
    }

    #[test]
    fn expanded_axis_grips_share_the_same_rotated_dimensions() {
        assert_eq!(
            table_axis_handle_dimensions(TableAxis::Column, true),
            (22.0, 14.0)
        );
        assert_eq!(
            table_axis_handle_dimensions(TableAxis::Row, true),
            (14.0, 22.0)
        );
    }
}
