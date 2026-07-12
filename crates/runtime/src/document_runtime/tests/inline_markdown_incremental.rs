/// Tests for inline Markdown incremental parsing
///
/// These tests verify that inline markdown shortcuts preserve existing marks
/// when new delimiters are added, rather than re-parsing the entire line.
use super::*;

#[test]
fn test_consecutive_inline_markdown_preserves_previous_marks() {
    // This test reproduces the bug where typing **asd**~~dasd~~ loses the Bold mark
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 0).unwrap();

    // Simulate typing "**asd**" character by character
    for ch in "**asd**".chars() {
        runtime.insert_char(ch).unwrap();
    }

    // Check that we have "asd" with Bold mark
    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text");
    };

    assert_eq!(
        cditor_core::rich_text::plain_text_from_spans(spans),
        "asd",
        "First markdown should be converted"
    );
    assert!(
        spans.iter().any(|s| s.marks.contains(&InlineMark::Bold)),
        "Should have Bold mark after typing **asd**, got spans: {:?}",
        spans
    );

    // Now type "~~dasd~~"
    for ch in "~~dasd~~".chars() {
        runtime.insert_char(ch).unwrap();
    }

    // Check that BOTH marks are preserved
    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text");
    };

    let text = cditor_core::rich_text::plain_text_from_spans(spans);
    assert_eq!(
        text, "asddasd",
        "Both markdown shortcuts should be converted"
    );

    // The critical assertion: Bold should still be present on "asd"
    let has_bold = spans.iter().any(|s| {
        s.text.contains("asd") && s.marks.contains(&InlineMark::Bold) && s.text.len() == 3 // Only the first "asd"
    });
    assert!(
        has_bold,
        "First 'asd' should still have Bold mark. Got spans: {:?}",
        spans
    );

    // And Strike should be on "dasd"
    let has_strike = spans
        .iter()
        .any(|s| s.text.contains("dasd") && s.marks.contains(&InlineMark::Strike));
    assert!(
        has_strike,
        "Second 'dasd' should have Strike mark. Got spans: {:?}",
        spans
    );
}

#[test]
fn test_nested_inline_markdown() {
    // Test **bold *italic* bold**
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 0).unwrap();

    for ch in "**bold *italic* bold**".chars() {
        runtime.insert_char(ch).unwrap();
    }

    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text");
    };

    let text = cditor_core::rich_text::plain_text_from_spans(spans);
    assert_eq!(text, "bold italic bold");

    // Check that "italic" has both Bold and Italic marks
    let nested_span = spans.iter().find(|s| s.text.contains("italic"));
    assert!(nested_span.is_some(), "Should have 'italic' span");

    if let Some(span) = nested_span {
        assert!(
            span.marks.contains(&InlineMark::Bold),
            "Nested 'italic' should have Bold"
        );
        assert!(
            span.marks.contains(&InlineMark::Italic),
            "Nested 'italic' should have Italic"
        );
    }
}
