use super::*;

#[test]
fn planned_window_hysteresis_keeps_boundary_window_stable() {
    let mut runtime = runtime_with_paragraph_blocks(3_000);
    runtime.window_planner = WindowPlanner::new(
        0,
        0,
        WindowPlannerPolicy {
            enter_threshold_viewports: 0.5,
            min_stable_frames_before_trim: 0,
            min_ms_between_window_commits: 0,
            ..WindowPlannerPolicy::default()
        },
    );
    let first_page_height = runtime.page_layout.pages[0].height;
    runtime
        .scroll
        .scroll_to_global_offset(
            first_page_height - 10.0,
            cditor_editor::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();
    let initial = runtime.current_page_window_planned();
    runtime
        .scroll
        .scroll_to_global_offset(
            first_page_height + 10.0,
            cditor_editor::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();
    let near_boundary = runtime.current_page_window_planned();

    assert_eq!(near_boundary, initial);
}

#[test]
fn planned_window_keeps_focused_page_pinned() {
    let mut runtime = runtime_with_paragraph_blocks(10_000);
    runtime.window_planner = WindowPlanner::new(0, 0, WindowPlannerPolicy::default());
    runtime.focus_block(1);
    let target_page = runtime.page_layout.page_count() - 1;
    let offset = runtime.page_layout.offset_of_page(target_page).unwrap();
    runtime
        .scroll
        .scroll_to_global_offset(offset, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();

    let planned = runtime.current_page_window_planned();
    let focused_page = runtime.page_layout.page_for_block_index(0).unwrap();
    assert!(planned.contains(&focused_page));
    assert!(planned.contains(&target_page));
}

#[test]
fn document_runtime_projects_v2_blocks_without_ui_truth() {
    let runtime = DocumentRuntime::demo();
    let projection = runtime.projection();
    assert_eq!(projection.total_visible_blocks, 4);
    assert_eq!(projection.blocks.len(), 4);
    assert_eq!(projection.blocks[0].block_id, 1);
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
}

#[test]
fn projection_for_window_exposes_total_visible_count_and_spacers() {
    let runtime = DocumentRuntime::demo();

    let projection = runtime.projection_for_window();

    assert_eq!(
        projection.total_visible_blocks,
        runtime.visible_index.total_visible_count()
    );
    assert_eq!(projection.before_window_height, 0.0);
    assert_eq!(projection.placeholder_window_height, None);
    assert_eq!(
        projection.after_window_height,
        projection.down_placer_height
    );
}

#[test]
fn scrollbar_drag_projects_the_target_placeholder_for_live_loading() {
    let records = (1..=1_000 as BlockId)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 32.0))
        })
        .collect::<Vec<_>>();
    let payloads = (1..=1_000 as BlockId)
        .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
        .collect::<Vec<_>>();
    let mut runtime = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
    let loaded = runtime.projection_for_window_planned();
    assert!(!loaded.render_window.is_placeholder());
    runtime.payload_window.block_range = 0..64;
    runtime
        .payload_window
        .payloads
        .retain(|block_id, _| *block_id <= 64);

    runtime
        .scroll
        .scroll_to_global_offset(20_000.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let policy = ScrollbarPolicy::default();
    runtime.begin_scrollbar_drag(policy);

    let target = runtime.projection_for_window_planned();

    assert!(target.render_window.is_placeholder());
    assert!(target.placeholder_window_height.is_some());
    assert!(target.blocks.is_empty());
    assert_ne!(
        target.render_window.block_range,
        loaded.render_window.block_range
    );
}

#[test]
fn projection_uses_placeholder_window_when_payload_window_is_not_loaded() {
    let records = (1..=1_000 as BlockId)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 32.0))
        })
        .collect::<Vec<_>>();
    let runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);

    let projection = runtime.projection_for_window();

    assert!(projection.render_window.is_placeholder());
    assert!(projection.blocks.is_empty());
    assert_eq!(
        projection.placeholder_window_height,
        Some(projection.render_window.height())
    );
    assert_eq!(
        projection.before_window_height
            + projection.placeholder_window_height.unwrap_or_default()
            + projection.after_window_height,
        runtime.scroll_extent_height(runtime.page_layout.total_height())
    );
}

#[test]
fn focus_block_at_offset_sets_caret_without_ui_truth() {
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

    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some(2));
    let projection = runtime.projection_for_window();
    assert_eq!(projection.blocks[0].caret_offset, Some(2));
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, 2..2);
    assert_eq!(editing.marked_range, None);
}
