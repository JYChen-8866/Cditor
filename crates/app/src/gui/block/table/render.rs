use std::ops::Range;

use gpui::{
    AnyElement, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement, ScrollHandle,
    StatefulInteractiveElement, Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use cditor_core::ids::BlockId;
use cditor_core::layout::TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX;
use cditor_core::rich_text::{InlineSpan, TableCellAlign, plain_text_from_spans};
use cditor_runtime::{TableCellPosition, TableViewState};

use super::cell::{is_active_cell, render_table_cell};
use super::grid::render_table_grid;
use super::reorder::TableReorderPreview;
use super::resize::TableResizePreview;
use super::selection::{TableAxisSelection, TableCellRangeSelection};
use super::style::{
    V1_TABLE_EMPTY_PADDING_PX, V1_TABLE_RADIUS_PX, table_border_color, table_surface_background,
};
use super::text::TableCellTextElement;
use super::trace_table;

pub(crate) fn render_table_block(
    block_id: BlockId,
    content_version: u64,
    table_view: &TableViewState,
    theme: GuiTheme,
    marked_range: Option<Range<usize>>,
    table_selection: Option<TableAxisSelection>,
    table_range_selection: Option<TableCellRangeSelection>,
    _table_resize_preview: Option<TableResizePreview>,
    _table_reorder_preview: Option<TableReorderPreview>,
    table_scroll_handle: Option<ScrollHandle>,
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
            "block={block_id} content_version={content_version} rows={} cols={} focused_cell={focused_cell:?} focused_cell_offset={focused_cell_offset:?} marked={marked_range:?}",
            table_view.row_count,
            table_view.col_count,
            focused_cell = table_view.focused_cell,
            focused_cell_offset = table_view.focused_cell_offset,
        ),
    );
    let table_content = div()
        .relative()
        // Fixed track size so the table keeps its intrinsic width and overflows
        // the scroll viewport instead of being squeezed by the flex parent.
        .flex_none()
        .w(px(table_view.width_px))
        .h(px(table_view.height_px))
        .child(
            div()
                .relative()
                .w(px(table_view.width_px))
                .h(px(table_view.height_px))
                .rounded(px(V1_TABLE_RADIUS_PX))
                .bg(rgb(table_surface_background(theme)))
                .overflow_hidden()
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
                        table_view.focused_cell_selection_range.clone(),
                        marked_range.clone(),
                        cell.header,
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
                        view.clone(),
                        block_id,
                    )
                }))
                .child(render_table_grid(table_view, theme)),
        );

    // Always wrap in a horizontally scrollable viewport that fills the available
    // content width. A narrow table sits at its natural width with no overflow;
    // a table wider than the viewport overflows and can be scrolled sideways.
    let mut viewport = div()
        .id(("table_scroll_container", block_id))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .overflow_x_scroll();
    if let Some(handle) = &table_scroll_handle {
        viewport = viewport.track_scroll(handle);
    }
    let viewport = viewport.child(table_content);

    // The custom horizontal scrollbar is rendered by the editor overlay layer.
    // This wrapper only reserves chrome height so following blocks are laid out
    // below the table chrome.
    div()
        .relative()
        .w_full()
        .min_w(px(0.0))
        .pb(px(TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX as f32))
        .child(viewport)
        .into_any_element()
}

fn render_table_cell_content(
    block_id: BlockId,
    content_version: u64,
    spans: Vec<InlineSpan>,
    active: bool,
    focused_cell_offset: Option<usize>,
    selected_range: Option<Range<usize>>,
    marked_range: Option<Range<usize>>,
    header: bool,
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
        active.then_some(selected_range).flatten(),
        active.then_some(marked_range).flatten(),
        header,
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
