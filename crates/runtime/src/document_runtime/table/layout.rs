use cditor_core::layout::{
    COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX, NOTION_TABLE_CELL_LINE_HEIGHT_PX,
    NOTION_TABLE_CELL_PADDING_Y_PX, NOTION_TABLE_DEFAULT_ROW_HEIGHT_PX,
    TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX,
};
use cditor_core::rich_text::{InlineSpan, TableCellMerge, TablePayload, TableTrackSize};

const DEFAULT_TABLE_COLUMN_WIDTH_PX: f32 = 120.0;
const DEFAULT_TABLE_ROW_HEIGHT_PX: f32 = NOTION_TABLE_DEFAULT_ROW_HEIGHT_PX as f32;
const TABLE_CELL_PADDING_Y_PX: f32 = NOTION_TABLE_CELL_PADDING_Y_PX as f32;
const TABLE_CELL_PADDING_X_PX: f32 = 10.0;
const TABLE_CELL_LINE_HEIGHT_PX: f32 = NOTION_TABLE_CELL_LINE_HEIGHT_PX as f32;
const TABLE_CELL_APPROX_ASCII_CHAR_WIDTH_PX: f32 = 7.0;
const TABLE_CELL_APPROX_CJK_CHAR_WIDTH_PX: f32 = 14.0;
const TABLE_CELL_APPROX_NON_ASCII_CHAR_WIDTH_PX: f32 = 11.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::document_runtime) struct TableLayoutMetrics {
    pub default_column_width_px: f32,
    pub default_row_height_px: f32,
    pub cell_padding_x_px: f32,
    pub cell_padding_y_px: f32,
    pub cell_line_height_px: f32,
    pub approx_ascii_char_width_px: f32,
    pub approx_cjk_char_width_px: f32,
    pub approx_non_ascii_char_width_px: f32,
}

impl Default for TableLayoutMetrics {
    fn default() -> Self {
        Self {
            default_column_width_px: DEFAULT_TABLE_COLUMN_WIDTH_PX,
            default_row_height_px: DEFAULT_TABLE_ROW_HEIGHT_PX,
            cell_padding_x_px: TABLE_CELL_PADDING_X_PX,
            cell_padding_y_px: TABLE_CELL_PADDING_Y_PX,
            cell_line_height_px: TABLE_CELL_LINE_HEIGHT_PX,
            approx_ascii_char_width_px: TABLE_CELL_APPROX_ASCII_CHAR_WIDTH_PX,
            approx_cjk_char_width_px: TABLE_CELL_APPROX_CJK_CHAR_WIDTH_PX,
            approx_non_ascii_char_width_px: TABLE_CELL_APPROX_NON_ASCII_CHAR_WIDTH_PX,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::document_runtime) struct TableLayoutInput<'a> {
    pub table: &'a TablePayload,
    pub available_width_px: Option<f32>,
    pub metrics: TableLayoutMetrics,
}

impl<'a> TableLayoutInput<'a> {
    pub(in crate::document_runtime) fn new(table: &'a TablePayload) -> Self {
        Self {
            table,
            available_width_px: None,
            metrics: TableLayoutMetrics::default(),
        }
    }
}

pub(in crate::document_runtime) struct TableLayout {
    pub row_count: usize,
    pub col_count: usize,
    pub column_widths: Vec<f32>,
    pub row_heights: Vec<f32>,
    pub x_offsets: Vec<f32>,
    pub y_offsets: Vec<f32>,
    pub width_px: f32,
    pub height_px: f32,
}

pub(in crate::document_runtime) fn table_layout_from_payload(table: &TablePayload) -> TableLayout {
    table_layout_from_input(TableLayoutInput::new(table))
}

pub(in crate::document_runtime) fn table_layout_from_input(
    input: TableLayoutInput<'_>,
) -> TableLayout {
    let table = input.table;
    let metrics = input.metrics;
    let row_count = table.row_count();
    let col_count = table.column_count();
    let column_widths = (0..col_count)
        .map(|col| {
            table
                .columns
                .get(col)
                .map(|column| track_size_px(column.width, metrics.default_column_width_px))
                .unwrap_or(metrics.default_column_width_px)
        })
        .collect::<Vec<_>>();
    let mut row_heights = (0..row_count)
        .map(|row| {
            table
                .rows
                .get(row)
                .map(|row| track_size_px(row.height, metrics.default_row_height_px))
                .unwrap_or(metrics.default_row_height_px)
        })
        .collect::<Vec<_>>();
    let column_widths =
        distribute_extra_width_to_auto_columns(table, column_widths, input.available_width_px);
    grow_auto_table_rows_for_content(table, &column_widths, &mut row_heights, metrics);

    let x_offsets = prefix_offsets(&column_widths);
    let y_offsets = prefix_offsets(&row_heights);
    TableLayout {
        row_count,
        col_count,
        width_px: column_widths.iter().sum(),
        height_px: row_heights.iter().sum(),
        column_widths,
        row_heights,
        x_offsets,
        y_offsets,
    }
}

pub(in crate::document_runtime) fn table_payload_projected_height_px(table: &TablePayload) -> f32 {
    table_layout_from_payload(table).height_px
        + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX as f32
        + TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX as f32
}

fn grow_auto_table_rows_for_content(
    table: &TablePayload,
    column_widths: &[f32],
    row_heights: &mut [f32],
    metrics: TableLayoutMetrics,
) {
    for (row, col, cell) in table.visible_cells() {
        let (row_span, col_span) = match cell.merge {
            TableCellMerge::Origin { row_span, col_span } => (row_span.max(1), col_span.max(1)),
            TableCellMerge::Unmerged | TableCellMerge::Covered { .. } => (1, 1),
        };
        let width = span_size(column_widths, col, col_span);
        let required_height = table_cell_auto_content_height_px(&cell.spans, width, metrics);
        let current_height = span_size(row_heights, row, row_span);
        if required_height <= current_height {
            continue;
        }

        let auto_rows = (row..row.saturating_add(row_span))
            .filter(|row_index| {
                table_row_is_auto(table, *row_index) && *row_index < row_heights.len()
            })
            .collect::<Vec<_>>();
        if auto_rows.is_empty() {
            continue;
        }

        let extra_per_row = (required_height - current_height) / auto_rows.len() as f32;
        for row_index in auto_rows {
            row_heights[row_index] += extra_per_row;
        }
    }
}

fn table_row_is_auto(table: &TablePayload, row: usize) -> bool {
    table
        .rows
        .get(row)
        .is_none_or(|row| matches!(row.height, TableTrackSize::Auto))
}

fn table_cell_auto_content_height_px(
    spans: &[InlineSpan],
    cell_width_px: f32,
    metrics: TableLayoutMetrics,
) -> f32 {
    let text = cditor_core::rich_text::plain_text_from_spans(spans);
    let content_width =
        (cell_width_px - metrics.cell_padding_x_px * 2.0).max(metrics.approx_ascii_char_width_px);
    let line_count = text
        .split('\n')
        .map(|line| table_cell_wrapped_line_count(line, content_width, metrics))
        .sum::<usize>()
        .max(1);
    (line_count as f32 * metrics.cell_line_height_px + metrics.cell_padding_y_px * 2.0)
        .max(metrics.default_row_height_px)
}

fn table_cell_wrapped_line_count(
    text: &str,
    content_width_px: f32,
    metrics: TableLayoutMetrics,
) -> usize {
    if text.is_empty() {
        return 1;
    }
    let mut lines = 1usize;
    let mut current_width = 0.0f32;
    for ch in text.chars() {
        let char_width = table_cell_approx_char_width_px(ch, metrics);
        if current_width > 0.0 && current_width + char_width > content_width_px {
            lines += 1;
            current_width = char_width;
        } else {
            current_width += char_width;
        }
    }
    lines
}

fn table_cell_approx_char_width_px(ch: char, metrics: TableLayoutMetrics) -> f32 {
    if ch.is_ascii() {
        metrics.approx_ascii_char_width_px
    } else if matches!(
        ch as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF | 0xAC00..=0xD7AF
    ) {
        metrics.approx_cjk_char_width_px
    } else {
        metrics.approx_non_ascii_char_width_px
    }
}

fn distribute_extra_width_to_auto_columns(
    table: &TablePayload,
    mut column_widths: Vec<f32>,
    available_width_px: Option<f32>,
) -> Vec<f32> {
    let Some(available_width_px) = available_width_px else {
        return column_widths;
    };
    let current_width = column_widths.iter().sum::<f32>();
    if available_width_px <= current_width {
        return column_widths;
    }
    let auto_columns = (0..column_widths.len())
        .filter(|col| {
            table
                .columns
                .get(*col)
                .is_none_or(|column| matches!(column.width, TableTrackSize::Auto))
        })
        .collect::<Vec<_>>();
    if auto_columns.is_empty() {
        return column_widths;
    }
    let extra_per_column = (available_width_px - current_width) / auto_columns.len() as f32;
    for col in auto_columns {
        column_widths[col] += extra_per_column;
    }
    column_widths
}

fn track_size_px(size: TableTrackSize, fallback: f32) -> f32 {
    match size {
        TableTrackSize::Auto => fallback,
        TableTrackSize::Px(px) => f32::from(px).max(1.0),
    }
}

fn prefix_offsets(sizes: &[f32]) -> Vec<f32> {
    let mut next = 0.0;
    sizes
        .iter()
        .map(|size| {
            let current = next;
            next += *size;
            current
        })
        .collect()
}

pub(in crate::document_runtime) fn span_size(sizes: &[f32], start: usize, span: usize) -> f32 {
    sizes.iter().skip(start).take(span.max(1)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::{
        InlineSpan, TableCellPayload, TableColumnPayload, TablePayload, TableRowPayload,
    };

    fn table_with_text(text: &str) -> TablePayload {
        let mut table = TablePayload {
            rows: vec![TableRowPayload {
                cells: vec![TableCellPayload {
                    spans: vec![InlineSpan::plain(text)],
                    ..TableCellPayload::default()
                }],
                height: TableTrackSize::Auto,
            }],
            columns: Vec::new(),
            header_rows: 0,
            header_cols: 0,
            header_style: Default::default(),
        };
        table.normalize();
        table
    }

    #[test]
    fn table_layout_input_keeps_legacy_defaults_without_available_width() {
        let table = table_with_text("abc");

        let layout = table_layout_from_input(TableLayoutInput::new(&table));

        assert_eq!(layout.column_widths, vec![DEFAULT_TABLE_COLUMN_WIDTH_PX]);
        assert_eq!(layout.row_heights, vec![DEFAULT_TABLE_ROW_HEIGHT_PX]);
        assert_eq!(layout.width_px, DEFAULT_TABLE_COLUMN_WIDTH_PX);
        assert_eq!(
            table_payload_projected_height_px(&table),
            DEFAULT_TABLE_ROW_HEIGHT_PX
                + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX as f32
                + TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX as f32
        );
    }

    #[test]
    fn table_layout_input_distributes_extra_available_width_to_auto_columns() {
        let mut table = table_with_text("abc");
        table.rows[0].cells.push(TableCellPayload::plain("def"));
        table.columns = vec![
            TableColumnPayload {
                width: TableTrackSize::Px(160),
            },
            TableColumnPayload {
                width: TableTrackSize::Auto,
            },
        ];
        table.normalize();

        let layout = table_layout_from_input(TableLayoutInput {
            table: &table,
            available_width_px: Some(400.0),
            metrics: TableLayoutMetrics::default(),
        });

        assert_eq!(layout.column_widths, vec![160.0, 240.0]);
        assert_eq!(layout.width_px, 400.0);
    }

    #[test]
    fn table_layout_input_metrics_control_wrapping_height() {
        let table = table_with_text("abcdefghijklmnop");

        let compact = table_layout_from_input(TableLayoutInput {
            table: &table,
            available_width_px: None,
            metrics: TableLayoutMetrics::default(),
        });
        let narrow_text = table_layout_from_input(TableLayoutInput {
            table: &table,
            available_width_px: None,
            metrics: TableLayoutMetrics {
                approx_ascii_char_width_px: 32.0,
                ..TableLayoutMetrics::default()
            },
        });

        assert!(narrow_text.height_px > compact.height_px);
    }

    #[test]
    fn multiline_rows_use_the_same_line_height_and_padding_as_the_gui() {
        let table = table_with_text("first\nsecond\nthird");

        let layout = table_layout_from_payload(&table);
        let expected = 3.0 * NOTION_TABLE_CELL_LINE_HEIGHT_PX as f32
            + 2.0 * NOTION_TABLE_CELL_PADDING_Y_PX as f32;

        assert!((layout.row_heights[0] - expected).abs() < 0.001);
        assert!(layout.row_heights[0] > DEFAULT_TABLE_ROW_HEIGHT_PX);
    }
}
