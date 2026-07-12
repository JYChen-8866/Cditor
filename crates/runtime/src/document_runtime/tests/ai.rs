use super::*;
use cditor_ai::AiStreamEvent;

#[test]
fn empty_editorial_text_blocks_support_space_invoked_ai() {
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
    assert_eq!(runtime.focused_empty_text_block_for_ai(), Some((1, 0)));

    runtime.insert_char('x').unwrap();
    assert_eq!(runtime.focused_empty_text_block_for_ai(), None);

    let mut heading = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Heading { level: 1 },
            "",
        )],
        720.0,
    );
    heading.focus_block_at_offset(1, 0).unwrap();
    assert_eq!(heading.focused_empty_text_block_for_ai(), Some((1, 0)));
}

#[test]
fn empty_line_ai_projects_a_streaming_assistant_panel() {
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
    let dispatch = runtime
        .begin_ai_request_with_presentation("Write", AiRequestPresentation::AssistantPanel)
        .unwrap();
    runtime.apply_ai_stream_event(AiStreamEvent::Delta {
        request_id: dispatch.request.request_id,
        text: "streamed".to_owned(),
    });

    let preview = runtime.projection_for_window().ai_preview.unwrap();
    assert_eq!(preview.kind, AiPreviewKind::AssistantPanel);
    assert_eq!(preview.status, AiPreviewStatus::Streaming);
    assert_eq!(preview.text, "streamed");
    assert_eq!(runtime.focused_text(), Some(""));
}

#[test]
fn assistant_panel_stream_survives_editor_focus_transition() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, ""),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "other block"),
        ],
        720.0,
    );
    runtime.focus_block_at_offset(1, 0).unwrap();
    let dispatch = runtime
        .begin_ai_request_with_presentation("Write", AiRequestPresentation::AssistantPanel)
        .unwrap();

    // Returning focus to the document after submitting the prompt must not
    // invalidate the assistant panel while the original block is unchanged.
    runtime.focus_block_at_offset(2, 0).unwrap();
    assert_eq!(
        runtime.apply_ai_stream_event(AiStreamEvent::Delta {
            request_id: dispatch.request.request_id,
            text: "still streaming".to_owned(),
        }),
        AiStreamApplyResult::Applied
    );
    assert_eq!(
        runtime.ai_session_snapshot().map(|session| session.preview),
        Some("still streaming".to_owned())
    );
}

#[test]
fn accepting_empty_line_ai_parses_markdown_into_structured_blocks() {
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
    let dispatch = runtime
        .begin_ai_request_with_presentation("Write a plan", AiRequestPresentation::AssistantPanel)
        .unwrap();
    runtime.apply_ai_stream_event(AiStreamEvent::Delta {
        request_id: dispatch.request.request_id,
        text: "### 阶段一\n- 学习 `Rust`".to_owned(),
    });
    runtime.apply_ai_stream_event(AiStreamEvent::Done {
        request_id: dispatch.request.request_id,
    });

    assert!(runtime.accept_ai_preview().unwrap());
    let first = runtime.block_payload_record(1).unwrap();
    assert!(matches!(first.kind, RichBlockKind::Heading { level: 3 }));
    assert_eq!(first.plain_text(), "阶段一");
    let second = runtime
        .projection_for_window()
        .blocks
        .iter()
        .find(|block| block.block_id != 1)
        .and_then(|block| runtime.block_payload_record(block.block_id));
    assert_eq!(
        second.as_ref().map(|payload| payload.plain_text()),
        Some("学习 Rust".to_owned())
    );
    let Some(BlockPayloadRecord {
        payload: BlockPayload::RichText { spans },
        ..
    }) = second.as_ref()
    else {
        panic!("AI markdown list item should be rich text");
    };
    assert!(
        spans
            .iter()
            .any(|span| span.text == "Rust" && span.marks.contains(&InlineMark::Code))
    );
    assert!(runtime.undo_focused_block().unwrap());
    let restored = runtime.block_payload_record(1).unwrap();
    assert!(matches!(restored.kind, RichBlockKind::Paragraph));
    assert_eq!(restored.plain_text(), "");
}

#[test]
fn inline_ai_stream_is_projected_without_mutating_document() {
    let mut runtime = DocumentRuntime::demo();
    let block_id = runtime.projection_for_window().blocks[0].block_id;
    runtime.focus_block_at_offset(block_id, 1).unwrap();
    let before = runtime.block_payload_record(block_id).unwrap();
    let dispatch = runtime.begin_ai_request("Continue writing").unwrap();
    assert_eq!(
        dispatch.request.task,
        cditor_ai::AiTaskKind::InlineCompletion
    );
    assert_eq!(
        runtime.apply_ai_stream_event(AiStreamEvent::Delta {
            request_id: dispatch.request.request_id,
            text: " suggested".to_owned(),
        }),
        AiStreamApplyResult::Applied
    );
    let projection = runtime.projection_for_window();
    let preview = projection.ai_preview.unwrap();
    assert_eq!(preview.block_id, block_id);
    assert_eq!(preview.anchor_offset, 1);
    assert_eq!(preview.text, " suggested");
    assert_eq!(runtime.block_payload_record(block_id).unwrap(), before);
}

#[test]
fn stale_ai_stream_is_discarded_after_target_edit() {
    let mut runtime = DocumentRuntime::demo();
    let block_id = runtime.projection_for_window().blocks[0].block_id;
    runtime.focus_block_at_offset(block_id, 1).unwrap();
    let dispatch = runtime.begin_ai_request("Continue").unwrap();
    runtime.insert_char('x').unwrap();
    assert_eq!(
        runtime.apply_ai_stream_event(AiStreamEvent::Delta {
            request_id: dispatch.request.request_id,
            text: "old".to_owned(),
        }),
        AiStreamApplyResult::DiscardedStale
    );
    assert!(dispatch.cancellation.is_cancelled());
    assert!(runtime.projection_for_window().ai_preview.is_none());
}

#[test]
fn accepted_inline_ai_is_one_undo_step() {
    let mut runtime = DocumentRuntime::demo();
    let block_id = runtime.projection_for_window().blocks[0].block_id;
    runtime.focus_block_at_offset(block_id, 1).unwrap();
    let before = runtime.block_payload_record(block_id).unwrap();
    let dispatch = runtime.begin_ai_request("Continue").unwrap();
    runtime.apply_ai_stream_event(AiStreamEvent::Delta {
        request_id: dispatch.request.request_id,
        text: "AI".to_owned(),
    });
    runtime.apply_ai_stream_event(AiStreamEvent::Done {
        request_id: dispatch.request.request_id,
    });
    assert!(runtime.accept_ai_preview().unwrap());
    assert_ne!(runtime.block_payload_record(block_id).unwrap(), before);
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.block_payload_record(block_id).unwrap(), before);
    assert!(!runtime.undo_focused_block().unwrap());
}

#[test]
fn rejected_ai_leaves_payload_and_undo_unchanged() {
    let mut runtime = DocumentRuntime::demo();
    let block_id = runtime.projection_for_window().blocks[0].block_id;
    runtime.focus_block_at_offset(block_id, 1).unwrap();
    let before = runtime.block_payload_record(block_id).unwrap();
    let dispatch = runtime.begin_ai_request("Continue").unwrap();
    runtime.apply_ai_stream_event(AiStreamEvent::Delta {
        request_id: dispatch.request.request_id,
        text: "AI".to_owned(),
    });
    assert!(runtime.reject_ai_preview());
    assert_eq!(runtime.block_payload_record(block_id).unwrap(), before);
    assert!(!runtime.undo_focused_block().unwrap());
}

#[test]
fn cross_block_ai_rewrite_accepts_atomically_and_undo_restores_blocks() {
    let mut runtime = DocumentRuntime::demo();
    let projection = runtime.projection_for_window();
    let first = projection.blocks[0].block_id;
    let second = projection.blocks[1].block_id;
    let before_first = runtime.block_payload_record(first).unwrap();
    let before_second = runtime.block_payload_record(second).unwrap();
    runtime
        .set_document_text_selection(first, 1, second, 1)
        .unwrap();
    let dispatch = runtime.begin_ai_request("Improve writing").unwrap();
    runtime.apply_ai_stream_event(AiStreamEvent::Delta {
        request_id: dispatch.request.request_id,
        text: "replacement".to_owned(),
    });
    runtime.apply_ai_stream_event(AiStreamEvent::Done {
        request_id: dispatch.request.request_id,
    });
    assert!(runtime.accept_ai_preview().unwrap());
    assert!(runtime.index.index_of(second).is_none());
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.block_payload_record(first).unwrap(), before_first);
    assert_eq!(runtime.block_payload_record(second).unwrap(), before_second);
}

#[test]
fn ai_insert_after_selection_preserves_selected_text_and_is_undoable() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.set_document_text_selection(1, 1, 1, 3).unwrap();
    let dispatch = runtime.begin_ai_request("Explain").unwrap();
    runtime.apply_ai_stream_event(AiStreamEvent::Delta {
        request_id: dispatch.request.request_id,
        text: "AI".to_owned(),
    });
    runtime.apply_ai_stream_event(AiStreamEvent::Done {
        request_id: dispatch.request.request_id,
    });

    assert!(runtime.apply_ai_preview(AiApplyMode::InsertAfter).unwrap());
    assert_eq!(runtime.focused_text(), Some("abcAIdef"));
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.focused_text(), Some("abcdef"));
}

#[test]
fn ai_replace_uses_the_original_single_block_selection_range() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.set_document_text_selection(1, 3, 1, 1).unwrap();
    let dispatch = runtime.begin_ai_request("Rewrite").unwrap();
    runtime.apply_ai_stream_event(AiStreamEvent::Delta {
        request_id: dispatch.request.request_id,
        text: "AI".to_owned(),
    });
    runtime.apply_ai_stream_event(AiStreamEvent::Done {
        request_id: dispatch.request.request_id,
    });

    assert!(runtime.apply_ai_preview(AiApplyMode::Replace).unwrap());
    assert_eq!(runtime.focused_text(), Some("aAIdef"));
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.focused_text(), Some("abcdef"));
}

#[test]
fn ai_replace_selection_parses_markdown_into_structured_blocks() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "旧内容",
        )],
        720.0,
    );
    runtime
        .set_document_text_selection(1, 0, 1, "旧内容".len())
        .unwrap();
    let dispatch = runtime.begin_ai_request("Rewrite as markdown").unwrap();
    runtime.apply_ai_stream_event(AiStreamEvent::Delta {
        request_id: dispatch.request.request_id,
        text: "# 新标题\n- 第一项\n- 第二项".to_owned(),
    });
    runtime.apply_ai_stream_event(AiStreamEvent::Done {
        request_id: dispatch.request.request_id,
    });

    assert!(runtime.apply_ai_preview(AiApplyMode::Replace).unwrap());
    let projection = runtime.projection_for_window();
    let kinds = projection
        .blocks
        .iter()
        .filter_map(|block| runtime.block_payload_record(block.block_id))
        .map(|payload| payload.kind)
        .collect::<Vec<_>>();
    assert!(matches!(
        kinds.first(),
        Some(RichBlockKind::Heading { level: 1 })
    ));
    assert!(matches!(kinds.get(1), Some(RichBlockKind::BulletedList)));
    assert!(matches!(kinds.get(2), Some(RichBlockKind::BulletedList)));
}

#[test]
fn cross_block_ai_insert_preserves_all_selected_blocks() {
    let mut runtime = DocumentRuntime::demo();
    let projection = runtime.projection_for_window();
    let first = projection.blocks[0].block_id;
    let second = projection.blocks[1].block_id;
    let before_first = runtime.block_payload_record(first).unwrap();
    let before_second = runtime.block_payload_record(second).unwrap();
    runtime
        .set_document_text_selection(first, 1, second, 1)
        .unwrap();
    let dispatch = runtime.begin_ai_request("Explain").unwrap();
    runtime.apply_ai_stream_event(AiStreamEvent::Delta {
        request_id: dispatch.request.request_id,
        text: "AI".to_owned(),
    });
    runtime.apply_ai_stream_event(AiStreamEvent::Done {
        request_id: dispatch.request.request_id,
    });

    assert!(runtime.apply_ai_preview(AiApplyMode::InsertAfter).unwrap());
    assert!(runtime.index.index_of(first).is_some());
    assert!(runtime.index.index_of(second).is_some());
    assert_eq!(runtime.block_payload_record(first).unwrap(), before_first);
    assert_ne!(runtime.block_payload_record(second).unwrap(), before_second);
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.block_payload_record(first).unwrap(), before_first);
    assert_eq!(runtime.block_payload_record(second).unwrap(), before_second);
}
