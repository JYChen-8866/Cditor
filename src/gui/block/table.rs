use gpui::{
    AnyElement, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement, Styled, div,
    px, rgb,
};

use crate::core::ids::BlockId;
use crate::core::rich_text::TablePayload;
use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::input::focus_table_cell_from_mouse;
use crate::gui::rich_text::render_inline_spans;
use crate::runtime::TableCellPosition;

pub const V1_TABLE_RADIUS_PX: f32 = 8.0;
pub const V1_TABLE_CELL_MIN_WIDTH_PX: f32 = 120.0;
pub const V1_TABLE_CELL_PADDING_X_PX: f32 = 10.0;
pub const V1_TABLE_CELL_PADDING_Y_PX: f32 = 8.0;
pub const V1_TABLE_EMPTY_PADDING_PX: f32 = 8.0;
pub const V1_TABLE_HEADER_BACKGROUND: u32 = 0xf1f5f9;
pub const V1_TABLE_ACTIVE_BORDER: u32 = 0x60a5fa;

pub fn render_table_block(
    block_id: BlockId,
    table: &TablePayload,
    theme: GuiTheme,
    focused_cell: Option<TableCellPosition>,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
) -> AnyElement {
    if table.rows.is_empty() {
        return render_empty_table(theme);
    }

    div()
        .relative()
        .w_full()
        .rounded(px(V1_TABLE_RADIUS_PX))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .children(table.rows.iter().enumerate().map(|(row_index, row)| {
                    div()
                        .flex()
                        .bg(rgb(if is_header_row(table, row_index) {
                            theme.table_header_background
                        } else {
                            theme.surface
                        }))
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .w_full()
                        .children(row.cells.iter().enumerate().map(|(cell_index, cell)| {
                            render_table_cell(
                                table,
                                row_index,
                                cell_index,
                                render_inline_spans(&cell.spans, theme),
                                theme,
                                focused_cell,
                                view.clone(),
                                focus.clone(),
                                block_id,
                            )
                        }))
                })),
        )
        .into_any_element()
}

fn render_empty_table(theme: GuiTheme) -> AnyElement {
    div()
        .rounded(px(V1_TABLE_RADIUS_PX))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        .p(px(V1_TABLE_EMPTY_PADDING_PX))
        .text_color(rgb(theme.muted))
        .child("Empty table")
        .into_any_element()
}

fn render_table_cell(
    table: &TablePayload,
    row_index: usize,
    cell_index: usize,
    content: AnyElement,
    theme: GuiTheme,
    focused_cell: Option<TableCellPosition>,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    block_id: BlockId,
) -> AnyElement {
    let header = is_header_cell(table, row_index, cell_index);
    let active = is_active_cell(focused_cell, row_index, cell_index);
    div()
        .flex_1()
        .min_w(px(V1_TABLE_CELL_MIN_WIDTH_PX))
        .px(px(V1_TABLE_CELL_PADDING_X_PX))
        .py(px(V1_TABLE_CELL_PADDING_Y_PX))
        .border_r_1()
        .border_color(rgb(if active {
            theme.table_active_border
        } else {
            theme.border
        }))
        .bg(rgb(if active {
            theme.action_background
        } else if header {
            theme.table_header_background
        } else {
            theme.surface
        }))
        .cursor_text()
        .track_focus(&focus)
        .on_mouse_down(gpui::MouseButton::Left, move |event, window, cx| {
            focus_table_cell_from_mouse(&view, block_id, row_index, cell_index, event, window, cx);
            cx.stop_propagation();
        })
        .child(content)
        .into_any_element()
}

fn is_header_row(table: &TablePayload, row_index: usize) -> bool {
    row_index < table.header_rows.max(usize::from(table.header_rows == 0))
}

fn is_header_cell(table: &TablePayload, row_index: usize, cell_index: usize) -> bool {
    is_header_row(table, row_index) || cell_index < table.header_cols
}

fn is_active_cell(focused_cell: Option<TableCellPosition>, row: usize, col: usize) -> bool {
    focused_cell == Some(TableCellPosition { row, col })
}

#[cfg(test)]
mod tests {
    use crate::core::rich_text::{InlineSpan, TableCellPayload, TableRowPayload};

    use super::*;

    #[test]
    fn v1_table_geometry_constants_match_editor2() {
        assert_eq!(V1_TABLE_RADIUS_PX, 8.0);
        assert_eq!(V1_TABLE_CELL_MIN_WIDTH_PX, 120.0);
        assert_eq!(V1_TABLE_CELL_PADDING_X_PX, 10.0);
        assert_eq!(V1_TABLE_CELL_PADDING_Y_PX, 8.0);
        assert_eq!(V1_TABLE_HEADER_BACKGROUND, 0xf1f5f9);
        assert_eq!(V1_TABLE_ACTIVE_BORDER, 0x60a5fa);
    }

    #[test]
    fn table_header_detection_follows_payload_header_rows_and_cols() {
        let table = TablePayload {
            rows: vec![TableRowPayload {
                cells: vec![
                    TableCellPayload {
                        spans: vec![InlineSpan::plain("A")],
                    },
                    TableCellPayload {
                        spans: vec![InlineSpan::plain("B")],
                    },
                ],
            }],
            header_rows: 1,
            header_cols: 1,
        };

        assert!(is_header_cell(&table, 0, 0));
        assert!(is_header_cell(&table, 0, 1));
        assert!(is_header_cell(&table, 1, 0));
        assert!(!is_header_cell(&table, 1, 1));
    }

    #[test]
    fn table_active_cell_detection_follows_projection_position() {
        let focused = Some(TableCellPosition { row: 2, col: 1 });

        assert!(is_active_cell(focused, 2, 1));
        assert!(!is_active_cell(focused, 1, 1));
        assert!(!is_active_cell(None, 2, 1));
    }
}
