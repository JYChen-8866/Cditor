use super::*;

#[test]
fn current_page_window_clamps_first_middle_and_last_pages() {
    let mut runtime = runtime_with_paragraph_blocks(3_000);
    let page_count = runtime.layout.page_layout.page_count();
    assert!(page_count >= 4);

    assert_eq!(runtime.current_page_window().start, 0);
    assert!(runtime.current_page_window().contains(&0));

    let middle_page = page_count / 2;
    let middle_offset = runtime
        .layout
        .page_layout
        .offset_of_page(middle_page)
        .unwrap();
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(
            middle_offset,
            cditor_viewport::scroll::ScrollOrigin::ProgrammaticVirtualScroll,
        )
        .unwrap();
    let middle_window = runtime.current_page_window();
    assert!(middle_window.contains(&middle_page));
    assert_eq!(middle_window.start, middle_page.saturating_sub(1));
    assert!(middle_window.end <= page_count);

    runtime
        .layout
        .scroll
        .scroll_to_global_offset(
            runtime.layout.scroll.model_total_height,
            cditor_viewport::scroll::ScrollOrigin::ProgrammaticVirtualScroll,
        )
        .unwrap();
    let last_window = runtime.current_page_window();
    assert!(last_window.contains(&(page_count - 1)));
    assert_eq!(last_window.end, page_count);
}

#[test]
fn document_runtime_insert_char_updates_payload_and_pins_editing_block() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);
    runtime.insert_char('!').unwrap();
    let projection = runtime.full_projection_for_tests();
    let block = projection
        .blocks
        .iter()
        .find(|block| block.block_id == 3)
        .unwrap();
    assert!(block.focused);
    assert!(block.pinned);
    let BlockPayloadView::Loaded(payload) = &block.payload else {
        panic!("payload should be loaded");
    };
    assert!(payload.plain_text().ends_with('!'));
}

#[test]
fn document_runtime_delete_backward_removes_one_grapheme() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "a👨‍👩‍👧‍👦",
        )],
        720.0,
    );
    runtime.focus_block(1);

    assert!(runtime.delete_backward().unwrap());

    let projection = runtime.full_projection_for_tests();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "a");
}

#[test]
fn select_all_marks_visible_projection_without_ui_truth() {
    let mut runtime = DocumentRuntime::demo();
    assert!(runtime.select_all_visible_blocks());

    let projection = runtime.full_projection_for_tests();
    assert!(projection.blocks.iter().all(|block| block.selected));
    assert_eq!(projection.blocks.len(), 4);
}

#[test]
fn undo_and_redo_restore_focused_block_text_snapshot() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);
    runtime.insert_char('!').unwrap();

    assert!(runtime.undo_focused_block().unwrap());
    let projection = runtime.full_projection_for_tests();
    let block = projection
        .blocks
        .iter()
        .find(|block| block.block_id == 3)
        .unwrap();
    let BlockPayloadView::Loaded(payload) = &block.payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "点击窗口后直接输入文本。");

    assert!(runtime.redo_focused_block().unwrap());
    let projection = runtime.full_projection_for_tests();
    let block = projection
        .blocks
        .iter()
        .find(|block| block.block_id == 3)
        .unwrap();
    let BlockPayloadView::Loaded(payload) = &block.payload else {
        panic!("payload should be loaded");
    };
    assert!(payload.plain_text().ends_with('!'));
}

#[test]
fn undo_and_redo_restore_block_kind_style_snapshot() {
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

    runtime.insert_space_or_markdown_shortcut().unwrap();
    assert!(matches!(
        runtime.full_projection_for_tests().blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));

    assert!(runtime.undo_focused_block().unwrap());
    let projection = runtime.full_projection_for_tests();
    assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "#");

    assert!(runtime.redo_focused_block().unwrap());
    let projection = runtime.full_projection_for_tests();
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
    let editing = runtime.editing.session.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, 0..0);
}

#[test]
fn undo_and_redo_restore_inline_mark_style_snapshot() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "bold",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 0).unwrap();
    runtime.move_focused_caret_to_offset(1, 4, true).unwrap();

    assert!(
        runtime
            .toggle_inline_mark_on_selection(InlineMark::Bold)
            .unwrap()
    );
    assert!(
        runtime
            .loaded_payload_records_snapshot()
            .iter()
            .any(|record| {
                matches!(
                    &record.payload,
                    BlockPayload::RichText { spans }
                        if spans.iter().any(|span| span.marks.contains(&InlineMark::Bold))
                )
            })
    );

    assert!(runtime.undo_focused_block().unwrap());
    assert!(
        runtime
            .loaded_payload_records_snapshot()
            .iter()
            .all(|record| {
                matches!(
                    &record.payload,
                    BlockPayload::RichText { spans }
                        if spans.iter().all(|span| !span.marks.contains(&InlineMark::Bold))
                )
            })
    );

    assert!(runtime.redo_focused_block().unwrap());
    assert!(
        runtime
            .loaded_payload_records_snapshot()
            .iter()
            .any(|record| {
                matches!(
                    &record.payload,
                    BlockPayload::RichText { spans }
                        if spans.iter().any(|span| span.marks.contains(&InlineMark::Bold))
                )
            })
    );
}

#[test]
fn ctrl_enter_inserts_new_paragraph_after_focused_block() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);

    let new_block_id = runtime.insert_paragraph_after_focused().unwrap();

    assert_eq!(new_block_id, 5);
    assert_eq!(runtime.focused_block_id(), Some(5));
    let projection = runtime.full_projection_for_tests();
    assert_eq!(projection.blocks.len(), 5);
    assert_eq!(projection.blocks[3].block_id, 5);
    assert!(matches!(
        projection.blocks[3].kind,
        RichBlockKind::Paragraph
    ));
}

#[test]
fn ctrl_enter_after_multiline_code_preserves_the_entire_code_payload() {
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
                text: "fn main() {\n    println!(\"hello\");\n}".to_owned(),
            },
        }],
        720.0,
    );
    runtime.focus_block_at_offset(1, 12).unwrap();

    let inserted = runtime.insert_paragraph_after_focused().unwrap();

    assert_eq!(inserted, 2);
    assert_eq!(
        runtime.block_payload_record(1).unwrap().plain_text(),
        "fn main() {\n    println!(\"hello\");\n}"
    );
    assert_eq!(runtime.block_payload_record(2).unwrap().plain_text(), "");
    assert_eq!(runtime.focused_block_id(), Some(2));
}

#[test]
fn gutter_add_inserts_paragraph_without_replacing_complex_block() {
    let image = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind: RichBlockKind::Image,
        payload: BlockPayload::Image(cditor_core::rich_text::ImagePayload {
            source: "asset://image.png".to_owned(),
            alt: "image".to_owned(),
            caption: String::new().into(),
            display_width_ratio_milli: None,
        }),
    };
    let mut runtime = DocumentRuntime::from_payloads(1, vec![image], 720.0);

    let inserted = runtime.insert_paragraph_after_block(1).unwrap();

    assert_eq!(inserted, 2);
    assert!(matches!(
        runtime.block_payload_record(1).unwrap().payload,
        BlockPayload::Image(_)
    ));
    assert!(matches!(
        runtime.block_payload_record(2).unwrap().kind,
        RichBlockKind::Paragraph
    ));
    assert_eq!(runtime.focused_block_id(), Some(2));
}

#[test]
fn gutter_add_inserts_after_the_entire_target_subtree() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::Paragraph, 0, None),
    ]);

    let inserted = runtime.insert_paragraph_after_block(1).unwrap();

    assert_eq!(inserted, 4);
    assert_eq!(runtime.document.index.block_ids, vec![1, 2, 4, 3]);
    assert_eq!(
        runtime.document.index.parent_ids,
        vec![None, Some(1), None, None]
    );
    assert_eq!(runtime.document.index.depths, vec![0, 1, 0, 0]);
    assert_eq!(runtime.focused_block_id(), Some(4));
    assert_eq!(runtime.layout.height_index.heights.len(), 4);
}

#[test]
fn shift_enter_inserts_soft_line_break_in_focused_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "first line",
        )],
        720.0,
    );
    runtime.focus_block(1);
    let before_height = runtime.full_projection_for_tests().blocks[0]
        .layout
        .effective_height();
    let before_total_height = runtime.layout.height_index.total_height();

    runtime.insert_soft_line_break().unwrap();

    let projection = runtime.full_projection_for_tests();
    let block = &projection.blocks[0];
    let BlockPayloadView::Loaded(payload) = &block.payload else {
        panic!("payload should be loaded");
    };
    assert!(payload.plain_text().ends_with('\n'));
    assert!(
        block.layout.effective_height() > before_height,
        "soft line break should grow block height: {} <= {before_height}",
        block.layout.effective_height()
    );
    assert!(runtime.layout.height_index.total_height() > before_total_height);
    assert_eq!(
        runtime.layout.page_layout.total_height(),
        runtime.layout.height_index.total_height()
    );
    assert_eq!(
        runtime.layout.scroll.model_total_height,
        runtime.scroll_extent_height(runtime.layout.height_index.total_height())
    );
}

#[test]
fn space_shortcut_turns_marker_into_heading_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "#",
        )],
        720.0,
    );
    runtime.focus_block(1);

    runtime.insert_space_or_markdown_shortcut().unwrap();

    let projection = runtime.full_projection_for_tests();
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
fn space_shortcuts_cover_dynamic_lists_and_callouts() {
    let cases = [
        ("42.", RichBlockKind::NumberedList),
        ("+", RichBlockKind::BulletedList),
        (
            "> [!WARNING]",
            RichBlockKind::Callout {
                variant: cditor_core::rich_text::CalloutVariant::Warning,
            },
        ),
    ];

    for (marker, expected_kind) in cases {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                marker,
            )],
            720.0,
        );
        runtime.focus_block(1);

        runtime.insert_space_or_markdown_shortcut().unwrap();

        let payload = runtime
            .document
            .payload_window
            .get(1)
            .expect("converted payload");
        assert_eq!(payload.kind, expected_kind, "marker: {marker}");
        assert_eq!(payload.plain_text(), "");
    }
}

#[test]
fn horizontal_rule_shortcuts_commit_on_enter_instead_of_space() {
    for marker in ["---", "***", "___"] {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                marker,
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, marker.len()).unwrap();

        runtime.handle_enter().unwrap();

        let payload = runtime.block_payload_record(1).unwrap();
        assert_eq!(payload.kind, RichBlockKind::Separator, "marker: {marker}");
        assert_eq!(payload.payload, BlockPayload::Empty);
    }

    let mut space = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "---",
        )],
        720.0,
    );
    space.focus_block_at_offset(1, 3).unwrap();
    space.insert_space_or_markdown_shortcut().unwrap();
    let payload = space.block_payload_record(1).unwrap();
    assert_eq!(payload.kind, RichBlockKind::Paragraph);
    assert_eq!(payload.plain_text(), "--- ");
}

#[test]
fn space_shortcut_turns_markdown_task_marker_into_todo_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "- [ ]",
        )],
        720.0,
    );
    runtime.focus_block(1);

    runtime.insert_space_or_markdown_shortcut().unwrap();

    let projection = runtime.full_projection_for_tests();
    assert_eq!(
        projection.blocks[0].kind,
        RichBlockKind::Todo { checked: false }
    );
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn space_shortcut_turns_checked_markdown_task_marker_into_todo_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "- [x]",
        )],
        720.0,
    );
    runtime.focus_block(1);

    runtime.insert_space_or_markdown_shortcut().unwrap();

    let projection = runtime.full_projection_for_tests();
    assert_eq!(
        projection.blocks[0].kind,
        RichBlockKind::Todo { checked: true }
    );
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn typing_markdown_task_marker_from_empty_block_turns_bullet_into_todo() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "-",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    runtime.insert_space_or_markdown_shortcut().unwrap();
    assert_eq!(
        runtime.full_projection_for_tests().blocks[0].kind,
        RichBlockKind::BulletedList
    );

    runtime.insert_char('[').unwrap();
    runtime.insert_space_or_markdown_shortcut().unwrap();
    runtime.insert_char(']').unwrap();
    runtime.insert_space_or_markdown_shortcut().unwrap();

    let projection = runtime.full_projection_for_tests();
    assert_eq!(
        projection.blocks[0].kind,
        RichBlockKind::Todo { checked: false }
    );
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn enter_shortcut_turns_code_fence_into_code_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "```rust",
        )],
        720.0,
    );
    runtime.focus_block(1);

    runtime.handle_enter().unwrap();

    let projection = runtime.full_projection_for_tests();
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Code { ref language } if language.as_deref() == Some("rust")
    ));
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn enter_shortcut_turns_mermaid_fence_into_focused_mermaid_source() {
    let marker = "```mermaid";
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            marker,
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, marker.len()).unwrap();

    runtime.handle_enter().unwrap();

    let payload = runtime.block_payload_record(1).unwrap();
    assert_eq!(payload.kind, RichBlockKind::Mermaid);
    // 新建的 Mermaid 块预置一段简短示例，光标落在示例末尾。
    let source = payload.plain_text();
    assert!(
        source.starts_with("flowchart "),
        "unexpected source: {source}"
    );
    assert_eq!(source.lines().count(), 2);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some(source.len()));
}

#[test]
fn inline_markdown_shortcut_updates_payload_spans() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "hello **bold**",
        )],
        720.0,
    );
    runtime.focus_block(1);
    runtime.insert_char('!').unwrap();

    let projection = runtime.full_projection_for_tests();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("payload should be rich text");
    };
    assert_eq!(payload.plain_text(), "hello bold!");
    assert!(spans.iter().any(|span| {
        span.text == "bold"
            && span
                .marks
                .contains(&cditor_core::rich_text::InlineMark::Bold)
    }));
}
