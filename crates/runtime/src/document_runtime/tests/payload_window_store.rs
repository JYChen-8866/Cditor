use super::*;

#[test]
fn planned_payload_window_without_records_does_not_render_per_block_placeholders() {
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
    let payloads = (1..=64 as BlockId)
        .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, payloads, 1, 720.0, 0..64);
    runtime.plan_payload_window_load(400..430);
    runtime
        .scroll
        .scroll_to_global_offset(400.0 * 32.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();

    let projection = runtime.projection_for_window_planned();

    assert!(projection.render_window.is_placeholder());
    assert!(projection.blocks.is_empty());
    assert!(projection.placeholder_window_height.is_some());
}

#[test]
fn payload_window_store_request_prioritizes_focus_and_selection_endpoints() {
    let mut runtime = runtime_with_paragraph_blocks(10);
    runtime.focus_block(5);
    runtime.select_all_visible_blocks();

    let request = runtime.plan_payload_window_load(3..6);

    assert_eq!(request.generation, 1);
    assert_eq!(request.block_range, 3..6);
    assert_eq!(&request.block_ids[..3], &[5, 1, 10]);
    assert!(request.block_ids.contains(&4));
    assert!(request.block_ids.contains(&6));
}

#[test]
fn payload_window_store_discards_stale_generation_result() {
    let mut runtime = runtime_with_paragraph_blocks(4);
    let stale = runtime.plan_payload_window_load(0..2);
    let current = runtime.plan_payload_window_load(2..4);
    assert_eq!(current.generation, 2);

    let decision = runtime.apply_payload_window_result(PayloadWindowLoadResult {
        request: stale,
        records: Vec::new(),
        missing_block_ids: Vec::new(),
    });

    assert_eq!(
        decision,
        PayloadWindowApplyDecision::DiscardedStaleGeneration {
            expected: 2,
            actual: 1,
        }
    );
    assert_eq!(runtime.payload_window.block_range, 2..4);
}

#[test]
fn stale_viewport_result_populates_cache_and_releases_its_loading_markers() {
    let records = (1..=6)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
        })
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);
    let stale = runtime.plan_payload_window_load(0..2);
    let current = runtime.plan_payload_window_load(4..6);

    let decision = runtime.apply_payload_window_result(PayloadWindowLoadResult {
        request: stale,
        records: vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "one"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "two"),
        ],
        missing_block_ids: Vec::new(),
    });

    assert_eq!(
        decision,
        PayloadWindowApplyDecision::DiscardedStaleGeneration {
            expected: current.generation,
            actual: current.generation - 1,
        }
    );
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "one");
    assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), "two");
    assert!(!runtime.payload_window.loading.contains(&1));
    assert!(!runtime.payload_window.loading.contains(&2));
    assert!(runtime.payload_window.loading.contains(&5));
    assert!(runtime.payload_window.loading.contains(&6));
}

#[test]
fn stale_result_cannot_clear_or_overwrite_a_newer_request_for_the_same_block() {
    let records = (1..=3)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
        })
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);
    let stale = runtime.plan_payload_window_load(0..2);
    let current = runtime.plan_payload_window_load(1..3);

    runtime.apply_payload_window_result(PayloadWindowLoadResult {
        request: stale,
        records: vec![BlockPayloadRecord::rich_text(
            2,
            RichBlockKind::Paragraph,
            "stale",
        )],
        missing_block_ids: Vec::new(),
    });

    assert!(runtime.payload_window.loading.contains(&2));
    assert!(runtime.payload_window.get(2).is_none());

    runtime.apply_payload_window_result(PayloadWindowLoadResult {
        request: current,
        records: vec![BlockPayloadRecord::rich_text(
            2,
            RichBlockKind::Paragraph,
            "current",
        )],
        missing_block_ids: Vec::new(),
    });
    assert_eq!(
        runtime.payload_window.get(2).unwrap().plain_text(),
        "current"
    );
}

#[test]
fn all_in_flight_blocks_keep_their_generation_until_the_request_finishes() {
    let records = (1..=4)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
        })
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);
    let request = runtime
        .plan_payload_window_load_if_needed(0..4)
        .expect("initial window needs a request");

    assert!(runtime.plan_payload_window_load_if_needed(1..3).is_none());
    assert_eq!(runtime.payload_window_generation(), request.generation);

    runtime.apply_payload_window_result(PayloadWindowLoadResult {
        records: request
            .block_ids
            .iter()
            .map(|block_id| {
                BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "loaded")
            })
            .collect(),
        request,
        missing_block_ids: Vec::new(),
    });
    assert!(runtime.payload_window.loading.is_empty());
    assert!(runtime.plan_payload_window_load_if_needed(1..3).is_none());
    assert_eq!(runtime.payload_window.block_range, 1..3);
}

#[test]
fn revisiting_a_resident_window_activates_it_without_a_database_request() {
    let records = (1..=8)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
        })
        .collect::<Vec<_>>();
    let payloads = (1..=8)
        .map(|block_id| {
            BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "resident")
        })
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, payloads, 1, 720.0, 4..8);

    assert!(runtime.activate_payload_window_if_resident(0..4));
    assert_eq!(runtime.payload_window.block_range, 0..4);
    assert!(runtime.plan_payload_window_load_if_needed(0..4).is_none());
    assert!(!runtime.activate_payload_window_if_resident(0..4));
}

#[test]
fn payload_window_store_marks_loading_and_missing_payload_errors() {
    let records = (1..=3)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
        })
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);

    let request = runtime.plan_payload_window_load(0..2);
    assert!(runtime.payload_window.loading.contains(&1));
    assert!(runtime.payload_window.loading.contains(&2));

    let decision = runtime.apply_payload_window_result(PayloadWindowLoadResult {
        request,
        records: Vec::new(),
        missing_block_ids: vec![1, 2],
    });

    assert_eq!(decision, PayloadWindowApplyDecision::Applied);
    assert!(runtime.payload_window.loading.is_empty());
    assert!(runtime.payload_window.failed.contains_key(&1));
    assert!(runtime.payload_window.failed.contains_key(&2));
}

#[test]
fn payload_window_store_deduplicates_an_in_flight_viewport_request() {
    let records = (1..=100)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
        })
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);

    let first = runtime
        .plan_payload_window_load_if_needed(20..40)
        .expect("first viewport needs a load");
    let duplicate = runtime.plan_payload_window_load_if_needed(20..40);

    assert_eq!(first.block_range, 20..40);
    assert_eq!(first.block_ids.len(), 20);
    assert!(duplicate.is_none());
    assert_eq!(runtime.payload_window_generation(), 1);
}

#[test]
fn payload_window_store_retries_failures_but_stops_after_the_limit() {
    let records = (1..=2)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
        })
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);

    for attempt in 1..=3 {
        let request = runtime
            .plan_payload_window_load_if_needed(0..2)
            .expect("failure remains retryable before the limit");
        runtime.apply_payload_window_load_error(request, format!("attempt {attempt}"));
    }

    assert!(runtime.plan_payload_window_load_if_needed(0..2).is_none());
    assert_eq!(runtime.payload_window.failure_attempts.get(&1), Some(&3));
    assert_eq!(
        runtime.payload_window.failed.get(&1).map(String::as_str),
        Some("attempt 3")
    );
}

#[test]
fn planned_window_load_replaces_bounded_placeholder_without_full_hydration() {
    let records = (1..=10_000 as BlockId)
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
    let initial_payloads = (1..=64 as BlockId)
        .map(|block_id| {
            BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "initial")
        })
        .collect::<Vec<_>>();
    let mut runtime = DocumentRuntime::from_index_records_with_window(
        1,
        records,
        initial_payloads,
        1,
        720.0,
        0..64,
    );
    runtime
        .scroll
        .scroll_to_global_offset(160_000.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();

    let placeholder = runtime.projection_for_window_planned();
    assert!(placeholder.render_window.is_placeholder());
    assert!(placeholder.render_window.block_range.len() <= 320);
    assert_eq!(
        placeholder.placeholder_window_height,
        Some(placeholder.render_window.block_range.len() as f64 * 32.0)
    );

    let request = runtime
        .plan_payload_window_load_if_needed(placeholder.render_window.block_range.clone())
        .expect("remote viewport must be loaded");
    let records = request
        .block_ids
        .iter()
        .map(|block_id| {
            BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "loaded")
        })
        .collect();
    runtime.apply_payload_window_result(PayloadWindowLoadResult {
        request,
        records,
        missing_block_ids: Vec::new(),
    });

    let loaded = runtime.projection_for_window_planned();
    assert!(!loaded.render_window.is_placeholder());
    assert!(loaded.blocks.len() <= 320);
    assert!(runtime.payload_window.payloads.len() < 500);
}

#[test]
fn rapid_remote_scroll_accepts_out_of_order_windows_without_blank_lockup() {
    let records = (1..=10_000 as BlockId)
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

    runtime
        .scroll
        .scroll_to_global_offset(80_000.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let first_projection = runtime.projection_for_window_planned();
    let first_range = first_projection.render_window.block_range.clone();
    let first_request = runtime
        .plan_payload_window_load_if_needed(first_range.clone())
        .unwrap();

    runtime
        .scroll
        .scroll_to_global_offset(240_000.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let final_projection = runtime.projection_for_window_planned();
    let final_range = final_projection.render_window.block_range.clone();
    let final_request = runtime
        .plan_payload_window_load_if_needed(final_range.clone())
        .unwrap();

    let stale_records = first_request
        .block_ids
        .iter()
        .map(|block_id| BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "first"))
        .collect();
    assert!(matches!(
        runtime.apply_payload_window_result(PayloadWindowLoadResult {
            request: first_request,
            records: stale_records,
            missing_block_ids: Vec::new(),
        }),
        PayloadWindowApplyDecision::DiscardedStaleGeneration { .. }
    ));

    let final_records = final_request
        .block_ids
        .iter()
        .map(|block_id| BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "final"))
        .collect();
    assert_eq!(
        runtime.apply_payload_window_result(PayloadWindowLoadResult {
            request: final_request,
            records: final_records,
            missing_block_ids: Vec::new(),
        }),
        PayloadWindowApplyDecision::Applied
    );
    assert!(
        !runtime
            .projection_for_window_planned()
            .render_window
            .is_placeholder()
    );

    runtime
        .scroll
        .scroll_to_global_offset(80_000.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let revisited = runtime.projection_for_window_planned();
    assert_eq!(revisited.render_window.block_range, first_range);
    assert!(!revisited.render_window.is_placeholder());
    assert!(revisited.blocks.iter().all(|block| !block.placeholder));
    assert!(runtime.activate_payload_window_if_resident(first_range));
    assert!(
        !runtime
            .projection_for_window_planned()
            .render_window
            .is_placeholder()
    );
}

#[test]
fn incremental_scroll_keeps_resident_blocks_and_only_placeholds_missing_edges() {
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
    let payloads = (1..=80 as BlockId)
        .map(|block_id| {
            BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "resident")
        })
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, payloads, 1, 720.0, 0..80);

    runtime
        .scroll
        .scroll_to_global_offset(1_280.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let projection = runtime.projection_for_window_planned();

    assert!(!projection.render_window.is_placeholder());
    assert!(projection.placeholder_window_height.is_none());
    assert!(projection.blocks.iter().any(|block| !block.placeholder));
    assert!(projection.blocks.iter().any(|block| block.placeholder));
    assert!(
        runtime
            .plan_payload_window_load_if_needed(projection.render_window.block_range.clone())
            .is_some()
    );
}
