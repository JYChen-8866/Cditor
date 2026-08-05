//! Table parsing for Markdown import.
//!
//! Candidate detection and cell splitting are delegated to the vendored
//! velotype table parser so GFM tables (including pipeless tables, escaped
//! pipes, alignment colons and short/long body rows) behave identically.

use super::*;
use super::velotype::table as velotype_table;
use super::velotype_bridge::table_data_to_payload;

pub(super) fn is_table_candidate_line(line: &str) -> bool {
    velotype_table::is_table_candidate_line(line)
}

pub(super) fn collect_table_candidate_region(lines: &[&str], start: usize) -> usize {
    let owned = lines.iter().map(|line| (*line).to_owned()).collect::<Vec<_>>();
    velotype_table::collect_table_candidate_region(&owned, start)
}

pub(super) fn parse_table_region(lines: &[&str]) -> Option<TablePayload> {
    let owned = lines.iter().map(|line| (*line).to_owned()).collect::<Vec<_>>();
    let table = velotype_table::parse_table_region(&owned)?;
    Some(table_data_to_payload(&table))
}

pub(super) fn table_to_plain_markdown(payload: &BlockPayload) -> Option<String> {
    let BlockPayload::Table(table) = payload else {
        return None;
    };
    let first = table.rows.first()?;
    let columns = first.cells.len();
    if columns == 0 {
        return None;
    }
    let mut lines = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let cells = row
            .cells
            .iter()
            .map(|cell| {
                escape_table_cell(&cditor_core::rich_text::plain_text_from_spans(&cell.spans))
            })
            .collect::<Vec<_>>();
        lines.push(format!("| {} |", cells.join(" | ")));
        if row_index == 0 {
            lines.push(format!("| {} |", vec!["---"; columns].join(" | ")));
        }
    }
    Some(lines.join("\n"))
}

fn escape_table_cell(cell: &str) -> String {
    cell.replace('|', "\\|")
}
