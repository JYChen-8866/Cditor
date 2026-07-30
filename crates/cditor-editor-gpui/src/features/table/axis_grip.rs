use cditor_component::SvgIcon;
use gpui::{AnyElement, IntoElement, px, rgb};

use super::selection::TableAxis;
use super::style::{TABLE_AXIS_HANDLE_SIZE_PX, TABLE_AXIS_SELECTED_HANDLE_LONG_EDGE_PX};

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
    const GUTTER_VERTICAL: &[u8] = include_bytes!("../../../../../assets/icons/gutter.svg");
    const GUTTER_HORIZONTAL: &[u8] =
        include_bytes!("../../../../../assets/icons/gutter-horizontal.svg");
    let (key, gutter) = match axis {
        TableAxis::Row => ("table-gutter-vertical", GUTTER_VERTICAL),
        TableAxis::Column => ("table-gutter-horizontal", GUTTER_HORIZONTAL),
    };
    SvgIcon::new(key, gutter)
        .color(rgb(color))
        .size(px(14.0))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_axis_grips_use_the_shared_gutter_asset() {
        const VERTICAL: &[u8] = include_bytes!("../../../../../assets/icons/gutter.svg");
        const HORIZONTAL: &[u8] =
            include_bytes!("../../../../../assets/icons/gutter-horizontal.svg");
        assert!(
            std::str::from_utf8(VERTICAL)
                .unwrap()
                .contains("cx=\"9\" cy=\"12\"")
        );
        assert!(
            std::str::from_utf8(HORIZONTAL)
                .unwrap()
                .contains("cx=\"12\" cy=\"9\"")
        );
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
