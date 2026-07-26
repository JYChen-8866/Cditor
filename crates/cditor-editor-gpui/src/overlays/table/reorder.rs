use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::features::table::{
    TABLE_RESIZE_INDICATOR_THICKNESS_PX, TableAxis, TableReorderPreview, TableToolbarEditorOrigin,
    table_reorder_indicator_edge_px_for_preview,
};
use crate::theme::GuiTheme;
use cditor_core::ids::BlockId;
use cditor_runtime::TableViewState;

#[derive(Clone, Copy)]
pub(crate) struct TableReorderOverlayViewport {
    pub origin: TableToolbarEditorOrigin,
    pub width_px: f32,
    pub height_px: f32,
    pub content_offset_y: f32,
}

pub(crate) fn render_table_reorder_preview_overlay(
    block_id: BlockId,
    table_view: &TableViewState,
    viewport: TableReorderOverlayViewport,
    preview: Option<TableReorderPreview>,
    theme: GuiTheme,
) -> Option<AnyElement> {
    let (axis, edge_px) =
        table_reorder_indicator_edge_px_for_preview(block_id, table_view, preview)?;
    Some(
        div()
            .absolute()
            .left(px(viewport.origin.x_px))
            .top(px(viewport.origin.y_px))
            .w(px(viewport.width_px))
            .h(px(viewport.height_px))
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .bg(rgb(theme.action_accent))
                    .rounded(px(TABLE_RESIZE_INDICATOR_THICKNESS_PX))
                    .when(axis == TableAxis::Column, |this| {
                        this.left(px(edge_px - TABLE_RESIZE_INDICATOR_THICKNESS_PX / 2.0))
                            .top(px(viewport.content_offset_y))
                            .w(px(TABLE_RESIZE_INDICATOR_THICKNESS_PX))
                            .h(px(table_view.height_px))
                    })
                    .when(axis == TableAxis::Row, |this| {
                        this.left(px(table_view.horizontal_scroll_offset_px))
                            .top(px(viewport.content_offset_y + edge_px
                                - TABLE_RESIZE_INDICATOR_THICKNESS_PX / 2.0))
                            .w(px(table_view.width_px))
                            .h(px(TABLE_RESIZE_INDICATOR_THICKNESS_PX))
                    }),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorder_preview_overlay_projects_scroll_inside_the_clipped_viewport() {
        let origin = TableToolbarEditorOrigin {
            x_px: 50.0,
            y_px: 100.0,
        };
        let edge_px = 84.0;
        let content_offset_y = -36.0;

        assert_eq!(
            origin.y_px + content_offset_y + edge_px - TABLE_RESIZE_INDICATOR_THICKNESS_PX / 2.0,
            147.0
        );
    }
}
