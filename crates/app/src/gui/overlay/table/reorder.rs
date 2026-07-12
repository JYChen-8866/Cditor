use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, IntoElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::block::table::{
    TABLE_RESIZE_INDICATOR_THICKNESS_PX, TableAxis, TableReorderPreview, TableToolbarEditorOrigin,
    table_reorder_indicator_edge_px_for_preview,
};
use cditor_core::ids::BlockId;
use cditor_runtime::TableViewState;

pub(crate) fn render_table_reorder_preview_overlay(
    block_id: BlockId,
    table_view: &TableViewState,
    origin: TableToolbarEditorOrigin,
    preview: Option<TableReorderPreview>,
    theme: GuiTheme,
) -> Option<AnyElement> {
    let (axis, edge_px) =
        table_reorder_indicator_edge_px_for_preview(block_id, table_view, preview)?;
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
    fn reorder_preview_overlay_origin_adds_editor_table_origin() {
        let origin = TableToolbarEditorOrigin {
            x_px: 50.0,
            y_px: 100.0,
        };
        let edge_px = 84.0;

        assert_eq!(
            origin.y_px + edge_px - TABLE_RESIZE_INDICATOR_THICKNESS_PX / 2.0,
            183.0
        );
    }
}
