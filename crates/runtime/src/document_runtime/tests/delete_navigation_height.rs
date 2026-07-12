use super::*;

#[test]
fn backspace_at_start_merges_non_empty_paragraph_into_previous() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "hello "),
        (RichBlockKind::Paragraph, 0, None, "world"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;

    assert!(runtime.delete_backward().unwrap());

    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some("hello world"));
    assert_eq!(runtime.selected_focused_text(), Some("world".to_owned()));
    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
}

#[test]
fn list_item_backspace_first_resets_then_second_merges() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::BulletedList, 0, None, "b"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();

    assert!(runtime.delete_backward().unwrap());
    assert!(matches!(
        runtime.kind_for_block(2),
        RichBlockKind::Paragraph
    ));
    assert_eq!(runtime.index.total_count(), 2);

    assert!(runtime.delete_backward().unwrap());
    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some("ab"));
}

#[test]
fn empty_block_backspace_and_delete_remove_block_and_focus_adjacent() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::Paragraph, 0, None, ""),
        (RichBlockKind::Paragraph, 0, None, "c"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();

    assert!(runtime.delete_backward().unwrap());
    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.focused_block_id(), Some(1));

    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::Paragraph, 0, None, ""),
        (RichBlockKind::Paragraph, 0, None, "c"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();

    assert!(runtime.delete_forward().unwrap());
    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.focused_block_id(), Some(3));
}

#[test]
fn last_empty_block_is_not_deleted() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "")]);
    runtime.focus_block_at_offset(1, 0).unwrap();

    assert!(!runtime.delete_backward().unwrap());
    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some(""));
}

#[test]
fn delete_at_end_merges_next_block_into_current() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::Paragraph, 0, None, "b"),
    ]);
    runtime.focus_block_at_offset(1, 1).unwrap();

    assert!(runtime.delete_forward().unwrap());
    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some("ab"));
}

#[test]
fn arrow_keys_cross_block_boundaries_and_shift_extends_selection() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "ab"),
        (RichBlockKind::Paragraph, 0, None, "cd"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();

    assert!(runtime.move_caret_left(false).unwrap());
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some(2));

    assert!(runtime.move_caret_right(false).unwrap());
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(0));

    runtime.focus_block_at_offset(1, 2).unwrap();
    assert!(runtime.move_caret_right(true).unwrap());
    assert!(runtime.has_cross_block_text_selection());
}

#[test]
fn delete_document_selection_collapses_cross_block_range() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "ab"),
        (RichBlockKind::Paragraph, 0, None, "middle"),
        (RichBlockKind::Paragraph, 0, None, "cd"),
    ]);
    runtime
        .set_document_text_selection(1, 1, 3, 1)
        .expect("selection spans loaded blocks");

    assert!(runtime.delete_document_selection().unwrap());

    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some("ad"));
    assert_eq!(runtime.caret_offset_for_block(1), Some(1));
}

#[test]
fn up_down_fallback_focuses_adjacent_visible_blocks() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::Paragraph, 0, None, "b"),
        (RichBlockKind::Paragraph, 0, None, "c"),
    ]);
    runtime.focus_block_at_offset(2, 1).unwrap();

    assert!(runtime.move_caret_up(false).unwrap());
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some(1));

    assert!(runtime.move_caret_down(false).unwrap());
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(0));
}

#[test]
fn delete_backward_uses_caret_offset() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "a👨‍👩‍👧‍👦b",
        )],
        720.0,
    );
    let caret_after_emoji = "a👨‍👩‍👧‍👦".len();
    runtime.set_caret_offset(1, caret_after_emoji).unwrap();

    assert!(runtime.delete_backward().unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "ab");
    assert_eq!(projection.blocks[0].caret_offset, Some(1));
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, 1..1);
    assert_eq!(runtime.input_session_selected_range(), Some(1..1));
}

#[test]
fn backspace_at_start_resets_textual_block_styles_to_paragraph() {
    let kinds = [
        RichBlockKind::Heading { level: 1 },
        RichBlockKind::Quote,
        RichBlockKind::Callout {
            variant: cditor_core::rich_text::CalloutVariant::Note,
        },
        RichBlockKind::Todo { checked: true },
        RichBlockKind::BulletedList,
        RichBlockKind::NumberedList,
        RichBlockKind::Toggle,
        RichBlockKind::Math,
        RichBlockKind::Mermaid,
        RichBlockKind::FootnoteDefinition,
        RichBlockKind::Comment,
        RichBlockKind::RawMarkdown,
        RichBlockKind::Custom("legacy-text".to_owned()),
    ];

    for kind in kinds {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(1, kind.clone(), "keep text")],
            720.0,
        );
        runtime.focus_block_at_offset(1, 0).unwrap();

        assert!(runtime.delete_backward().unwrap(), "{kind:?} should reset");

        let projection = runtime.projection_for_window();
        assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "keep text");
        assert_eq!(projection.blocks[0].caret_offset, Some(0));
    }
}

#[test]
fn backspace_at_start_resets_code_and_html_payloads_to_paragraph_without_losing_text() {
    let cases = [
        BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {}".to_owned(),
            },
        },
        BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Html,
            payload: BlockPayload::Html {
                html: "<b>hello</b>".to_owned(),
                sanitized: true,
            },
        },
    ];

    for record in cases {
        let expected_text = record.plain_text();
        let mut runtime = DocumentRuntime::from_payloads(1, vec![record], 720.0);
        runtime.focus_block_at_offset(1, 0).unwrap();

        assert!(runtime.delete_backward().unwrap());

        let projection = runtime.projection_for_window();
        assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), expected_text);
        assert_eq!(projection.blocks[0].caret_offset, Some(0));
    }
}

#[test]
fn backspace_at_start_keeps_plain_paragraph_unchanged() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "plain",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 0).unwrap();

    assert!(!runtime.delete_backward().unwrap());

    let projection = runtime.projection_for_window();
    assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "plain");
    assert_eq!(projection.blocks[0].caret_offset, Some(0));
}

#[test]
fn measured_height_above_viewport_restores_viewport_top_anchor() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before = runtime.scroll.global_scroll_top;

    assert!(runtime.apply_measured_height(1, 1, 64.0).unwrap());

    assert_eq!(before, 3_200.0);
    assert_eq!(runtime.scroll.global_scroll_top, before + 32.0);
}

#[test]
fn measured_height_below_viewport_does_not_move_scroll_top() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before = runtime.scroll.global_scroll_top;

    assert!(runtime.apply_measured_height(900, 1, 64.0).unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before);
}

#[test]
fn measured_height_rejects_stale_content_version() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);
    runtime.insert_char('!').unwrap();

    let applied = runtime.apply_measured_height(3, 1, 96.0).unwrap();

    assert!(!applied);
    let block_index = runtime.index.index_of(3).unwrap();
    assert_ne!(
        runtime.index.layout_meta[block_index].measured_height,
        Some(96.0)
    );
}

#[test]
fn editing_code_block_preserves_code_payload() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main()".to_owned(),
            },
        }],
        720.0,
    );

    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime.insert_char('x').unwrap();

    let payload = runtime.payload_window.get(1).unwrap();
    match &payload.payload {
        BlockPayload::Code { language, text } => {
            assert_eq!(language.as_deref(), Some("rust"));
            assert_eq!(text, "fnx main()");
        }
        _ => panic!("expected code payload after editing code block"),
    }
    assert_eq!(runtime.caret_offset_for_block(1), Some(3));
}

#[test]
fn code_block_estimate_includes_language_label_and_chrome() {
    let payload = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind: RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        payload: BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "fn main() {\n    let value = 1;\n    value + 1\n}".to_owned(),
        },
    };

    assert!(estimate_payload_height(&payload, 0) >= RichBlockRecord::DEFAULT_CODE_HEIGHT);
    assert!(estimate_payload_height(&payload, 0) >= 130.0);
}

#[test]
fn enter_in_quote_soft_wraps_and_grows_block_height() {
    let records = vec![
        BlockIndexRecord::new(
            1,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Quote),
            0,
        )
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 36.0)),
    ];
    let payloads = vec![BlockPayloadRecord::rich_text(
        1,
        RichBlockKind::Quote,
        "> 引用块: UI 只是投影，runtime 才是真相。",
    )];
    let mut runtime = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
    runtime.focus_block(1);
    let before = runtime
        .index
        .index_of(1)
        .map(|index| runtime.index.layout_meta[index].effective_height())
        .unwrap();

    runtime.handle_enter().unwrap();

    assert!(runtime.focused_text().unwrap().contains('\n'));
    let after = runtime
        .index
        .index_of(1)
        .map(|index| runtime.index.layout_meta[index].effective_height())
        .unwrap();
    assert!(
        after > before,
        "quote height should grow: {before} -> {after}"
    );
}
