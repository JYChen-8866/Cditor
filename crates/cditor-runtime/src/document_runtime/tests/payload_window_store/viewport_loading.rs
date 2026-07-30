use super::*;

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
        .layout
        .scroll
        .scroll_to_global_offset(160_000.0, cditor_viewport::scroll::ScrollOrigin::UserWheel)
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
    runtime.apply_payload_window_result(prepared_payload_result(request, records, Vec::new()));

    let loaded = runtime.projection_for_window_planned();
    assert!(!loaded.render_window.is_placeholder());
    assert!(loaded.blocks.len() <= 320);
    assert!(runtime.document.payload_window.payloads.len() < 500);
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
        .layout
        .scroll
        .scroll_to_global_offset(80_000.0, cditor_viewport::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let first_projection = runtime.projection_for_window_planned();
    let first_range = first_projection.render_window.block_range.clone();
    let first_request = runtime
        .plan_payload_window_load_if_needed(first_range.clone())
        .unwrap();

    runtime
        .layout
        .scroll
        .scroll_to_global_offset(240_000.0, cditor_viewport::scroll::ScrollOrigin::UserWheel)
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
        runtime.apply_payload_window_result(prepared_payload_result(
            first_request,
            stale_records,
            Vec::new(),
        )),
        PayloadWindowApplyDecision::DiscardedStaleGeneration { .. }
    ));

    let final_records = final_request
        .block_ids
        .iter()
        .map(|block_id| BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "final"))
        .collect();
    assert_eq!(
        runtime.apply_payload_window_result(prepared_payload_result(
            final_request,
            final_records,
            Vec::new(),
        )),
        PayloadWindowApplyDecision::Applied
    );
    assert!(
        !runtime
            .projection_for_window_planned()
            .render_window
            .is_placeholder()
    );

    runtime
        .layout
        .scroll
        .scroll_to_global_offset(80_000.0, cditor_viewport::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let revisited = runtime.projection_for_window_planned();
    assert_eq!(revisited.render_window.block_range, first_range);
    assert!(!revisited.render_window.is_placeholder());
    assert!(revisited.blocks.iter().all(|block| !block.placeholder));
    assert!(
        !runtime.activate_payload_window_if_resident(first_range),
        "projection commit already activated the resident window"
    );
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
    let stable = runtime.projection_for_window_planned();
    assert!(stable.blocks.iter().all(|block| !block.placeholder));

    runtime
        .layout
        .scroll
        .scroll_to_global_offset(1_280.0, cditor_viewport::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let preparing = runtime.projection_for_window_planned();

    assert!(!preparing.render_window.is_placeholder());
    assert!(preparing.placeholder_window_height.is_none());
    assert!(preparing.blocks.iter().all(|block| !block.placeholder));
    assert_eq!(
        preparing.render_window.block_range, stable.render_window.block_range,
        "an incomplete adjacent target must retain the complete stable frame"
    );
    let desired_range = runtime.current_foreground_payload_range();
    assert!(
        runtime
            .plan_payload_window_load_if_needed(desired_range)
            .is_some()
    );
}

#[test]
fn visible_request_takes_ownership_from_overlapping_prefetch() {
    let records = (1..=32)
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
    let prefetch = runtime
        .plan_payload_prefetch_load_if_needed(8..16)
        .expect("prefetch owns the cold range first");
    let visible = runtime
        .plan_payload_window_load_if_needed(8..16)
        .expect("visible work must take over a prefetch-owned range");
    assert!(visible.generation > prefetch.generation);
    assert!(visible.block_ids.iter().all(|block_id| {
        runtime.document.payload_window.loading_priority(*block_id)
            == Some(PayloadLoadPriority::Visible)
    }));

    let stale_records = prefetch
        .block_ids
        .iter()
        .map(|block_id| {
            BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "stale prefetch")
        })
        .collect();
    assert!(matches!(
        runtime.apply_payload_window_result(prepared_payload_result(
            prefetch,
            stale_records,
            Vec::new(),
        )),
        PayloadWindowApplyDecision::DiscardedStaleGeneration { .. }
    ));
    assert!(
        visible.block_ids.iter().all(|block_id| runtime
            .document
            .payload_window
            .get(*block_id)
            .is_none())
    );

    let visible_records = visible
        .block_ids
        .iter()
        .map(|block_id| {
            BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "visible")
        })
        .collect();
    assert_eq!(
        runtime.apply_payload_window_result(prepared_payload_result(
            visible,
            visible_records,
            Vec::new(),
        )),
        PayloadWindowApplyDecision::Applied
    );
}

#[test]
fn transient_prefetch_error_does_not_publish_a_visible_failure() {
    let records = (1..=16)
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
        .plan_payload_prefetch_load_if_needed(4..8)
        .expect("prefetch range is cold");

    assert_eq!(
        runtime.apply_payload_prefetch_load_error(request),
        PayloadWindowApplyDecision::Applied
    );
    assert!(runtime.document.payload_window.loading.is_empty());
    assert!(runtime.document.payload_window.failed.is_empty());
}
