use super::*;

#[test]
fn replace_text_space_path_applies_block_markdown_shortcut() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "#",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, " ").unwrap());

    let projection = runtime.projection_for_window();
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn bold_markdown_shortcut_creates_bold_not_italic() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "**abc**",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, "**abc**".len()).unwrap();

    assert!(runtime.apply_inline_markdown_shortcut(1).unwrap());

    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text payload");
    };
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "abc");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
    assert_eq!(runtime.caret_offset_for_block(1), Some("abc".len()));
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("abc".len().."abc".len())
    );
}

#[test]
fn inserting_inside_bold_span_preserves_bold_mark() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "ab".to_owned(),
                    marks: vec![InlineMark::Bold],
                }],
            },
        }],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    runtime.insert_char('X').unwrap();

    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text payload");
    };
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "aXb");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
}

#[test]
fn deleting_inside_bold_span_preserves_remaining_bold_mark() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "abc".to_owned(),
                    marks: vec![InlineMark::Bold],
                }],
            },
        }],
        720.0,
    );
    runtime.focus_block_at_offset(1, "abc".len()).unwrap();

    assert!(runtime.delete_backward().unwrap());

    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text payload");
    };
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "ab");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
}

#[test]
fn replace_text_path_applies_inline_markdown_shortcut() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "**bold**",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, "**bold**".len()).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "!").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("payload should be rich text");
    };
    assert_eq!(payload.plain_text(), "bold!");
    assert!(spans.iter().any(|span| {
        span.text == "bold"
            && span
                .marks
                .contains(&cditor_core::rich_text::InlineMark::Bold)
    }));
    assert_eq!(projection.blocks[0].caret_offset, Some("bold!".len()));
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("bold!".len().."bold!".len())
    );
}

#[test]
fn move_focused_caret_to_offset_updates_caret_without_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    assert!(runtime.move_focused_caret_to_offset(1, 5, false).unwrap());

    let projection = runtime.projection_for_window();
    assert_eq!(projection.blocks[0].caret_offset, Some(5));
    assert_eq!(runtime.focused_text_selection_range(), None);
}

#[test]
fn move_focused_caret_to_offset_extends_same_block_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    assert!(runtime.move_focused_caret_to_offset(1, 5, true).unwrap());

    let projection = runtime.projection_for_window();
    assert_eq!(projection.blocks[0].caret_offset, Some(5));
    assert_eq!(runtime.focused_text_selection_range(), Some(2..5));
    assert_eq!(runtime.selected_focused_text().as_deref(), Some("cde"));
}

#[test]
fn insert_char_uses_middle_caret_offset() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    runtime.insert_char('X').unwrap();

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abXcd");
    assert_eq!(projection.blocks[0].caret_offset, Some(3));
}

#[test]
fn replace_text_in_focused_range_can_insert_in_middle_without_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "中").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "ab中cd");
    assert_eq!(projection.blocks[0].caret_offset, Some("ab中".len()));
}

#[test]
fn replace_text_in_focused_range_inserts_string_at_middle_caret() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 3).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "XYZ").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abcXYZdef");
    assert_eq!(projection.blocks[0].caret_offset, Some("abcXYZ".len()));
}

#[test]
fn replace_text_in_focused_range_replaces_selection_and_caret_after_inserted_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime.set_document_text_selection(1, 2, 1, 4).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "XYZ").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abXYZef");
    assert_eq!(projection.blocks[0].caret_offset, Some("abXYZ".len()));
    assert_eq!(runtime.focused_text_selection_range(), None);
}

#[test]
fn ime_preview_and_commit_can_start_in_middle_of_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    runtime.begin_or_update_composition(1, 2..2, "你").unwrap();
    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("ab你cd")
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(2.."ab你".len())
    );
    assert!(runtime.commit_composition().unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "ab你cd");
    assert_eq!(projection.blocks[0].caret_offset, Some("ab你".len()));
}

#[test]
fn replace_text_prioritizes_active_composition_over_stale_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.set_document_text_selection(1, 1, 1, 5).unwrap();
    runtime
        .begin_or_update_composition_with_selection(1, 3..3, "中", None)
        .unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "文").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abc文def");
    assert_eq!(projection.blocks[0].caret_offset, Some("abc文".len()));
}

#[test]
fn ime_preview_tracks_selected_subrange_inside_marked_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    runtime
        .begin_or_update_composition_with_selection(1, 2..2, "你好", Some("你".len().."你好".len()))
        .unwrap();

    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("ab你好cd")
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(2.."ab你好".len())
    );
    assert_eq!(
        runtime.active_composition_selected_range(),
        Some("ab你".len().."ab你好".len())
    );
    assert_eq!(runtime.caret_offset_for_block(1), Some("ab你好".len()));
}

#[test]
fn ime_composition_update_replaces_base_range_for_cjk_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    runtime.begin_or_update_composition(1, 2..2, "に").unwrap();
    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("abにcd")
    );

    runtime
        .begin_or_update_composition(1, 2..2, "日本")
        .unwrap();
    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("ab日本cd")
    );

    runtime.begin_or_update_composition(1, 2..2, "한").unwrap();
    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("ab한cd")
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(2.."ab한".len())
    );

    assert!(runtime.commit_composition().unwrap());
    assert_eq!(
        runtime.payload_window.get(1).unwrap().plain_text(),
        "ab한cd"
    );
}
