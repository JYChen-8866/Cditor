//! Native Markdown table data model plus parse and serialize helpers.
//!
//! VENDORED from velotype (`src/components/markdown/table.rs`) with the
//! gpui render/runtime half removed; only the pure parse and serialize
//! functions are kept so Cditor imports tables exactly the way velotype does.

use super::inline::InlineTextTree;

/// Horizontal alignment declared by the table's delimiter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableColumnAlignment {
    /// No explicit alignment marker (`---`). Renders left, but stays distinct
    /// from [`Left`](Self::Left) so an unmarked column is not silently rewritten
    /// with a leading colon on the next serialize.
    Default,
    /// Explicit left alignment (`:---`).
    Left,
    /// Center-aligned cells (`:---:`).
    Center,
    /// Right-aligned cells (`---:`).
    Right,
}
/// Persistent cell contents for a native table block.
#[derive(Debug, Clone)]
pub struct TableData {
    pub header: Vec<InlineTextTree>,
    pub rows: Vec<Vec<InlineTextTree>>,
    pub alignments: Vec<TableColumnAlignment>,
}

impl PartialEq for TableData {
    fn eq(&self, other: &Self) -> bool {
        self.header == other.header
            && self.rows == other.rows
            && self.alignments == other.alignments
    }
}

impl Eq for TableData {}

impl TableData {
    /// Creates an empty table with one header row, `body_rows` body rows, and
    /// `columns` left-aligned columns.
    pub fn new_empty(body_rows: usize, columns: usize) -> Self {
        let columns = columns.max(1);
        let header = (0..columns)
            .map(|_| InlineTextTree::plain(String::new()))
            .collect::<Vec<_>>();
        let rows = (0..body_rows.max(1))
            .map(|_| {
                (0..columns)
                    .map(|_| InlineTextTree::plain(String::new()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let alignments = vec![TableColumnAlignment::Default; columns];
        Self {
            header,
            rows,
            alignments,
        }
    }

    pub(crate) fn column_count(&self) -> usize {
        self.header
            .len()
            .max(self.alignments.len())
            .max(self.rows.iter().map(Vec::len).max().unwrap_or(0))
            .max(1)
    }
}

fn strip_table_indent(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|b| *b == b' ').count();
    (indent <= 3).then_some(&line[indent..])
}

fn split_table_cells(line: &str) -> Option<Vec<String>> {
    let rest = strip_table_indent(line)?.trim_end();
    if rest.is_empty() {
        return None;
    }
    // Outer pipes are optional (GFM): strip them when present so pipeless rows
    // like `Name | Score` split the same way as `| Name | Score |`.
    let inner = rest.strip_prefix('|').unwrap_or(rest);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut escaping = false;

    for ch in inner.chars() {
        if escaping {
            match ch {
                '|' | '\\' => current.push(ch),
                _ => {
                    current.push('\\');
                    current.push(ch);
                }
            }
            escaping = false;
            continue;
        }

        match ch {
            '\\' => escaping = true,
            '|' => {
                cells.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if escaping {
        current.push('\\');
    }
    cells.push(current.trim().to_string());
    Some(cells)
}

fn parse_alignment_cell(cell: &str) -> Option<TableColumnAlignment> {
    let trimmed = cell.trim();
    if trimmed.len() < 3 {
        return None;
    }

    let left = trimmed.starts_with(':');
    let right = trimmed.ends_with(':');
    let core = trimmed.trim_start_matches(':').trim_end_matches(':');
    if core.len() < 3 || !core.chars().all(|ch| ch == '-') {
        return None;
    }

    Some(match (left, right) {
        (true, true) => TableColumnAlignment::Center,
        (false, true) => TableColumnAlignment::Right,
        (true, false) => TableColumnAlignment::Left,
        (false, false) => TableColumnAlignment::Default,
    })
}

fn serialize_alignment(alignment: TableColumnAlignment) -> &'static str {
    match alignment {
        TableColumnAlignment::Default => "---",
        TableColumnAlignment::Left => ":---",
        TableColumnAlignment::Center => ":---:",
        TableColumnAlignment::Right => "---:",
    }
}

pub(crate) fn serialize_table_cell_markdown(tree: &InlineTextTree) -> String {
    tree.serialize_markdown()
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
}

fn serialize_row<'a>(cells: impl IntoIterator<Item = &'a InlineTextTree>) -> String {
    let rendered = cells
        .into_iter()
        .map(serialize_table_cell_markdown)
        .collect::<Vec<_>>();
    format!("| {} |", rendered.join(" | "))
}

/// Returns true when a line is a candidate native table row in the current
/// container scope.
pub fn is_table_candidate_line(line: &str) -> bool {
    strip_table_indent(line)
        .map(str::trim_end)
        .is_some_and(|rest| rest.starts_with('|'))
}

/// Number of pipe-separated cells in `line`, treating outer pipes as optional
/// (GFM) so pipeless rows like `Name | Score` are recognized. Returns `None`
/// for single-column lines so prose containing a stray `|` is not mistaken for
/// a table row.
pub fn table_row_column_count(line: &str) -> Option<usize> {
    split_table_cells(line)
        .map(|cells| cells.len())
        .filter(|count| *count >= 2)
}

/// True when `line` could be a table row, including a pipeless GFM row.
pub fn is_table_row_candidate(line: &str) -> bool {
    table_row_column_count(line).is_some()
}

/// Collects a contiguous table-candidate region in the current container
/// scope.
pub fn collect_table_candidate_region(lines: &[String], start: usize) -> usize {
    let mut index = start + 1;
    while index < lines.len() && is_table_candidate_line(&lines[index]) {
        index += 1;
    }
    index
}

/// Parses a pipe-table region into native table data.
pub fn parse_table_region(lines: &[String]) -> Option<TableData> {
    if lines.len() < 2 {
        return None;
    }

    let header = split_table_cells(&lines[0])?;
    let alignment_cells = split_table_cells(&lines[1])?;
    if header.is_empty() || alignment_cells.len() != header.len() {
        return None;
    }

    let alignments = alignment_cells
        .iter()
        .map(|cell| parse_alignment_cell(cell))
        .collect::<Option<Vec<_>>>()?;

    let mut rows = Vec::new();
    for line in &lines[2..] {
        // GFM normalizes body rows to the header width: short rows are padded
        // with empty cells and long rows drop their trailing cells, instead of
        // invalidating the whole table.
        let mut cells = split_table_cells(line)?;
        cells.resize(header.len(), String::new());
        rows.push(
            cells
                .into_iter()
                .map(|cell| InlineTextTree::from_markdown(&cell))
                .collect::<Vec<_>>(),
        );
    }

    Some(TableData {
        header: header
            .into_iter()
            .map(|cell| InlineTextTree::from_markdown(&cell))
            .collect(),
        rows,
        alignments,
    })
}

/// Returns true when `line` is a delimiter row of exactly `columns` cells, each
/// a valid alignment specifier.
fn is_delimiter_row(line: &str, columns: usize) -> bool {
    split_table_cells(line).is_some_and(|cells| {
        cells.len() == columns
            && cells
                .iter()
                .all(|cell| parse_alignment_cell(cell).is_some())
    })
}

/// Detects a table that starts at `start` without requiring outer pipes,
/// returning the region end (exclusive) when `lines[start]` is a multi-column
/// header followed by a matching delimiter row. Body rows extend to the next
/// blank line, matching GFM. Returns `None` for ordinary prose so a stray `|`
/// is never mistaken for a table; single-column pipeless candidates are also
/// rejected because they are ambiguous with setext headings.
pub fn collect_pipeless_table_region(lines: &[String], start: usize) -> Option<usize> {
    let header = split_table_cells(lines.get(start)?)?;
    if header.len() < 2 {
        return None;
    }
    if !is_delimiter_row(lines.get(start + 1)?, header.len()) {
        return None;
    }

    let mut end = start + 2;
    while end < lines.len() && !lines[end].trim().is_empty() {
        end += 1;
    }
    Some(end)
}

/// Returns true when a root-level line is a candidate native table row.
pub fn is_root_table_candidate_line(line: &str) -> bool {
    is_table_candidate_line(line)
}

/// Collects a contiguous root-level table candidate region.
pub fn collect_root_table_candidate_region(lines: &[String], start: usize) -> usize {
    collect_table_candidate_region(lines, start)
}

/// Parses a root-level pipe table region into native table data.
pub fn parse_root_table_region(lines: &[String]) -> Option<TableData> {
    parse_table_region(lines)
}

/// Parses a single table body row, normalized to `columns` cells (padded when
/// short, truncated when long). Returns `None` when the line is not a table
/// row at all.
pub fn parse_table_body_row(line: &str, columns: usize) -> Option<Vec<InlineTextTree>> {
    let mut cells = split_table_cells(line)?;
    cells.resize(columns, String::new());
    Some(
        cells
            .into_iter()
            .map(|cell| InlineTextTree::from_markdown(&cell))
            .collect(),
    )
}

/// Serializes native table data to canonical pipe-table Markdown lines.
pub fn serialize_table_markdown_lines(table: &TableData) -> Vec<String> {
    let mut lines = Vec::with_capacity(2 + table.rows.len());
    lines.push(serialize_row(table.header.iter()));
    lines.push(format!(
        "| {} |",
        table
            .alignments
            .iter()
            .map(|alignment| serialize_alignment(*alignment))
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    lines.extend(table.rows.iter().map(|row| serialize_row(row.iter())));
    lines
}

#[cfg(test)]
mod tests {
    use super::{
        TableColumnAlignment, TableData, collect_pipeless_table_region,
        collect_root_table_candidate_region, is_root_table_candidate_line, parse_root_table_region,
        serialize_table_markdown_lines,
    };
    use super::super::inline::InlineTextTree;

    #[test]
    fn parses_valid_root_table_region() {
        let lines = vec![
            "| Left | Center | Right |".to_string(),
            "| :--- | :---: | ---: |".to_string(),
            "| a | b | c |".to_string(),
        ];
        let table = parse_root_table_region(&lines).expect("table should parse");
        assert_eq!(table.alignments.len(), 3);
        assert_eq!(
            table.alignments,
            vec![
                TableColumnAlignment::Left,
                TableColumnAlignment::Center,
                TableColumnAlignment::Right
            ]
        );
        assert_eq!(table.header[0].serialize_markdown(), "Left");
        assert_eq!(table.rows[0][2].serialize_markdown(), "c");
    }

    #[test]
    fn rejects_invalid_alignment_row() {
        let lines = vec!["| Left | Right |".to_string(), "| nope | --- |".to_string()];
        assert!(parse_root_table_region(&lines).is_none());
    }

    #[test]
    fn rejects_alignment_row_with_wrong_column_count() {
        let lines = vec!["| A | B | C |".to_string(), "| --- | --- |".to_string()];
        assert!(parse_root_table_region(&lines).is_none());
    }

    #[test]
    fn preserves_explicit_left_alignment_colon() {
        // ":---" is explicit left and must survive a parse/serialize round-trip
        // instead of being silently rewritten to a bare "---".
        let lines = vec![
            "| L | D | R |".to_string(),
            "| :--- | --- | ---: |".to_string(),
            "| a | b | c |".to_string(),
        ];
        let table = parse_root_table_region(&lines).expect("table should parse");
        assert_eq!(
            table.alignments,
            vec![
                TableColumnAlignment::Left,
                TableColumnAlignment::Default,
                TableColumnAlignment::Right
            ]
        );
        assert_eq!(
            serialize_table_markdown_lines(&table)[1],
            "| :--- | --- | ---: |"
        );
    }

    #[test]
    fn pads_short_body_rows_and_truncates_long_ones() {
        let lines = vec![
            "| A | B | C |".to_string(),
            "| --- | --- | --- |".to_string(),
            "| short |".to_string(),
            "| 1 | 2 | 3 | 4 |".to_string(),
        ];
        let table = parse_root_table_region(&lines).expect("table should parse");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].len(), 3);
        assert_eq!(table.rows[0][0].serialize_markdown(), "short");
        assert!(table.rows[0][1].serialize_markdown().is_empty());
        assert!(table.rows[0][2].serialize_markdown().is_empty());
        assert_eq!(table.rows[1].len(), 3);
        assert_eq!(table.rows[1][2].serialize_markdown(), "3");
    }

    #[test]
    fn parses_pipeless_table() {
        let lines = vec![
            "Name | Score".to_string(),
            "--- | ---".to_string(),
            "Alice | 10".to_string(),
            "Bob | 7".to_string(),
        ];
        let end = collect_pipeless_table_region(&lines, 0).expect("region");
        assert_eq!(end, 4);
        let table = parse_root_table_region(&lines[..end]).expect("table should parse");
        assert_eq!(table.header.len(), 2);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.header[0].serialize_markdown(), "Name");
        assert_eq!(table.rows[1][1].serialize_markdown(), "7");
    }

    #[test]
    fn prose_with_pipe_is_not_a_pipeless_table() {
        let lines = vec!["this | that".to_string(), "and the next line".to_string()];
        assert!(collect_pipeless_table_region(&lines, 0).is_none());
    }

    #[test]
    fn pipeless_table_requires_valid_delimiter_row() {
        let lines = vec!["Name | Score".to_string(), "Alice | 10".to_string()];
        assert!(collect_pipeless_table_region(&lines, 0).is_none());
    }

    #[test]
    fn single_column_pipeless_is_not_a_table() {
        // Ambiguous with a setext heading; must not be captured as a table.
        let lines = vec!["Title".to_string(), "---".to_string()];
        assert!(collect_pipeless_table_region(&lines, 0).is_none());
    }

    #[test]
    fn serializes_canonical_pipe_table() {
        let table = TableData {
            header: vec![
                InlineTextTree::from_markdown("**bold**"),
                InlineTextTree::from_markdown("[link](https://example.com)"),
            ],
            rows: vec![vec![
                InlineTextTree::plain("A | B".to_string()),
                InlineTextTree::plain("value".to_string()),
            ]],
            alignments: vec![TableColumnAlignment::Default, TableColumnAlignment::Right],
        };
        assert_eq!(
            serialize_table_markdown_lines(&table),
            vec![
                "| **bold** | [link](https://example.com) |".to_string(),
                "| --- | ---: |".to_string(),
                "| A \\| B | value |".to_string(),
            ]
        );
    }

    #[test]
    fn detects_root_table_candidate_runs() {
        let lines = vec![
            "| A | B |".to_string(),
            "| --- | --- |".to_string(),
            "| 1 | 2 |".to_string(),
            "paragraph".to_string(),
        ];
        assert!(is_root_table_candidate_line(&lines[0]));
        assert_eq!(collect_root_table_candidate_region(&lines, 0), 3);
    }

}
