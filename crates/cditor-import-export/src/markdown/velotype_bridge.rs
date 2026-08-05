//! Bridge between the vendored velotype Markdown parsers and Cditor's rich
//! text model (`InlineSpan`/`InlineMark` and `TablePayload`).
//!
//! velotype stores inline formatting as attribute fragments (`InlineFragment`)
//! while Cditor stores flat spans with mark enums. The conversion is lossless
//! for the marks Cditor supports; inline math and footnote references keep
//! their raw Markdown source as plain text so round-trips stay faithful.

use cditor_core::rich_text::{
    InlineMark, InlineSpan, TableCellAlign, TableCellMerge, TableCellPayload, TableColumnPayload,
    TablePayload, TableRowPayload,
};

use super::velotype::html::HtmlCssColor;
use super::velotype::inline::{InlineFragment, InlineLink, InlineTextTree};
use super::velotype::table::{TableColumnAlignment, TableData};

/// Parses inline Markdown with the vendored velotype parser and converts the
/// resulting fragment tree into Cditor spans.
pub(super) fn parse_inline_markdown_velotype(markdown: &str) -> Vec<InlineSpan> {
    let tree = InlineTextTree::from_markdown(markdown);
    inline_tree_to_spans(&tree)
}

/// Converts a velotype inline fragment tree into Cditor spans.
pub(super) fn inline_tree_to_spans(tree: &InlineTextTree) -> Vec<InlineSpan> {
    let mut spans: Vec<InlineSpan> = Vec::new();
    for fragment in &tree.fragments {
        let span = InlineSpan {
            text: fragment.text.clone(),
            marks: marks_for_fragment(fragment),
        };
        merge_span(&mut spans, span);
    }
    if spans.is_empty() {
        spans.push(InlineSpan::plain(String::new()));
    }
    spans
}

fn marks_for_fragment(fragment: &InlineFragment) -> Vec<InlineMark> {
    let mut marks = Vec::new();
    if fragment.style.bold {
        marks.push(InlineMark::Bold);
    }
    if fragment.style.italic {
        marks.push(InlineMark::Italic);
    }
    if fragment.style.underline {
        marks.push(InlineMark::Underline);
    }
    if fragment.style.strikethrough {
        marks.push(InlineMark::Strike);
    }
    if fragment.style.code {
        marks.push(InlineMark::Code);
    }
    if let Some(html_style) = &fragment.html_style {
        if let Some(color) = html_style.color {
            marks.push(InlineMark::Color(css_color_hex(color)));
        }
        if let Some(background) = html_style.background_color {
            marks.push(InlineMark::Background(css_color_hex(background)));
        }
    }
    if let Some(link) = &fragment.link {
        marks.push(InlineMark::Link {
            href: link_open_target(link).to_owned(),
        });
    }
    marks
}

fn link_open_target(link: &InlineLink) -> &str {
    match link {
        InlineLink::Inline { destination, .. } | InlineLink::Reference { destination, .. } => {
            destination
        }
        InlineLink::Autolink { target } => target,
    }
}

/// Serializes a parsed CSS color to Cditor's `#rrggbb[aa]` convention.
fn css_color_hex(color: HtmlCssColor) -> String {
    let HtmlCssColor::Rgba(color) = color else {
        return "currentColor".to_owned();
    };
    if color.alpha < 0.999 {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.red,
            color.green,
            color.blue,
            (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8
        )
    } else {
        format!("#{:02x}{:02x}{:02x}", color.red, color.green, color.blue)
    }
}

fn merge_span(spans: &mut Vec<InlineSpan>, span: InlineSpan) {
    if span.text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.marks == span.marks
    {
        last.text.push_str(&span.text);
        return;
    }
    spans.push(span);
}

/// Converts a velotype `TableData` into Cditor's `TablePayload`.
pub(super) fn table_data_to_payload(table: &TableData) -> TablePayload {
    let columns = table.column_count();
    let mut rows = Vec::with_capacity(1 + table.rows.len());

    let header = TableRowPayload {
        cells: table
            .header
            .iter()
            .map(|tree| TableCellPayload {
                spans: inline_tree_to_spans(tree),
                ..Default::default()
            })
            .collect(),
        height: Default::default(),
    };
    rows.push(header);

    for row in &table.rows {
        let cells = row
            .iter()
            .enumerate()
            .map(|(index, tree)| TableCellPayload {
                spans: inline_tree_to_spans(tree),
                align: table
                    .alignments
                    .get(index)
                    .copied()
                    .map(alignment_to_cell)
                    .unwrap_or_default(),
                ..Default::default()
            })
            .collect();
        rows.push(TableRowPayload {
            cells,
            height: Default::default(),
        });
    }

    TablePayload {
        rows,
        columns: (0..columns)
            .map(|_| TableColumnPayload::default())
            .collect(),
        header_rows: 1,
        header_cols: 0,
        header_style: Default::default(),
    }
}

fn alignment_to_cell(alignment: TableColumnAlignment) -> TableCellAlign {
    match alignment {
        TableColumnAlignment::Default | TableColumnAlignment::Left => TableCellAlign::Left,
        TableColumnAlignment::Center => TableCellAlign::Center,
        TableColumnAlignment::Right => TableCellAlign::Right,
    }
}
