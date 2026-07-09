use std::ops::Range;

use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, Entity, FocusHandle, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{InlineSpan, TableCellAlign, plain_text_from_spans};
use cditor_runtime::{TableCellPosition, TableViewState};

use super::cell::{is_active_cell, render_table_cell};
use super::reorder::{TableReorderPreview, render_table_reorder_indicator, table_axis_track_sizes};
use super::resize::{TableResizePreview, render_table_resize_overlays};
use super::selection::{TableAxis, TableAxisSelection, TableCellRangeSelection};
use super::style::{
    V1_TABLE_EMPTY_PADDING_PX, V1_TABLE_RADIUS_PX, table_border_color, table_surface_background,
};
use super::text::TableCellTextElement;
use super::toolbar::{render_table_axis_toolbar, render_table_range_toolbar};
use super::trace_table;

pub(crate) fn render_table_block(
    block_id: BlockId,
    content_version: u64,
    table_view: &TableViewState,
    theme: GuiTheme,
    marked_range: Option<Range<usize>>,
    table_selection: Option<TableAxisSelection>,
    table_range_selection: Option<TableCellRangeSelection>,
    table_resize_preview: Option<TableResizePreview>,
    table_reorder_preview: Option<TableReorderPreview>,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
) -> AnyElement {
    if table_view.visible_cells.is_empty() {
        trace_table(
            "render.empty",
            format_args!("block={block_id} content_version={content_version}"),
        );
        return render_empty_table(theme);
    }
    trace_table(
        "render.block",
        format_args!(
            "block={block_id} content_version={content_version} rows={} cols={} focused_cell={focused_cell:?} focused_offset={focused_cell_offset:?} marked={marked_range:?}",
            table_view.row_count,
            table_view.col_count,
            focused_cell = table_view.focused_cell,
            focused_cell_offset = table_view.focused_cell_offset,
        ),
    );
    let row_track_sizes = table_axis_track_sizes(table_view, TableAxis::Row);
    let column_track_sizes = table_axis_track_sizes(table_view, TableAxis::Column);

    div()
        .relative()
        .w(px(table_view.width_px))
        .h(px(table_view.height_px))
        .rounded(px(V1_TABLE_RADIUS_PX))
        .border_1()
        .border_color(rgb(table_border_color(theme)))
        .bg(rgb(table_surface_background(theme)))
        .child(
            div()
                .relative()
                .w(px(table_view.width_px))
                .h(px(table_view.height_px))
                .children(table_view.visible_cells.iter().map(|cell| {
                    let active = is_active_cell(
                        table_view.focused_cell,
                        cell.position.row,
                        cell.position.col,
                    );
                    let content = render_table_cell_content(
                        block_id,
                        content_version,
                        cell.spans.clone(),
                        active,
                        table_view.focused_cell_offset,
                        marked_range.clone(),
                        theme,
                        view.clone(),
                        focus.clone(),
                        cell.position,
                        cell.align,
                    );
                    render_table_cell(
                        cell,
                        content,
                        theme,
                        table_view.focused_cell,
                        table_selection,
                        table_range_selection,
                        &row_track_sizes,
                        &column_track_sizes,
                        view.clone(),
                        block_id,
                    )
                }))
                .when_some(table_selection, |this, selection| {
                    this.child(render_table_axis_toolbar(
                        selection,
                        table_view,
                        theme,
                        view.clone(),
                    ))
                })
                .when_some(table_range_selection, |this, selection| {
                    this.child(render_table_range_toolbar(
                        selection,
                        table_view,
                        theme,
                        view.clone(),
                    ))
                })
                .children(render_table_resize_overlays(
                    block_id,
                    table_view,
                    table_resize_preview,
                    theme,
                    view,
                ))
                .when_some(
                    render_table_reorder_indicator(
                        block_id,
                        table_view,
                        table_reorder_preview,
                        theme,
                    ),
                    |this, indicator| this.child(indicator),
                ),
        )
        .into_any_element()
}

fn render_table_cell_content(
    block_id: BlockId,
    content_version: u64,
    spans: Vec<InlineSpan>,
    active: bool,
    focused_cell_offset: Option<usize>,
    marked_range: Option<Range<usize>>,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    position: TableCellPosition,
    align: TableCellAlign,
) -> AnyElement {
    let text = plain_text_from_spans(&spans);
    if active {
        trace_table(
            "render.active_cell",
            format_args!(
                "block={block_id} row={} col={} text_len={} caret={focused_cell_offset:?} marked={marked_range:?}",
                position.row,
                position.col,
                text.len()
            ),
        );
    }
    TableCellTextElement::new(
        block_id,
        content_version,
        position,
        text,
        active,
        focused_cell_offset,
        active.then_some(marked_range).flatten(),
        theme,
        view,
        focus,
        align,
    )
    .into_any_element()
}

fn render_empty_table(theme: GuiTheme) -> AnyElement {
    div()
        .rounded(px(V1_TABLE_RADIUS_PX))
        .border_1()
        .border_color(rgb(table_border_color(theme)))
        .bg(rgb(table_surface_background(theme)))
        .p(px(V1_TABLE_EMPTY_PADDING_PX))
        .text_color(rgb(theme.muted))
        .child("Empty table")
        .into_any_element()
}
