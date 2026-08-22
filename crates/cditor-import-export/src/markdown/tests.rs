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
fn parses_mermaid_fenced_markdown_as_a_mermaid_block() {
    for source in [
        "```mermaid\nflowchart TD\n  A --> B\n```",
        "``` mermaid\nflowchart TD\n  A --> B\n```",
    ] {
        let parsed = parse_markdown_document(source, MarkdownImportOptions::default());
        assert_eq!(parsed.blocks.len(), 1, "{source:?}");
        let block = &parsed.blocks[0];
        assert_eq!(block.kind, RichBlockKind::Mermaid, "{source:?}");
        assert_eq!(block.payload.plain_text(), "flowchart TD\n  A --> B");
    }
}

#[test]
fn parses_mermaid_fenced_markdown_paste_as_a_mermaid_block() {
    let source = "```mermaid\nflowchart TD\n  A --> B\n```";
    let parsed = parse_markdown_paste_document(source, MarkdownImportOptions::default());
    assert_eq!(parsed.blocks.len(), 1);
    assert_eq!(parsed.blocks[0].kind, RichBlockKind::Mermaid);
    assert_eq!(
        parsed.blocks[0].payload.plain_text(),
        "flowchart TD\n  A --> B"
    );
}

#[test]
fn parses_mermaid_fenced_markdown_incrementally() {
    let source = "```MERMAID\nflowchart TD\n  A --> B\n```";
    let block = import_markdown_block_incremental(source, MarkdownImportOptions::default())
        .expect("complete fenced Mermaid should parse incrementally");
    assert_eq!(block.kind, RichBlockKind::Mermaid);
    assert_eq!(block.payload.plain_text(), "flowchart TD\n  A --> B");
}

#[test]
fn exports_mermaid_as_a_mermaid_fence() {
    let mut document = RichTextDocument::empty(1);
    document.push_root_block(RichBlockRecord::rich_text(
        1,
        RichBlockKind::Mermaid,
        "flowchart TD\n  A --> B",
    ));
    assert_eq!(
        export_plain_markdown(&document),
        "```mermaid\nflowchart TD\n  A --> B\n```"
    );
}

#[test]
fn exports_video_as_safe_html_that_preserves_the_source() {
    let mut document = RichTextDocument::empty(1);
    document.push_root_block(RichBlockRecord::new(
        1,
        RichBlockKind::Video,
        BlockPayload::Video(cditor_core::rich_text::VideoPayload {
            source: "/tmp/a&b\".mp4".to_owned(),
            title: "A <video>".to_owned(),
            ..Default::default()
        }),
    ));
    assert_eq!(
        export_plain_markdown(&document),
        r#"<video src="/tmp/a&amp;b&quot;.mp4" controls title="A &lt;video&gt;"></video>"#
    );
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
        cditor_core::rich_text::plain_text_from_spans(&table.rows[1].cells[0].spans),
        "left | right"
    );
}

#[test]
fn parse_gfm_table_builds_table_payload() {
    let table = parse_gfm_table("| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |")
        .expect("standalone table parses");
    assert_eq!(table.header_rows, 1);
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(
        cditor_core::rich_text::plain_text_from_spans(&table.rows[1].cells[0].spans),
        "1"
    );
}

#[test]
fn parse_gfm_table_rejects_prose_and_single_lines() {
    assert!(parse_gfm_table("just a paragraph").is_none());
    assert!(parse_gfm_table("| one | line |").is_none());
    assert!(parse_gfm_table("| A | B |\nparagraph after").is_none());
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

#[test]
fn commonmark_wrapped_paragraph_is_one_typed_block() {
    let parsed = parse_markdown_document(
        "first visual line\ncontinues in the same paragraph",
        MarkdownImportOptions::default(),
    );

    assert_eq!(parsed.blocks.len(), 1);
    assert_eq!(parsed.blocks[0].kind, RichBlockKind::Paragraph);
    assert_eq!(
        parsed.blocks[0].payload.plain_text(),
        "first visual line\ncontinues in the same paragraph"
    );
}

#[test]
fn unsupported_commonmark_blocks_preserve_exact_raw_source() {
    for source in [
        "<section data-x=\"1\">raw</section>",
        "[^note]: footnote **body**",
        "term\n: definition",
        "# heading {#stable-id}",
        "~~~rust\nfn main() {}\n~~~",
    ] {
        let parsed = parse_markdown_document(source, MarkdownImportOptions::default());
        assert_eq!(parsed.blocks.len(), 1, "{source:?}");
        assert_eq!(parsed.blocks[0].kind, RichBlockKind::RawMarkdown);
        assert_eq!(parsed.blocks[0].raw_fallback.as_deref(), Some(source));
        assert_eq!(export_plain_markdown(&parsed_to_document(parsed)), source);
    }
}

#[test]
fn gfm_table_and_task_list_remain_typed() {
    let parsed = parse_markdown_document(
        "- [x] shipped\n\n| A | B |\n|---|---|\n| 1 | 2 |",
        MarkdownImportOptions::default(),
    );

    assert!(matches!(
        parsed.blocks.first().map(|block| &block.kind),
        Some(RichBlockKind::Todo { checked: true })
    ));
    assert!(
        parsed
            .blocks
            .iter()
            .any(|block| block.kind == RichBlockKind::Table)
    );
}

#[test]
fn commonmark_gfm_fixture_matrix_is_typed_or_losslessly_preserved() {
    let typed = [
        ("# heading", RichBlockKind::Heading { level: 1 }),
        ("paragraph", RichBlockKind::Paragraph),
        ("> quote\n> continued", RichBlockKind::Quote),
        ("- item", RichBlockKind::BulletedList),
        ("1. item", RichBlockKind::NumberedList),
        (
            "```rust\nfn main() {}\n```",
            RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
        ),
        ("---", RichBlockKind::Separator),
        ("| A |\n|---|\n| 1 |", RichBlockKind::Table),
    ];
    for (source, expected) in typed {
        let parsed = parse_markdown_document(source, MarkdownImportOptions::default());
        assert_eq!(parsed.blocks[0].kind, expected, "{source:?}");
    }

    for source in [
        "setext heading\n===",
        "![alt](image.png)",
        "[reference][id]\n\n[id]: https://example.com",
        "inline <kbd>html</kbd>",
        "$inline-math$",
    ] {
        let parsed = parse_markdown_document(source, MarkdownImportOptions::default());
        assert!(
            parsed
                .blocks
                .iter()
                .all(|block| block.kind == RichBlockKind::RawMarkdown),
            "{source:?}: {:?}",
            parsed.blocks
        );
        assert_eq!(export_plain_markdown(&parsed_to_document(parsed)), source);
    }
}

fn parsed_to_document(parsed: ParsedMarkdownDocument) -> RichTextDocument {
    let mut document = RichTextDocument::empty(1);
    document.root_blocks = parsed.root_blocks;
    document.blocks = parsed.blocks;
    document
}

#[test]
fn export_plain_text_keeps_markdown_markers_verbatim() {
    let mut document = RichTextDocument::empty(1);
    document.push_root_block(RichBlockRecord::paragraph(1, "a_b *c* [d] e\\f 中文_测试"));
    assert_eq!(
        export_plain_markdown(&document),
        "a_b *c* [d] e\\f 中文_测试"
    );
}

#[test]
fn export_italic_uses_single_asterisk_like_lute() {
    let mut document = RichTextDocument::empty(1);
    let spans = vec![InlineSpan {
        text: "强调".to_owned(),
        marks: vec![InlineMark::Italic],
    }];
    document.push_root_block(RichBlockRecord::new(
        1,
        RichBlockKind::Paragraph,
        BlockPayload::RichText { spans },
    ));
    assert_eq!(export_plain_markdown(&document), "*强调*");
}

#[test]
fn export_link_text_escapes_protyle_inline_markers() {
    let mut document = RichTextDocument::empty(1);
    let spans = vec![InlineSpan {
        text: "a*b_c`~$=^<>".to_owned(),
        marks: vec![InlineMark::Link {
            href: "https://example.com".to_owned(),
        }],
    }];
    document.push_root_block(RichBlockRecord::new(
        1,
        RichBlockKind::Paragraph,
        BlockPayload::RichText { spans },
    ));
    assert_eq!(
        export_plain_markdown(&document),
        "[a\\*b\\_c\\`\\~\\$\\=\\^\\<\\>](https://example.com)"
    );
}

#[test]
fn export_multi_mark_spans_wrap_in_lute_order() {
    let mut document = RichTextDocument::empty(1);
    let spans = vec![
        InlineSpan {
            text: "bold".to_owned(),
            marks: vec![InlineMark::Bold],
        },
        InlineSpan {
            text: "both".to_owned(),
            marks: vec![InlineMark::Bold, InlineMark::Italic],
        },
        InlineSpan {
            text: "strike".to_owned(),
            marks: vec![InlineMark::Strike],
        },
    ];
    document.push_root_block(RichBlockRecord::new(
        1,
        RichBlockKind::Paragraph,
        BlockPayload::RichText { spans },
    ));
    assert_eq!(
        export_plain_markdown(&document),
        "**bold*****both***~~strike~~"
    );
}

#[test]
fn clipboard_selection_exports_rich_inline_markdown() {
    let selection = cditor_core::clipboard::ClipboardSelection::Inline {
        spans: vec![
            InlineSpan {
                text: "bold".to_owned(),
                marks: vec![InlineMark::Bold],
            },
            InlineSpan::plain(" and "),
            InlineSpan {
                text: "link".to_owned(),
                marks: vec![InlineMark::Link {
                    href: "https://example.com".to_owned(),
                }],
            },
        ],
    };

    assert_eq!(
        export_clipboard_selection_markdown(&selection),
        "**bold** and [link](https://example.com)"
    );
}

#[test]
fn clipboard_selection_exports_complete_blocks_with_block_syntax() {
    use cditor_core::clipboard::{ClipboardBlock, ClipboardSelection};

    let selection = ClipboardSelection::Blocks {
        blocks: vec![
            ClipboardBlock {
                source_id: 1,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Heading { level: 2 },
                payload: BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("Title")],
                },
            },
            ClipboardBlock {
                source_id: 2,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Todo { checked: true },
                payload: BlockPayload::RichText {
                    spans: vec![InlineSpan::plain("Done")],
                },
            },
        ],
    };

    assert_eq!(
        export_clipboard_selection_markdown(&selection),
        "## Title\n- [x] Done"
    );
}

#[test]
fn clipboard_selection_does_not_add_block_markers_to_partial_fragments() {
    use cditor_core::clipboard::{
        ClipboardBlockFragment, ClipboardFragmentBoundary, ClipboardSelection,
    };

    let selection = ClipboardSelection::TextFragments {
        fragments: vec![
            ClipboardBlockFragment {
                source_id: 1,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Heading { level: 1 },
                spans: vec![InlineSpan::plain("tle")],
                boundary: ClipboardFragmentBoundary::StartPartial,
                starts_at_block_start: false,
                ends_at_block_end: true,
            },
            ClipboardBlockFragment {
                source_id: 2,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Quote,
                spans: vec![InlineSpan {
                    text: "quo".to_owned(),
                    marks: vec![InlineMark::Italic],
                }],
                boundary: ClipboardFragmentBoundary::EndPartial,
                starts_at_block_start: true,
                ends_at_block_end: false,
            },
        ],
    };

    assert_eq!(
        export_clipboard_selection_markdown(&selection),
        "tle\n*quo*"
    );
}
