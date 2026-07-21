use super::*;

#[test]
fn records_markdown_parse_stats_like_v1() {
    MARKDOWN_PARSE_STATS.reset();

    let _ = parse_markdown_document("# Title", MarkdownImportOptions::default());
    let _ = import_markdown_block_incremental("- item", MarkdownImportOptions::default());

    let snapshot = MARKDOWN_PARSE_STATS.snapshot();
    assert!(snapshot.full_parse_count >= 1);
    assert!(snapshot.full_parse_chars >= "# Title".len() as u64);
    assert!(snapshot.incremental_parse_count >= 1);
    assert!(snapshot.incremental_parse_chars >= "- item".len() as u64);
}

#[test]
fn markdown_paste_detection_includes_inline_syntax() {
    assert!(looks_like_markdown_paste("**bold** and `code`"));
    assert!(looks_like_markdown_paste("[link](https://example.com)"));
    assert!(!looks_like_markdown_paste("plain text"));
}

#[test]
fn outer_markdown_fence_is_unwrapped_before_block_parsing() {
    let parsed = parse_markdown_document(
        "```markdown\n# Title\n- item\n```",
        MarkdownImportOptions::default(),
    );

    assert!(matches!(
        parsed.blocks.first().map(|block| &block.kind),
        Some(RichBlockKind::Heading { level: 1 })
    ));
    assert!(matches!(
        parsed.blocks.get(1).map(|block| &block.kind),
        Some(RichBlockKind::BulletedList)
    ));
}

#[test]
fn block_shortcuts_match_editor2_markers() {
    assert_eq!(
        block_kind_shortcut("#"),
        Some(RichBlockKind::Heading { level: 1 })
    );
    assert_eq!(
        block_kind_shortcut("[x]"),
        Some(RichBlockKind::Todo { checked: true })
    );
    assert_eq!(
        block_kind_shortcut("- [ ]"),
        Some(RichBlockKind::Todo { checked: false })
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("- [x] done"),
        Some((RichBlockKind::Todo { checked: true }, 6))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("## title"),
        Some((RichBlockKind::Heading { level: 2 }, 3))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("42. item"),
        Some((RichBlockKind::NumberedList, 4))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("+ item"),
        Some((RichBlockKind::BulletedList, 2))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("--- "),
        Some((RichBlockKind::Separator, 4))
    );
    assert_eq!(
        block_kind_shortcut_with_marker_len("> [!WARNING] "),
        Some((
            RichBlockKind::Callout {
                variant: CalloutVariant::Warning,
            },
            13,
        ))
    );
    assert_eq!(
        code_fence_shortcut("```Rust"),
        Some(RichBlockKind::Code {
            language: Some("rust".to_owned())
        })
    );
}

#[test]
fn inline_shortcuts_parse_bold_code_and_link() {
    let spans =
        markdown_inline_shortcut_spans("hello **bold** and `code` plus [zed](https://zed.dev)")
            .expect("inline markdown should parse");
    assert!(
        spans
            .iter()
            .any(|span| span.marks.contains(&InlineMark::Bold) && span.text == "bold")
    );
    assert!(
        spans
            .iter()
            .any(|span| span.marks.contains(&InlineMark::Code) && span.text == "code")
    );
    assert!(spans.iter().any(|span| matches!(span.marks.as_slice(), [InlineMark::Link { href }] if href == "https://zed.dev")));
}

#[test]
fn inline_shortcuts_parse_combined_bold_italic() {
    let spans = markdown_inline_shortcut_spans("***strong emphasis*** and ___also___")
        .expect("combined inline markdown should parse");
    assert_eq!(spans[0].text, "strong emphasis");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold, InlineMark::Italic]);
    assert_eq!(spans[2].text, "also");
    assert_eq!(spans[2].marks, vec![InlineMark::Bold, InlineMark::Italic]);
}

#[test]
fn parses_markdown_document_blocks_tables_and_code() {
    let parsed = parse_markdown_document(
        "# Title\n- item\n  - child\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```",
        MarkdownImportOptions::default(),
    );
    assert!(matches!(
        parsed.blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    assert!(
        parsed
            .blocks
            .iter()
            .any(|block| matches!(block.kind, RichBlockKind::Table))
    );
    assert!(
        parsed
            .blocks
            .iter()
            .any(|block| matches!(block.kind, RichBlockKind::Code { .. }))
    );
    let child = parsed
        .blocks
        .iter()
        .find(|block| block.depth == 1)
        .expect("nested list child");
    assert!(child.parent_id.is_some());
}

#[test]
fn export_plain_markdown_matches_v1_basic_boundary() {
    let mut document = RichTextDocument::empty(1);
    document.push_root_block(RichBlockRecord::heading(1, 2, "Title"));
    document.push_root_block(RichBlockRecord::bulleted_list(2, "item"));
    document.push_root_block(RichBlockRecord::todo(3, true, "done"));
    document.push_root_block(RichBlockRecord::code_block(
        4,
        Some("rust".to_owned()),
        "fn main() {}",
    ));

    assert_eq!(
        export_plain_markdown(&document),
        "## Title\n- item\n- [x] done\n```rust\nfn main() {}\n```"
    );
}

#[test]
fn table_cells_support_escaped_pipe_like_v1() {
    let parsed = parse_markdown_document(
        "| A | B |\n|---|---|\n| left \\| right | ok |",
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
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[1].cells.len(), 2);
    assert_eq!(
        crate::rich_text::plain_text_from_spans(&table.rows[1].cells[0].spans),
        "left | right"
    );
}

#[test]
fn incremental_multiline_callout_is_supported() {
    let block = import_markdown_block_incremental(
        "> [!WARNING]\n> be careful",
        MarkdownImportOptions::default(),
    )
    .expect("callout markdown should parse");
    assert!(matches!(
        block.kind,
        RichBlockKind::Callout {
            variant: CalloutVariant::Warning
        }
    ));
}
