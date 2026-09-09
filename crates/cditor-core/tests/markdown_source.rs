use cditor_core::markdown::*;
use cditor_core::rich_text::{InlineMark, InlineSpan, parse_inline_markdown_extended};

fn caret(offset: usize) -> SourceSelection {
    SourceSelection { anchor: ByteOffset(offset), focus: ByteOffset(offset) }
}
fn edit(start: usize, end: usize, replacement: &str) -> SourceEdit {
    SourceEdit { range: ByteOffset(start)..ByteOffset(end), replacement: replacement.into() }
}

#[test]
fn source_preserves_all_bytes_and_nested_nodes() {
    let input = "\r\n__bold *中文* bold__\r\n\r\n[a][ref]\r\n\r\n[ref]: <https://example.com> 'title'\r\n";
    let source = MarkdownSource::new(input);
    assert_eq!(source.text(), input);
    let syntax = source.parse();
    let (id, strong) = syntax.nodes().iter().enumerate().find(|(_, n)| n.kind == SyntaxKind::Strong).unwrap();
    assert_eq!(&input[strong.source.clone()], "__bold *中文* bold__");
    assert!(strong.children.iter().any(|&child| syntax.nodes()[child].kind == SyntaxKind::Emphasis));
    assert_eq!(syntax.gaps(id).iter().map(|range| &input[range.clone()]).collect::<Vec<_>>(), ["__", "__"]);
    let projection = syntax.project_inline(id, input).unwrap();
    assert_eq!(projection.text, "bold 中文 bold");
    assert_eq!(projection.spans[1].marks, [InlineMark::Bold, InlineMark::Italic]);
}

#[test]
fn projection_affinity_and_transformed_tokens() {
    let text = "a**中文**b &amp; `code`";
    let syntax = MarkdownSyntax::parse(text);
    let projection = syntax.project_inline(1, text).unwrap();
    assert_eq!(projection.visible_to_source(1, Affinity::Before).unwrap(), 1);
    assert_eq!(projection.visible_to_source(1, Affinity::After).unwrap(), 3);
    assert!(projection.visible_to_source(2, Affinity::After).is_err());
    assert!(projection.source_to_visible(4, Affinity::After).is_err());
    let code = projection.text.find("code").unwrap();
    assert!(projection.visible_to_source(code + 1, Affinity::After).is_err());
    let entity = text.find("&amp;").unwrap();
    let visible = projection.text.find('&').unwrap();
    assert_eq!(projection.source_to_visible(entity + 2, Affinity::Before).unwrap(), visible);
    assert_eq!(projection.source_to_visible(entity + 2, Affinity::After).unwrap(), visible + 1);
}

#[test]
fn source_transactions_are_atomic_reversible_and_versioned() {
    let mut source = MarkdownSource::new("**abc** 中文");
    source.set_selection(caret(3)).unwrap();
    source.apply(SourceTransaction { revision: 0, edits: vec![edit(3, 4, "XYZ"), edit(8, 14, "text")], after_selection: caret(6) }).unwrap();
    assert_eq!(source.text(), "**aXYZc** text");
    assert_eq!(source.selection(), &caret(6));
    assert!(source.undo().unwrap());
    assert_eq!(source.text(), "**abc** 中文");
    assert_eq!(source.selection(), &caret(3));
    assert!(source.redo().unwrap());
    assert_eq!(source.text(), "**aXYZc** text");
    assert_eq!(source.revision(), 3);
    assert_eq!(source.apply(SourceTransaction { revision: 0, edits: vec![], after_selection: caret(0) }), Err(MarkdownError::StaleRevision));
    let before = source.text();
    assert!(source.apply(SourceTransaction { revision: 3, edits: vec![edit(0, 1, "x")], after_selection: caret(999) }).is_err());
    assert_eq!(source.text(), before);
}

#[test]
fn adjacent_deletions_undo_and_new_edit_clears_redo() {
    let mut source = MarkdownSource::new("abcd");
    source.apply(SourceTransaction { revision: 0, edits: vec![edit(0, 1, ""), edit(1, 2, "")], after_selection: caret(0) }).unwrap();
    assert_eq!(source.text(), "cd");
    source.undo().unwrap();
    assert_eq!(source.text(), "abcd");
    source.apply(SourceTransaction { revision: 2, edits: vec![edit(0, 1, "A")], after_selection: caret(1) }).unwrap();
    assert!(!source.redo().unwrap());
    assert!(source.apply(SourceTransaction { revision: 3, edits: vec![edit(0, 2, ""), edit(1, 3, "")], after_selection: caret(0) }).is_err());
    assert_eq!(source.text(), "Abcd");
}

#[test]
fn utf16_rejects_half_surrogates_and_half_utf8() {
    let source = MarkdownSource::new("a中😀e\u{301}");
    for (byte, utf16) in [(0, 0), (1, 1), (4, 2), (8, 4), (9, 5), (11, 6)] {
        assert_eq!(source.byte_to_utf16(ByteOffset(byte)).unwrap(), utf16);
        assert_eq!(source.utf16_to_byte(utf16).unwrap(), ByteOffset(byte));
    }
    assert!(source.utf16_to_byte(3).is_err());
    assert!(source.byte_to_utf16(ByteOffset(2)).is_err());
    assert!(source.utf16_to_byte(100).is_err());
}

#[test]
fn shortcut_nested_links_code_and_same_mark_nesting() {
    for text in [
        "**bold *italic* bold**", "*italic **bold** italic*", "**outer **inner** outer**",
        "[**bold** *italic*](https://example.com/a_(b))", "**bold `code` bold**",
        "**中文 *嵌套* 中文**", "``a`b``", "++under **bold** under++",
    ] {
        let result = parse_inline_markdown_extended(text);
        assert!(result.changed, "{text}");
        if !text.contains("++") {
            let expected = MarkdownSyntax::parse(text).project_inline(1, text).unwrap();
            assert!(equivalent_spans(&result.spans, &expected.spans), "{text}: {:?}", result.spans);
        }
    }
    let result = parse_inline_markdown_extended("**`code`**");
    assert!(result.spans[0].marks.contains(&InlineMark::Bold));
    assert!(result.spans[0].marks.contains(&InlineMark::Code));
    for text in ["**incomplete", "**bold*", "foo_bar_baz", "\\*literal\\*", "[**link](bad)"] {
        let result = parse_inline_markdown_extended(text);
        assert!(!result.changed, "{text}");
        assert_eq!(result.spans, [InlineSpan::plain(text)]);
    }
}

#[test]
fn canonical_nested_export_and_unsupported_formats() {
    for input in ["**bold *italic* bold**", "*italic **bold** italic*", "~~strike **bold**~~", "[**bold** *italic*](https://example.com)", "`` `edge` ``", "`  `"] {
        let projection = MarkdownSyntax::parse(input).project_inline(1, input).unwrap();
        let output = render_inline_spans(&projection.spans).unwrap();
        let reparsed = MarkdownSyntax::parse(&output).project_inline(1, &output).unwrap();
        assert!(equivalent_spans(&projection.spans, &reparsed.spans), "{input}: {output}");
    }
    for marks in [vec![InlineMark::Color("red".into())], vec![InlineMark::Underline], vec![InlineMark::Code, InlineMark::Bold]] {
        assert!(render_inline_spans(&[InlineSpan { text: "text".into(), marks }]).is_err());
    }
}

#[test]
fn deeply_nested_source_does_not_recurse() {
    let input = format!("{}text", "> ".repeat(1000));
    let source = MarkdownSource::new(&input);
    assert!(source.parse().nodes().len() > 1000);
}
