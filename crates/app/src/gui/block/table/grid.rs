use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use cditor_runtime::TableViewState;

use super::style::table_border_color;

const TABLE_GRID_LINE_WIDTH_PX: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct TableGridLine {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

pub(super) fn render_table_grid(table_view: &TableViewState, theme: GuiTheme) -> AnyElement {
    let color = table_border_color(theme);
    div()
        .absolute()
        .left_0()
        .top_0()
        .w(px(table_view.width_px))
        .h(px(table_view.height_px))
        .children(table_grid_lines(table_view).into_iter().map(move |line| {
            div()
                .absolute()
                .left(px(line.x))
                .top(px(line.y))
                .w(px(line.width))
                .h(px(line.height))
                .bg(rgb(color))
        }))
        .into_any_element()
}

fn table_grid_lines(table_view: &TableViewState) -> Vec<TableGridLine> {
    let mut lines = Vec::with_capacity(table_view.visible_cells.len() * 2 + 2);
    lines.push(TableGridLine {
        x: 0.0,
        y: 0.0,
        width: table_view.width_px,
        height: TABLE_GRID_LINE_WIDTH_PX,
    });
    lines.push(TableGridLine {
        x: 0.0,
        y: 0.0,
        width: TABLE_GRID_LINE_WIDTH_PX,
        height: table_view.height_px,
    });
    for cell in &table_view.visible_cells {
        lines.push(TableGridLine {
            x: trailing_grid_edge(cell.x_px + cell.width_px),
            y: cell.y_px,
            width: TABLE_GRID_LINE_WIDTH_PX,
            height: cell.height_px,
        });
        lines.push(TableGridLine {
            x: cell.x_px,
            y: trailing_grid_edge(cell.y_px + cell.height_px),
            width: cell.width_px,
            height: TABLE_GRID_LINE_WIDTH_PX,
        });
    }
    lines
}

fn trailing_grid_edge(edge_px: f32) -> f32 {
    (edge_px - TABLE_GRID_LINE_WIDTH_PX).max(0.0)
}

#[cfg(test)]
mod tests {
    use cditor_runtime::{TableCellPosition, TableVisibleCell};

    use super::*;

    #[test]
    fn projected_grid_and_active_border_share_the_same_outer_rect() {
        let table_view = table_view_with_two_cells();
        let lines = table_grid_lines(&table_view);

        assert!(lines.contains(&TableGridLine {
            x: 0.0,
            y: 0.0,
            width: 240.0,
            height: 1.0,
        }));
        assert!(lines.contains(&TableGridLine {
            x: 119.0,
            y: 0.0,
            width: 1.0,
            height: 36.0,
        }));
        assert!(lines.contains(&TableGridLine {
            x: 0.0,
            y: 35.0,
            width: 120.0,
            height: 1.0,
        }));
        assert!(lines.contains(&TableGridLine {
            x: 239.0,
            y: 0.0,
            width: 1.0,
            height: 36.0,
        }));
    }

    #[test]
    fn merged_cell_does_not_reintroduce_covered_grid_lines() {
        let mut table_view = table_view_with_two_cells();
        table_view.visible_cells = vec![TableVisibleCell {
            position: TableCellPosition { row: 0, col: 0 },
            row_span: 1,
            col_span: 2,
            x_px: 0.0,
            y_px: 0.0,
            width_px: 240.0,
            height_px: 36.0,
            header: false,
            align: cditor_core::rich_text::TableCellAlign::Left,
            background_color: None,
            spans: Vec::new(),
        }];

        let lines = table_grid_lines(&table_view);

        assert!(!lines.iter().any(|line| line.x == 119.0));
        assert!(lines.iter().any(|line| line.x == 239.0));
    }

    fn table_view_with_two_cells() -> TableViewState {
        TableViewState {
            table: Default::default(),
            row_count: 1,
            col_count: 2,
            width_px: 240.0,
            height_px: 36.0,
            column_widths_px: vec![120.0, 120.0],
            row_heights_px: vec![36.0],
            horizontal_scroll_offset_px: 0.0,
            visible_cells: vec![visible_cell(0, 0, 0.0), visible_cell(0, 1, 120.0)],
            focused_cell: None,
            focused_cell_offset: None,
            focused_cell_selection_range: None,
        }
    }

    fn visible_cell(row: usize, col: usize, x_px: f32) -> TableVisibleCell {
        TableVisibleCell {
            position: TableCellPosition { row, col },
            row_span: 1,
            col_span: 1,
            x_px,
            y_px: 0.0,
            width_px: 120.0,
            height_px: 36.0,
            header: false,
            align: cditor_core::rich_text::TableCellAlign::Left,
            background_color: None,
            spans: Vec::new(),
        }
    }
}
