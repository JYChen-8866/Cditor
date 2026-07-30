use super::*;
use crate::content::payload_window::MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS;

#[test]
fn small_document_requires_the_complete_render_window_before_commit() {
    let mut runtime = runtime_with_paragraph_blocks(24);

    let projection = runtime.projection_for_window_planned();

    assert_eq!(projection.render_window.block_range, 0..24);
    assert_eq!(
        projection.payload_visible_block_range,
        projection.render_window.block_range
    );
}

#[test]
fn large_document_uses_a_bounded_atomic_render_window() {
    let mut runtime = runtime_with_paragraph_blocks(3_000);

    let projection = runtime.projection_for_window_planned();

    assert!(projection.render_window.block_range.len() <= 320);
    assert_eq!(
        projection.payload_visible_block_range,
        projection.render_window.block_range
    );
}

fn assert_loaded_versions_do_not_regress(
    previous: &EditorViewProjection,
    next: &EditorViewProjection,
) {
    let next_blocks = next
        .blocks
        .iter()
        .map(|block| (block.block_id, block))
        .collect::<HashMap<_, _>>();
    for previous_block in &previous.blocks {
        let BlockPayloadView::Loaded(previous_payload) = &previous_block.payload else {
            continue;
        };
        let Some(next_block) = next_blocks.get(&previous_block.block_id) else {
            continue;
        };
        let BlockPayloadView::Loaded(next_payload) = &next_block.payload else {
            panic!(
                "loaded block {} regressed to placeholder",
                previous_block.block_id
            );
        };
        assert!(next_payload.content_version >= previous_payload.content_version);
    }
}

#[test]
fn committed_projection_survives_payload_cache_loss_without_skeletons() {
    let mut runtime = runtime_with_paragraph_blocks(24);
    let committed = runtime.projection_for_window_planned();
    assert!(committed.blocks.iter().all(|block| !block.placeholder));

    let evicted_ids = committed
        .blocks
        .iter()
        .map(|block| block.block_id)
        .filter(|block_id| *block_id != 1)
        .collect::<Vec<_>>();
    for block_id in evicted_ids {
        runtime.document.payload_window.payloads.remove(&block_id);
    }

    let retained = runtime.projection_for_window_planned();

    assert_eq!(retained.blocks.len(), committed.blocks.len());
    assert!(retained.blocks.iter().all(|block| !block.placeholder));
    assert_eq!(
        retained
            .blocks
            .iter()
            .map(|block| block.block_id)
            .collect::<Vec<_>>(),
        committed
            .blocks
            .iter()
            .map(|block| block.block_id)
            .collect::<Vec<_>>()
    );
    assert_loaded_versions_do_not_regress(&committed, &retained);
}

#[test]
fn projection_lifecycle_randomized_actions_never_downgrade_loaded_blocks() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    let mut stable = runtime.projection_for_window_planned();
    let mut seed = 0x5eed_cafe_f00d_u64;

    for step in 0..512 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        match seed % 4 {
            0 => {
                let block_ids = stable
                    .blocks
                    .iter()
                    .map(|block| block.block_id)
                    .filter(|block_id| block_id % 3 == step % 3)
                    .collect::<Vec<_>>();
                for block_id in block_ids {
                    runtime.document.payload_window.payloads.remove(&block_id);
                }
            }
            1 => {
                let max_scroll = runtime.layout.scroll.max_scroll_top();
                let target = (seed as f64 % max_scroll.max(1.0)).min(max_scroll);
                runtime
                    .layout
                    .scroll
                    .scroll_to_global_offset(
                        target,
                        cditor_viewport::scroll::ScrollOrigin::UserWheel,
                    )
                    .unwrap();
            }
            2 => {
                let _ = runtime.trim_payload_cache(
                    crate::PayloadCachePolicy {
                        max_entries: 64,
                        max_estimated_bytes: usize::MAX,
                    },
                    [],
                );
            }
            _ => {}
        }

        let next = runtime.projection_for_window_planned();
        assert_loaded_versions_do_not_regress(&stable, &next);
        assert!(next.render_window.block_range.len() <= 320);
        stable = next;
    }
}

#[test]
fn ime_composition_refreshes_the_focused_block_without_downgrading_peers() {
    let mut runtime = runtime_with_paragraph_blocks(24);
    runtime.focus_block(1);
    let stable = runtime.projection_for_window_planned();

    runtime.begin_or_update_composition(1, 0..0, "中").unwrap();
    let preview = runtime.projection_for_window_planned();

    assert_loaded_versions_do_not_regress(&stable, &preview);
    assert!(preview.blocks.iter().all(|block| !block.placeholder));
    let focused = preview
        .blocks
        .iter()
        .find(|block| block.block_id == 1)
        .unwrap();
    assert_eq!(focused.marked_range, Some(0.."中".len()));
}

#[test]
fn selection_refresh_does_not_downgrade_any_loaded_block() {
    let mut runtime = runtime_with_paragraph_blocks(24);
    let stable = runtime.projection_for_window_planned();

    runtime
        .set_document_selection(DocumentSelection::caret(TextPosition::downstream(12, 0)))
        .unwrap();
    let selected = runtime.projection_for_window_planned();

    assert_loaded_versions_do_not_regress(&stable, &selected);
    assert!(selected.blocks.iter().all(|block| !block.placeholder));
    assert!(
        selected
            .blocks
            .iter()
            .any(|block| block.block_id == 12 && block.focused)
    );
}

#[test]
fn inserting_and_deleting_blocks_recommit_without_skeleton_gaps() {
    let mut runtime = runtime_with_paragraph_blocks(24);
    let before_insert = runtime.projection_for_window_planned();

    let inserted_id = runtime.insert_paragraph_after_block(12).unwrap();
    let after_insert = runtime.projection_for_window_planned();
    assert!(after_insert.blocks.iter().all(|block| !block.placeholder));
    assert!(
        after_insert
            .blocks
            .iter()
            .any(|block| block.block_id == inserted_id)
    );
    assert_loaded_versions_do_not_regress(&before_insert, &after_insert);

    assert!(runtime.delete_block_by_id(inserted_id).unwrap());
    let after_delete = runtime.projection_for_window_planned();
    assert!(after_delete.blocks.iter().all(|block| !block.placeholder));
    assert!(
        after_delete
            .blocks
            .iter()
            .all(|block| block.block_id != inserted_id)
    );
    assert_loaded_versions_do_not_regress(&before_insert, &after_delete);
}

#[test]
fn local_insert_commits_loaded_without_downgrading_existing_blocks() {
    let mut runtime = runtime_with_paragraph_blocks(24);
    let stable = runtime.projection_for_window_planned();

    let inserted_id = runtime.insert_paragraph_after_block(1).unwrap();
    let inserted = runtime.projection_for_window_planned();

    assert_loaded_versions_do_not_regress(&stable, &inserted);
    assert!(inserted.blocks.iter().all(|block| !block.placeholder));
    assert!(
        inserted
            .blocks
            .iter()
            .any(|block| block.block_id == inserted_id)
    );
}

#[test]
fn local_delete_commits_without_skeleton_gap() {
    let mut runtime = runtime_with_paragraph_blocks(24);
    let stable = runtime.projection_for_window_planned();

    assert!(runtime.delete_block_by_id(12).unwrap());
    let deleted = runtime.projection_for_window_planned();

    assert_loaded_versions_do_not_regress(&stable, &deleted);
    assert!(deleted.blocks.iter().all(|block| !block.placeholder));
    assert!(deleted.blocks.iter().all(|block| block.block_id != 12));
}

#[test]
fn structure_move_reconciles_loaded_blocks_by_id_without_skeletons() {
    let mut runtime = runtime_with_paragraph_blocks(24);
    let stable = runtime.projection_for_window_planned();

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
    let moved = runtime.projection_for_window_planned();

    assert_loaded_versions_do_not_regress(&stable, &moved);
    assert!(moved.blocks.iter().all(|block| !block.placeholder));
    let ids = moved
        .blocks
        .iter()
        .map(|block| block.block_id)
        .collect::<Vec<_>>();
    assert_eq!(&ids[0..4], &[2, 3, 1, 4]);
}

#[test]
fn fold_and_unfold_keep_remaining_and_restored_blocks_loaded() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::Heading { level: 1 }, 0, None),
        (RichBlockKind::Paragraph, 1, Some(1)),
        (RichBlockKind::Paragraph, 1, Some(1)),
        (RichBlockKind::Paragraph, 0, None),
    ]);
    let expanded = runtime.projection_for_window_planned();

    assert!(runtime.toggle_block_fold(1).unwrap());
    let folded = runtime.projection_for_window_planned();
    assert_loaded_versions_do_not_regress(&expanded, &folded);
    assert!(folded.blocks.iter().all(|block| !block.placeholder));

    assert!(runtime.toggle_block_fold(1).unwrap());
    let restored = runtime.projection_for_window_planned();
    assert_loaded_versions_do_not_regress(&folded, &restored);
    assert!(restored.blocks.iter().all(|block| !block.placeholder));
    assert_eq!(restored.blocks.len(), 4);
}

#[test]
fn planned_window_hysteresis_keeps_boundary_window_stable() {
    let mut runtime = runtime_with_paragraph_blocks(3_000);
    runtime.layout.window_planner = WindowPlanner::new(
        0,
        0,
        WindowPlannerPolicy {
            enter_threshold_viewports: 0.5,
            min_stable_frames_before_trim: 0,
            min_ms_between_window_commits: 0,
            ..WindowPlannerPolicy::default()
        },
    );
    let first_page_height = runtime.layout.page_layout.pages[0].height;
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(
            first_page_height - 10.0,
            cditor_viewport::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();
    let initial = runtime.current_page_window_planned();
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(
            first_page_height + 10.0,
            cditor_viewport::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();
    let near_boundary = runtime.current_page_window_planned();

    assert_eq!(near_boundary, initial);
}

#[test]
fn planned_window_keeps_focused_page_pinned() {
    let mut runtime = runtime_with_paragraph_blocks(10_000);
    runtime.layout.window_planner = WindowPlanner::new(0, 0, WindowPlannerPolicy::default());
    runtime.focus_block(1);
    let target_page = runtime.layout.page_layout.page_count() - 1;
    let offset = runtime
        .layout
        .page_layout
        .offset_of_page(target_page)
        .unwrap();
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(offset, cditor_viewport::scroll::ScrollOrigin::UserWheel)
        .unwrap();

    let planned = runtime.current_page_window_planned();
    let focused_page = runtime.layout.page_layout.page_for_block_index(0).unwrap();
    assert!(planned.contains(&focused_page));
    assert!(planned.contains(&target_page));
}

#[test]
fn planned_projection_separates_render_payload_and_layout_prefetch_ranges() {
    let mut runtime = runtime_with_paragraph_blocks(3_500);
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(
            runtime.layout.height_index.offset_of_block(2_000).unwrap(),
            cditor_viewport::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();

    let normal = runtime.projection_for_window_planned();
    assert!(
        normal
            .payload_prefetch_block_range
            .contains(&normal.render_window.block_range.start)
    );
    assert!(normal.payload_prefetch_block_range.end >= normal.render_window.block_range.end);
    assert!(normal.payload_prefetch_block_range.len() > normal.render_window.block_range.len());
    assert!(
        normal
            .payload_visible_block_range
            .start
            .ge(&normal.render_window.block_range.start)
    );
    assert!(normal.payload_visible_block_range.end <= normal.render_window.block_range.end);
    assert!(
        normal.layout_prefetch_page_range.start <= normal.render_window.page_range.start
            && normal.layout_prefetch_page_range.end >= normal.render_window.page_range.end
    );

    runtime.set_window_memory_pressure(WindowMemoryPressure::Critical);
    let critical = runtime.projection_for_window_planned();
    assert_eq!(
        critical.payload_prefetch_block_range,
        critical.render_window.block_range
    );
    assert!(
        critical.layout_prefetch_page_range.start <= critical.render_window.page_range.start
            && critical.layout_prefetch_page_range.end >= critical.render_window.page_range.end
    );
}

#[test]
fn planned_projection_sizes_render_overscan_by_viewport_height() {
    let mut runtime = runtime_with_paragraph_blocks(10_000);
    let target_index = 2_000;
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(
            runtime
                .layout
                .height_index
                .offset_of_block(target_index)
                .unwrap(),
            cditor_viewport::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();

    let projection = runtime.projection_for_window_planned();

    // Paragraph fixtures are 32px tall and the test viewport is 720px. One
    // viewport of overscan on each side therefore keeps 69 blocks resident,
    // instead of paying a fixed 48-block cost on both sides.
    assert_eq!(projection.render_window.block_range, 1_977..2_046);
    assert_eq!(projection.blocks.len(), 69);
    assert!(projection.render_window.block_range.contains(&target_index));
    assert!(
        projection.render_window.block_range.start <= projection.payload_visible_block_range.start
            && projection.payload_visible_block_range.end
                <= projection.render_window.block_range.end
    );
}

#[test]
fn scrollbar_foreground_range_guards_the_complete_render_window() {
    let mut runtime = runtime_with_paragraph_blocks(10_000);
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(
            runtime.layout.height_index.offset_of_block(2_000).unwrap(),
            cditor_viewport::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();
    let render_range = runtime
        .projection_for_window_planned()
        .render_window
        .block_range;
    runtime.begin_scrollbar_drag(ScrollbarPolicy::default());

    let foreground_range = runtime.current_foreground_payload_range();

    assert_eq!(foreground_range.start, render_range.start - 2);
    assert_eq!(
        foreground_range.end,
        (render_range.end + 2).min(runtime.document.visible_index.total_visible_count())
    );
    assert!(foreground_range.start <= render_range.start);
    assert!(foreground_range.end >= render_range.end);

    let mut edge_runtime = runtime_with_paragraph_blocks(10_000);
    let policy = ScrollbarPolicy::default();
    edge_runtime.begin_scrollbar_drag(policy);
    assert_eq!(edge_runtime.current_foreground_payload_range().start, 0);
    edge_runtime
        .drag_scrollbar_to_ratio(policy, 1.0)
        .unwrap()
        .unwrap();
    assert_eq!(edge_runtime.current_foreground_payload_range().end, 10_000);
}

#[test]
fn critical_memory_pressure_removes_render_overscan_but_keeps_visible_core() {
    let mut runtime = runtime_with_paragraph_blocks(10_000);
    let target_index = 2_000;
    runtime.set_window_memory_pressure(WindowMemoryPressure::Critical);
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(
            runtime
                .layout
                .height_index
                .offset_of_block(target_index)
                .unwrap(),
            cditor_viewport::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();

    let projection = runtime.projection_for_window_planned();

    assert_eq!(
        projection.render_window.block_range,
        projection.payload_visible_block_range
    );
    assert_eq!(projection.render_window.block_range, 2_000..2_023);
}

#[test]
fn zero_height_layout_history_cannot_expand_the_visible_payload_core_past_the_window_bound() {
    let records = (1..=1_000 as BlockId)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 0.0))
        })
        .collect::<Vec<_>>();
    let payloads = (1..=1_000 as BlockId)
        .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
        .collect::<Vec<_>>();
    let mut runtime = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);

    let projection = runtime.projection_for_window_planned();

    assert!(projection.render_window.block_range.len() <= 320);
    assert!(projection.payload_visible_block_range.len() <= 320);
    assert!(
        projection.render_window.block_range.start <= projection.payload_visible_block_range.start
            && projection.payload_visible_block_range.end
                <= projection.render_window.block_range.end
    );
}

#[test]
fn document_runtime_projects_v2_blocks_without_ui_truth() {
    let runtime = DocumentRuntime::demo();
    let projection = runtime.full_projection_for_tests();
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
        runtime.document.visible_index.total_visible_count()
    );
    assert_eq!(projection.before_window_height, 0.0);
    assert_eq!(projection.placeholder_window_height, None);
    assert_eq!(
        projection.after_window_height,
        projection.down_placer_height
    );
}

#[test]
fn remote_target_keeps_the_stable_projection_until_visible_payloads_are_ready() {
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
    let stable_block_ids = loaded
        .blocks
        .iter()
        .map(|block| block.block_id)
        .collect::<HashSet<_>>();
    runtime.document.payload_window.block_range = loaded.render_window.block_range.clone();
    runtime
        .document
        .payload_window
        .payloads
        .retain(|block_id, _| stable_block_ids.contains(block_id));

    runtime
        .layout
        .scroll
        .scroll_to_global_offset(20_000.0, cditor_viewport::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let policy = ScrollbarPolicy::default();
    runtime.begin_scrollbar_drag(policy);

    let preparing = runtime.projection_for_window_planned();

    assert!(!preparing.render_window.is_placeholder());
    assert_eq!(preparing.placeholder_window_height, None);
    assert_eq!(
        preparing.render_window.block_range,
        loaded.render_window.block_range
    );
    assert_eq!(
        preparing.scroll.global_scroll_top,
        loaded.scroll.global_scroll_top
    );
    assert_ne!(
        preparing.payload_visible_block_range,
        preparing.render_window.block_range
    );
    assert!(
        preparing
            .blocks
            .iter()
            .all(|block| matches!(block.payload, BlockPayloadView::Loaded(_)))
    );

    let request = runtime
        .plan_payload_window_load_if_needed(preparing.payload_visible_block_range.clone())
        .expect("remote visible core needs payloads");
    assert_eq!(request.block_range, preparing.payload_visible_block_range);
    let records = request
        .block_ids
        .iter()
        .map(|block_id| {
            BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "loaded")
        })
        .collect();
    runtime.apply_payload_window_result(prepared_payload_result(request, records, Vec::new()));

    let committed = runtime.projection_for_window_planned();
    assert!(!committed.render_window.is_placeholder());
    assert_ne!(
        committed.render_window.block_range,
        loaded.render_window.block_range
    );
    assert_eq!(
        committed.render_window.block_range,
        preparing.payload_visible_block_range
    );
    assert_eq!(
        committed.scroll.global_scroll_top,
        runtime.layout.scroll.global_scroll_top
    );
    assert!(committed.payload_visible_block_range.clone().all(|index| {
        let block_id = runtime
            .document
            .visible_index
            .id_at_visible_index(index)
            .unwrap();
        runtime.document.payload_window.get(block_id).is_some()
    }));
    assert!(committed.blocks.iter().all(|block| !block.placeholder));
}

#[test]
fn failed_remote_target_is_explicit_while_the_last_stable_window_remains_recoverable() {
    let records = (1..=256 as BlockId)
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
    let payloads = (1..=256 as BlockId)
        .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
        .collect::<Vec<_>>();
    let mut runtime = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
    let stable = runtime.projection_for_window_planned();
    let stable_block_ids = stable
        .blocks
        .iter()
        .map(|block| block.block_id)
        .collect::<HashSet<_>>();
    runtime
        .document
        .payload_window
        .payloads
        .retain(|block_id, _| stable_block_ids.contains(block_id));

    runtime
        .layout
        .scroll
        .scroll_to_global_offset(
            6_000.0,
            cditor_viewport::scroll::ScrollOrigin::UserScrollbar,
        )
        .unwrap();
    let preparing = runtime.projection_for_window_planned();
    assert_eq!(
        preparing.render_window.block_range,
        stable.render_window.block_range
    );
    for attempt in 1..=MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS {
        let request = runtime
            .plan_payload_window_load_if_needed(preparing.payload_visible_block_range.clone())
            .expect("retryable remote target needs a visible payload request");
        runtime.apply_payload_window_load_error(request, "sqlite read failed");
        if attempt < MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS {
            let retrying = runtime.projection_for_window_planned();
            assert_eq!(
                retrying.render_window.block_range,
                stable.render_window.block_range
            );
            assert!(!retrying.render_window.is_placeholder());
            assert!(retrying.placeholder_window_failure.is_none());
        }
    }

    let failed = runtime.projection_for_window_planned();
    assert!(!failed.render_window.is_placeholder());
    assert_eq!(
        failed.render_window.block_range,
        stable.render_window.block_range
    );
    assert_eq!(
        failed.placeholder_window_error.as_deref(),
        Some("sqlite read failed")
    );
    assert_eq!(
        failed
            .placeholder_window_failure
            .as_ref()
            .map(|failure| failure.attempts),
        Some(MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS)
    );
    assert!(runtime.layout.projection_window.stable().is_some());
    assert!(runtime.layout.projection_window.preparing().is_none());

    assert!(runtime.retry_failed_payload_window(failed.payload_visible_block_range.clone()) > 0);
    let retrying = runtime.projection_for_window_planned();
    assert_eq!(
        retrying.render_window.block_range, stable.render_window.block_range,
        "retry returns to the last stable presentation until the target is ready"
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
        runtime.scroll_extent_height(runtime.layout.page_layout.total_height())
    );
}

#[test]
fn cold_projection_keeps_an_explicit_error_after_payload_load_failure() {
    let records = (1..=100 as BlockId)
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
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);
    let cold = runtime.projection_for_window_planned();
    for _ in 0..MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS {
        let request = runtime
            .plan_payload_window_load_if_needed(cold.payload_visible_block_range.clone())
            .expect("retryable cold visible core needs payloads");
        runtime.apply_payload_window_load_error(request, "sqlite read failed");
    }

    let failed = runtime.projection_for_window_planned();
    assert!(failed.render_window.is_placeholder());
    assert!(failed.blocks.is_empty());
    assert_eq!(
        failed.placeholder_window_error.as_deref(),
        Some("sqlite read failed")
    );
    assert_eq!(
        failed
            .placeholder_window_failure
            .as_ref()
            .map(|failure| failure.attempts),
        Some(MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS)
    );
    assert_eq!(
        failed.payload_visible_block_range, cold.payload_visible_block_range,
        "terminal failure, scheduler suppression, and explicit retry share one visible core"
    );
    assert!(runtime.retry_failed_payload_window(failed.payload_visible_block_range.clone()) > 0);
    assert!(
        runtime
            .projection_for_window_planned()
            .placeholder_window_failure
            .is_none(),
        "explicit retry releases suppression for the same visible core"
    );
}

#[test]
fn terminal_failure_in_atomic_render_window_blocks_cold_commit() {
    let records = (1..=100 as BlockId)
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
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);
    let cold = runtime.projection_for_window_planned();
    let failed_index = cold.payload_visible_block_range.end.saturating_sub(1);

    assert!(cold.render_window.block_range.contains(&failed_index));
    assert!(cold.payload_visible_block_range.contains(&failed_index));
    let failed_block_id = runtime
        .document
        .visible_index
        .id_at_visible_index(failed_index)
        .unwrap();
    for _ in 0..MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS {
        runtime
            .document
            .payload_window
            .mark_failed(failed_block_id, "render window sqlite read failed");
    }

    let still_cold = runtime.projection_for_window_planned();
    assert!(still_cold.render_window.is_placeholder());
    assert!(still_cold.placeholder_window_failure.is_some());
    assert_eq!(
        still_cold.payload_visible_block_range,
        cold.payload_visible_block_range
    );
    let visible_request = runtime
        .plan_payload_window_load_if_needed(still_cold.payload_visible_block_range.clone())
        .expect("resident peers may continue loading around a terminally failed block");
    assert!(!visible_request.block_ids.contains(&failed_block_id));
    let records = visible_request
        .block_ids
        .iter()
        .map(|block_id| {
            BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "loaded")
        })
        .collect();
    runtime.apply_payload_window_result(prepared_payload_result(
        visible_request,
        records,
        Vec::new(),
    ));

    let blocked = runtime.projection_for_window_planned();
    assert!(blocked.render_window.is_placeholder());
    assert!(blocked.placeholder_window_failure.is_some());

    assert_eq!(
        runtime.retry_failed_payload_window(blocked.payload_visible_block_range.clone()),
        1
    );
    let retry = runtime
        .plan_payload_window_load_if_needed(blocked.payload_visible_block_range.clone())
        .expect("explicit retry includes the failed atomic-window block");
    assert!(retry.block_ids.contains(&failed_block_id));
    let retry_records = retry
        .block_ids
        .iter()
        .map(|block_id| {
            BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "retried")
        })
        .collect();
    runtime.apply_payload_window_result(prepared_payload_result(retry, retry_records, Vec::new()));

    let committed = runtime.projection_for_window_planned();
    assert!(!committed.render_window.is_placeholder());
    assert!(committed.placeholder_window_failure.is_none());
    assert!(committed.blocks.iter().all(|block| !block.placeholder));
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
    let editing = runtime.editing.session.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, 2..2);
    assert_eq!(editing.marked_range, None);
}

#[test]
fn repeated_projection_shares_large_code_and_mutation_is_copy_on_write() {
    let mut runtime = runtime_with_paragraph_blocks(1);
    let original_bytes = 10 * 1024 * 1024;
    runtime
        .document
        .payload_window
        .insert_loaded(BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code { language: None },
            payload: BlockPayload::Code {
                language: None,
                text: "x".repeat(original_bytes),
            },
        });

    let first = runtime.projection_for_window();
    let second = runtime.projection_for_window();
    let BlockPayloadView::Loaded(first_payload) = &first.blocks[0].payload else {
        panic!("large code payload must be resident");
    };
    let BlockPayloadView::Loaded(second_payload) = &second.blocks[0].payload else {
        panic!("large code payload must be resident");
    };
    assert!(Arc::ptr_eq(first_payload, second_payload));

    let resident = runtime
        .document
        .payload_window
        .get_mut(1)
        .expect("large code payload remains resident");
    resident.content_version = 2;
    let BlockPayload::Code { text, .. } = &mut resident.payload else {
        panic!("expected code payload");
    };
    text.push('!');

    let after_edit = runtime.projection_for_window();
    let BlockPayloadView::Loaded(after_edit_payload) = &after_edit.blocks[0].payload else {
        panic!("edited code payload must remain resident");
    };
    assert!(!Arc::ptr_eq(first_payload, after_edit_payload));
    let BlockPayload::Code { text: old_text, .. } = &first_payload.payload else {
        panic!("first projection keeps code payload");
    };
    let BlockPayload::Code { text: new_text, .. } = &after_edit_payload.payload else {
        panic!("edited projection keeps code payload");
    };
    assert_eq!(old_text.len(), original_bytes);
    assert_eq!(new_text.len(), original_bytes + 1);
}

#[test]
fn repeated_projection_shares_large_table_and_cell_spans() {
    use cditor_core::rich_text::{
        TableCellPayload, TableColumnPayload, TableHeaderStyle, TablePayload, TableRowPayload,
    };

    let mut runtime = runtime_with_paragraph_blocks(1);
    let rows = (0..256)
        .map(|row| TableRowPayload {
            cells: (0..32)
                .map(|col| TableCellPayload::plain(format!("cell-{row}-{col}-{}", "x".repeat(64))))
                .collect(),
            height: Default::default(),
        })
        .collect();
    runtime
        .document
        .payload_window
        .insert_loaded(BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(TablePayload {
                rows,
                columns: (0..32).map(|_| TableColumnPayload::default()).collect(),
                header_rows: 1,
                header_cols: 1,
                header_style: TableHeaderStyle::default(),
            }),
        });

    let first = runtime.projection_for_window();
    let second = runtime.projection_for_window();
    let first_table = first.blocks[0]
        .table_view
        .as_ref()
        .expect("large table is projected");
    let second_table = second.blocks[0]
        .table_view
        .as_ref()
        .expect("large table is projected again");

    assert!(first_table.table.shares_storage_with(&second_table.table));
    assert_eq!(first_table.visible_cells.len(), 256 * 32);
    assert_eq!(second_table.visible_cells.len(), 256 * 32);
    let first_text = &first_table.visible_cells[0].spans[0].text;
    let second_text = &second_table.visible_cells[0].spans[0].text;
    assert_eq!(first_text.as_ptr(), second_text.as_ptr());
}
