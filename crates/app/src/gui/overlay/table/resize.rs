use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, IntoElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::block::table::{
    TABLE_RESIZE_INDICATOR_THICKNESS_PX, TableResizePreview, TableToolbarEditorOrigin,
    table_resize_indicator_edge_px,
};
use cditor_core::ids::BlockId;
use cditor_runtime::TableViewState;

use crate::gui::block::table::TableAxis;

pub(crate) fn render_table_resize_preview_overlay(
    block_id: BlockId,
    table_view: &TableViewState,
    origin: TableToolbarEditorOrigin,
    preview: Option<TableResizePreview>,
    theme: GuiTheme,
) -> Option<AnyElement> {
    let (preview_block_id, axis, index, size_px) = preview?;
    if preview_block_id != block_id {
        return None;
    }
    let edge_px = table_resize_indicator_edge_px(table_view, axis, index, size_px)?;
    Some(
        div()
            .absolute()
            .bg(rgb(theme.action_accent))
            .rounded(px(TABLE_RESIZE_INDICATOR_THICKNESS_PX))
            .when(axis == TableAxis::Column, |this| {
                this.left(px(
                    origin.x_px + edge_px - TABLE_RESIZE_INDICATOR_THICKNESS_PX / 2.0
                ))
                .top(px(origin.y_px))
                .w(px(TABLE_RESIZE_INDICATOR_THICKNESS_PX))
                .h(px(table_view.height_px))
            })
            .when(axis == TableAxis::Row, |this| {
                this.left(px(origin.x_px))
                    .top(px(
                        origin.y_px + edge_px - TABLE_RESIZE_INDICATOR_THICKNESS_PX / 2.0
                    ))
                    .w(px(table_view.width_px))
                    .h(px(TABLE_RESIZE_INDICATOR_THICKNESS_PX))
            })
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_preview_overlay_origin_adds_editor_table_origin() {
        let origin = TableToolbarEditorOrigin {
            x_px: 50.0,
            y_px: 100.0,
        };
        let edge_px = 120.0;

        assert_eq!(
            origin.x_px + edge_px - TABLE_RESIZE_INDICATOR_THICKNESS_PX / 2.0,
            169.0
        );
    }
}
