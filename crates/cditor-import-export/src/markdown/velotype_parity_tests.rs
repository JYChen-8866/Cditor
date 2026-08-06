//! Parity tests for the vendored velotype Markdown parser integration.
//!
//! These document behaviors that now match velotype exactly: CommonMark-style
//! delimiter flanking, nested emphasis, inline HTML style spans mapping to
//! color marks, and GFM table row padding/truncation.

use super::*;

#[test]
fn intraword_underscores_stay_literal_like_velotype() {
    // Mirrors velotype exactly: a lone intraword underscore stays literal,
    // while a second underscore later in the line acts as a closing marker.
    for text in ["a_b", "foo_bar"] {
        let spans = import_markdown_inline_incremental(text).expect("inline parse");
        assert_eq!(spans.len(), 1, "text {text:?}");
        assert_eq!(spans[0].text, text);
        assert!(spans[0].marks.is_empty());
    }
    let spans = import_markdown_inline_incremental("README_a.md 与 foo_bar 保持原样")
        .expect("inline parse");
    assert_eq!(
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<Vec<_>>(),
        vec!["README", "a.md 与 foo", "bar 保持原样"]
    );
    assert_eq!(spans[1].marks, vec![InlineMark::Italic]);
    assert!(!markdown_inline_shortcut_spans("README_a.md").is_some());
}

#[test]
fn nested_emphasis_and_adjacent_marks_match_velotype() {
    let spans = import_markdown_inline_incremental("**bold *italic* bold**").expect("inline parse");
    assert_eq!(
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<Vec<_>>(),
        vec!["bold ", "italic", " bold"]
    );
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
    assert!(spans[1].marks.contains(&InlineMark::Bold));
    assert!(spans[1].marks.contains(&InlineMark::Italic));
    assert_eq!(spans[2].marks, vec![InlineMark::Bold]);
}

#[test]
fn inline_html_style_maps_to_color_marks() {
    let spans = import_markdown_inline_incremental("留意<span style='color:blue;'>磁盘</span>问题")
        .expect("inline parse");
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].text, "留意");
    assert_eq!(spans[1].text, "磁盘");
    assert_eq!(
        spans[1].marks,
        vec![InlineMark::Color("#0000ff".to_owned())]
    );
    assert_eq!(spans[2].text, "问题");
}

#[test]
fn table_rows_pad_and_truncate_to_header_width_like_velotype() {
    let parsed = parse_markdown_document(
        "| A | B |\n|---|---|\n| short |\n| 1 | 2 | 3 |",
        MarkdownImportOptions::default(),
    );
    let table = parsed
        .blocks
        .iter()
        .find_map(|block| match &block.payload {
            BlockPayload::Table(table) => Some(table),
            _ => None,
        })
        .expect("table should parse");
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[1].cells.len(), 2);
    assert_eq!(
        cditor_core::rich_text::plain_text_from_spans(&table.rows[1].cells[0].spans),
        "short"
    );
    assert_eq!(
        cditor_core::rich_text::plain_text_from_spans(&table.rows[1].cells[1].spans),
        ""
    );
    assert_eq!(
        cditor_core::rich_text::plain_text_from_spans(&table.rows[2].cells[0].spans),
        "1"
    );
    assert_eq!(
        cditor_core::rich_text::plain_text_from_spans(&table.rows[2].cells[1].spans),
        "2"
    );
}
