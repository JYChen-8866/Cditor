use super::*;
use cditor_core::edit::SelectionRange;

#[test]
fn document_text_selection_projects_partial_and_full_ranges() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "abcd"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "efgh"),
            BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "ijkl"),
        ],
        720.0,
    );

    runtime.set_document_text_selection(1, 2, 3, 1).unwrap();
    let projection = runtime.projection_for_window();

    assert_eq!(
        projection.blocks[0].selection_range,
        Some(SelectionRange::Partial(2..4))
    );
    assert_eq!(
        projection.blocks[1].selection_range,
        Some(SelectionRange::Full)
    );
    assert_eq!(
        projection.blocks[2].selection_range,
        Some(SelectionRange::Partial(0..1))
    );
    assert!(
        projection.blocks.iter().all(|block| !block.selected),
        "cross-block text fragments must not become whole-block selections"
    );
    assert_eq!(
        runtime.selected_document_text().as_deref(),
        Some("cd\nefgh\ni")
    );
}

#[test]
fn focused_text_selection_replaces_and_moves_with_shift_arrows() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block_at_offset(3, 0).unwrap();

    assert!(runtime.move_caret_right(true).unwrap());
    let selected = runtime.selected_focused_text().unwrap();
    assert_eq!(selected.chars().count(), 1);

    runtime.insert_char('你').unwrap();
    assert!(runtime.focused_text_selection_range().is_none());
    assert!(runtime.focused_text().unwrap().starts_with('你'));
}

#[test]
fn select_all_copy_cut_paste_and_inline_mark_work_on_focused_text() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block_at_offset(3, 0).unwrap();
    assert!(runtime.select_focused_text_all());
    let selected = runtime.selected_focused_text().unwrap();
    assert!(!selected.is_empty());

    assert!(
        runtime
            .toggle_inline_mark_on_selection(InlineMark::Bold)
            .unwrap()
    );
    let payload = runtime.payload_window.get(3).unwrap();
    match &payload.payload {
        BlockPayload::RichText { spans } => {
            assert!(
                spans
                    .iter()
                    .any(|span| span.marks.contains(&InlineMark::Bold))
            );
        }
        _ => panic!("expected rich text payload"),
    }
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 3 });
    assert_eq!(editing.selected_range, 0..selected.len());

    assert!(
        runtime
            .replace_text_in_focused_range(None, "粘贴文本")
            .unwrap()
    );
    assert_eq!(runtime.focused_text(), Some("粘贴文本"));
}

#[test]
fn inline_mark_rejects_code_payload_without_mutation_or_undo() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "fn main() {}".to_owned(),
        },
    );
    runtime.focus_block_at_offset(1, 0).unwrap();
    runtime.set_document_text_selection(1, 0, 1, 2).unwrap();
    let before = runtime.block_payload_record(1).unwrap();

    let error = runtime
        .toggle_inline_mark_on_selection(InlineMark::Bold)
        .unwrap_err();

    assert!(error.contains("does not support inline marks"));
    assert_eq!(runtime.block_payload_record(1).unwrap(), before);
    assert!(!runtime.undo_stacks.contains_key(&1));
}

#[test]
fn queued_measured_heights_do_not_apply_until_flush() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_total_height = runtime.height_index.total_height();

    assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    assert_eq!(runtime.height_index.total_height(), before_total_height);

    assert!(runtime.flush_pending_height_corrections().unwrap());
    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top + 32.0);
    assert_eq!(
        runtime.height_index.total_height(),
        before_total_height + 32.0
    );
}

#[test]
fn flush_measured_heights_restores_anchor_once_for_batched_changes() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before = runtime.scroll.global_scroll_top;

    assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
    assert!(runtime.queue_measured_height(2, 1, 72.0).unwrap());
    assert!(runtime.queue_measured_height(3, 1, 80.0).unwrap());
    assert!(runtime.flush_pending_height_corrections().unwrap());

    assert_eq!(
        runtime.scroll.global_scroll_top,
        before + 32.0 + 40.0 + 48.0
    );
}

#[test]
fn flush_discards_stale_measured_height_versions() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);

    assert!(runtime.queue_measured_height(3, 1, 96.0).unwrap());
    runtime.insert_char('!').unwrap();

    assert!(!runtime.flush_pending_height_corrections().unwrap());
    let block_index = runtime.index.index_of(3).unwrap();
    assert_ne!(
        runtime.index.layout_meta[block_index].measured_height,
        Some(96.0)
    );
}

#[test]
fn flush_below_viewport_heights_does_not_move_scroll_top() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before = runtime.scroll.global_scroll_top;

    assert!(runtime.queue_measured_height(900, 1, 64.0).unwrap());
    assert!(runtime.queue_measured_height(901, 1, 72.0).unwrap());
    assert!(runtime.flush_pending_height_corrections().unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before);
}

#[test]
fn wheel_scroll_height_flush_preserves_user_scroll_top_without_bounce() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    runtime.scroll_by_delta(-64.0).unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_total_height = runtime.height_index.total_height();

    assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
    assert!(
        runtime
            .flush_pending_height_corrections_with_priority(HeightCorrectionPriority::DeferRemote)
            .unwrap()
    );

    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    assert_eq!(
        runtime.height_index.total_height(),
        before_total_height + 32.0
    );
    assert_eq!(
        runtime.scroll.model_total_height,
        runtime.scroll_extent_height(before_total_height + 32.0)
    );
    assert!(runtime.pending_measured_heights.is_empty());
}

#[test]
fn scrollbar_drag_freezes_displayed_total_and_defers_anchor_restore() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let policy = ScrollbarPolicy {
        track_height: 720.0,
        ..ScrollbarPolicy::default()
    };
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_total_height = runtime.scroll.displayed_total_height;

    let visual = runtime.begin_scrollbar_drag(policy);
    assert!(visual.enabled);
    assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
    assert!(runtime.flush_pending_height_corrections().unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    assert_eq!(
        runtime.scroll.model_total_height,
        runtime.scroll_extent_height(before_total_height + 32.0)
    );
    assert_eq!(runtime.scroll.displayed_total_height, before_total_height);

    let end = runtime.finish_scrollbar_drag().unwrap().unwrap();
    assert_eq!(end.pending_layout_corrections, 1);
    assert_eq!(
        runtime.scroll.displayed_total_height,
        runtime.scroll.model_total_height
    );
}

#[test]
fn scrollbar_drag_uses_frozen_total_height_for_thumb_mapping() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    let policy = ScrollbarPolicy {
        track_height: 720.0,
        ..ScrollbarPolicy::default()
    };
    let visual = runtime.begin_scrollbar_drag(policy);
    let max_thumb_top = policy.track_height - visual.thumb_height;

    let update = runtime
        .drag_scrollbar_to_thumb_top(policy, max_thumb_top)
        .unwrap()
        .unwrap();

    assert_eq!(update.drag_ratio, 1.0);
    assert_eq!(
        runtime.scroll.global_scroll_top,
        runtime.scroll.max_scroll_top()
    );
    assert!(runtime.finish_scrollbar_drag().unwrap().is_some());
}

#[test]
fn rich_text_height_updates_after_wrap() {
    let long_text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            long_text,
        )],
        720.0,
    );
    let snapshot = runtime.projection_for_window().blocks[0].clone();
    let content_version = runtime.block_content_version(snapshot.block_id).unwrap();
    let measured_height = RichBlockRecord::DEFAULT_TEXT_HEIGHT * 2.0;

    let applied = runtime
        .apply_measured_height(snapshot.block_id, content_version, measured_height)
        .unwrap();

    assert!(applied);
    let updated = runtime.projection_for_window();
    assert_eq!(
        updated.blocks[0].layout.measured_height,
        Some(measured_height)
    );
    assert_eq!(runtime.height_index.total_height(), measured_height);
    assert_eq!(runtime.page_layout.total_height(), measured_height);
}

#[test]
fn document_runtime_scroll_by_delta_clamps_and_updates_page_window() {
    let mut runtime = runtime_with_paragraph_blocks(100_000);
    let initial_window = runtime.current_page_window();

    runtime.scroll_by_delta(50_000.0).unwrap();

    assert_eq!(runtime.scroll.global_scroll_top, 50_000.0);
    assert_ne!(runtime.current_page_window(), initial_window);

    runtime.scroll_by_delta(-100_000.0).unwrap();
    assert_eq!(runtime.scroll.global_scroll_top, 0.0);

    runtime.scroll_by_delta(10_000_000.0).unwrap();
    assert_eq!(
        runtime.scroll.global_scroll_top,
        runtime.scroll.max_scroll_top()
    );
}

#[test]
fn projection_window_spacer_heights_sum_to_total() {
    let mut runtime = runtime_with_paragraph_blocks(100_000);
    let middle_page = runtime.page_layout.page_count() / 2;
    let middle_offset = runtime.page_layout.offset_of_page(middle_page).unwrap();
    runtime
        .scroll
        .scroll_to_global_offset(
            middle_offset,
            cditor_editor::scroll::ScrollOrigin::ProgrammaticVirtualScroll,
        )
        .unwrap();

    let projection = runtime.projection_for_window();
    let projected_total = projection.before_window_height
        + projection.render_window.height()
        + projection.after_window_height;

    assert!(projection.before_window_height > 0.0);
    assert!(
        (projected_total - runtime.scroll_extent_height(runtime.page_layout.total_height())).abs()
            < 0.001
    );
}

#[test]
fn projection_for_window_limits_blocks_for_100k_document() {
    let runtime = runtime_with_paragraph_blocks(100_000);

    let projection = runtime.projection_for_window();

    assert_eq!(projection.total_visible_blocks, 100_000);
    assert!(projection.blocks.len() < 10_000);
    assert_eq!(
        projection.render_window.block_range.len(),
        projection.blocks.len()
    );
    assert_eq!(
        projection.render_window.page_range,
        runtime.current_page_window()
    );
    assert_eq!(projection.blocks.first().unwrap().visible_index, 0);
    assert_eq!(
        projection.blocks.last().unwrap().visible_index + 1,
        projection.render_window.block_range.end
    );
}
