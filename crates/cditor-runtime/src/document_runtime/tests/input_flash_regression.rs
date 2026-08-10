//! Regressions for the input-flash / skeleton investigation
//! (`doc/diagnostics/editor-input-flash-skeleton-root-cause.md`).
//!
//! Covers: Enter-split measured-height retention, measured-height tolerance
//! absorbing platform re-measure jitter, and window-planning hysteresis
//! keeping the projection identity stable under height-only drift.

use super::*;

fn soft_wrapped_paragraph_runtime(text: &str) -> DocumentRuntime {
    let records = vec![
        BlockIndexRecord::new(
            1,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            0,
        )
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0)),
        BlockIndexRecord::new(
            2,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            0,
        )
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(2, 32.0)),
    ];
    let payloads = vec![
        BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, text),
        BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "下一段"),
    ];
    DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0)
}

fn apply_measured_height(runtime: &mut DocumentRuntime, block_id: BlockId, height: f64) {
    let version = runtime
        .document
        .payload_window
        .get(block_id)
        .expect("payload resident")
        .content_version;
    assert!(
        runtime
            .queue_measured_height(block_id, version, height)
            .unwrap(),
        "measurement should be accepted as a change"
    );
    assert!(runtime.flush_pending_height_corrections().unwrap());
}

#[test]
fn enter_split_of_a_soft_wrapped_paragraph_preserves_measured_height() {
    let mut runtime = soft_wrapped_paragraph_runtime(&"word ".repeat(60));
    let line_height = cditor_core::layout::text_line_height_for_kind(&RichBlockKind::Paragraph);
    // A soft-wrapped block measured at the real render width: five lines plus
    // block chrome, deliberately different from the synthetic-width estimate.
    let measured = 5.0 * line_height + 8.0;
    apply_measured_height(&mut runtime, 1, measured);

    runtime.focus_block(1);
    let text_len = runtime.focused_text().unwrap().len();
    runtime.focus_block_at_offset(1, text_len / 2).unwrap();
    runtime.handle_enter().unwrap();
    let new_block_id = runtime
        .focused_block_id()
        .expect("enter focuses the new block");
    assert_ne!(new_block_id, 1);

    let height_of = |runtime: &DocumentRuntime, block_id: BlockId| {
        let index = runtime.document.index.index_of(block_id).unwrap();
        runtime.document.index.layout_meta[index].effective_height()
    };
    let prefix = height_of(&runtime, 1);
    let suffix = height_of(&runtime, new_block_id);
    let sum = prefix + suffix;
    // The split duplicates chrome and can add one wrap boundary; anything
    // beyond that means the measurement was thrown away and re-estimated at
    // the synthetic layout width.
    assert!(
        sum >= measured - line_height && sum <= measured + 2.0 * line_height + 8.0,
        "split heights should stay near the measurement: \
         prefix={prefix:.1} suffix={suffix:.1} sum={sum:.1} measured={measured:.1}"
    );
    assert!(
        prefix < measured && suffix < measured,
        "each half must be smaller than the whole: prefix={prefix:.1} suffix={suffix:.1}"
    );
}

#[test]
fn sub_tolerance_measurement_jitter_is_absorbed_without_queueing() {
    let mut runtime = soft_wrapped_paragraph_runtime(&"word ".repeat(60));
    let line_height = cditor_core::layout::text_line_height_for_kind(&RichBlockKind::Paragraph);
    let measured = 3.0 * line_height;
    apply_measured_height(&mut runtime, 1, measured);

    let tolerance = cditor_core::layout::measured_height_tolerance_px(&RichBlockKind::Paragraph);
    assert!(tolerance >= 0.5);
    let version = runtime
        .document
        .payload_window
        .get(1)
        .unwrap()
        .content_version;
    // Windows-style per-keystroke jitter: the platform re-measures the same
    // logical layout with alternating sub-tolerance deltas.
    for step in 0..6 {
        let sign = if step % 2 == 0 { 1.0 } else { -1.0 };
        let jittered = measured + sign * tolerance * 0.5;
        assert!(
            !runtime.queue_measured_height(1, version, jittered).unwrap(),
            "jitter of {:.2}px must be absorbed (tolerance {tolerance:.2}px)",
            tolerance * 0.5
        );
    }
    assert!(runtime.layout.pending_measured_heights.is_empty());
}

#[test]
fn height_only_drift_keeps_the_planned_window_identity() {
    let mut runtime = runtime_with_paragraph_blocks(3_000);
    let first = runtime.projection_for_window_planned();
    let planned_range = first.render_window.block_range.clone();
    let generation = runtime.layout.projection.generation();
    assert!(!first.render_window.is_placeholder());

    // Grow a block inside the viewport by several rows: enough to move the
    // window's overscan edge by a couple of blocks, well within hysteresis.
    let grown_block: BlockId = 10;
    apply_measured_height(&mut runtime, grown_block, 112.0);

    let second = runtime.projection_for_window_planned();
    assert_eq!(
        second.render_window.block_range, planned_range,
        "height-only drift must not re-plan the window"
    );
    assert_eq!(
        runtime.layout.projection.generation(),
        generation,
        "height-only drift must not bump the projection generation"
    );
    assert!(!second.render_window.is_placeholder());

    // A real scroll bypasses hysteresis and re-plans.
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(20_000.0, ScrollOrigin::ProgrammaticVirtualScroll)
        .unwrap();
    let third = runtime.projection_for_window_planned();
    assert_ne!(
        third.render_window.block_range, planned_range,
        "scrolling must re-plan the window"
    );
}

#[test]
fn structure_edit_still_replans_the_window() {
    let mut runtime = runtime_with_paragraph_blocks(64);
    let first = runtime.projection_for_window_planned();
    assert!(!first.render_window.is_placeholder());
    let generation = runtime.layout.projection.generation();

    runtime.focus_block(1);
    runtime.handle_enter().unwrap();
    let new_block_id = runtime
        .focused_block_id()
        .expect("enter focuses the new block");

    let second = runtime.projection_for_window_planned();
    assert!(
        runtime.layout.projection.generation() > generation,
        "a structure edit must bypass hysteresis and re-plan the window"
    );
    assert!(
        !second.render_window.is_placeholder(),
        "a local enter split with resident payloads must not fall back to a skeleton window"
    );
    assert!(
        second
            .blocks
            .iter()
            .any(|block| block.block_id == new_block_id && !block.placeholder),
        "the new block must be projected with a loaded payload"
    );
}

#[test]
fn structure_edit_with_missing_payloads_replays_the_previous_frame() {
    let mut runtime = runtime_with_paragraph_blocks(200);
    let first = runtime.projection_for_window_planned();
    assert!(!first.render_window.is_placeholder());
    assert!(!first.blocks.is_empty());

    // Cache pressure evicted a mid-viewport payload before the edit, so the
    // post-edit desired window cannot be resident this frame.
    let victim = runtime.document.visible_index.visible_block_ids[5];
    runtime.document.payload_window.payloads.remove(&victim);

    runtime.focus_block(1);
    runtime.handle_enter().unwrap();

    let second = runtime.projection_for_window_planned();
    assert!(
        !second.render_window.is_placeholder(),
        "the invalidated stable frame must replay instead of a skeleton window"
    );
    assert!(second.placeholder_window_height.is_none());
    assert!(
        !second.blocks.is_empty(),
        "the replayed frame keeps real content on screen"
    );

    // Once the missing payload returns, the next frame publishes fresh.
    runtime
        .document
        .payload_window
        .insert_loaded(BlockPayloadRecord::rich_text(
            victim,
            RichBlockKind::Paragraph,
            "reloaded",
        ));
    let third = runtime.projection_for_window_planned();
    assert!(!third.render_window.is_placeholder());
    assert!(
        third
            .blocks
            .iter()
            .any(|block| block.block_id == victim && !block.placeholder)
    );
    assert!(
        runtime
            .layout
            .projection
            .publication
            .stale_fallback
            .is_none(),
        "a successful publication drops the retained fallback frame"
    );
}

#[test]
fn overscan_gap_does_not_block_the_visible_core_commit() {
    // 1000 blocks, only the first 80 payloads resident.
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

    // Scroll so the viewport core stays resident but the trailing overscan
    // reaches past the resident range.
    runtime
        .layout
        .scroll
        .scroll_to_global_offset(1_280.0, ScrollOrigin::UserWheel)
        .unwrap();
    let projection = runtime.projection_for_window_planned();

    assert!(!projection.render_window.is_placeholder());
    assert!(
        projection.render_window.block_range.end > 80,
        "the render window must reach past the resident payloads for this test"
    );
    assert!(
        projection
            .blocks
            .iter()
            .filter(|block| projection
                .payload_visible_block_range
                .contains(&block.visible_index))
            .all(|block| !block.placeholder),
        "the visible core commits fully loaded"
    );
    assert!(
        projection.blocks.iter().any(|block| block.placeholder),
        "overscan rows beyond the resident range reserve geometry as placeholders"
    );
}

#[test]
fn composition_preview_is_projected_for_code_and_mermaid_blocks() {
    for kind in [
        RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        RichBlockKind::Mermaid,
    ] {
        let record = BlockIndexRecord::new(1, None, 0, kind_tag_for_rich_block_kind(&kind), 0)
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0));
        let payload = BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: kind.clone(),
            payload: BlockPayload::Code {
                language: match &kind {
                    RichBlockKind::Code { language } => language.clone(),
                    _ => Some("mermaid".to_owned()),
                },
                text: "fn main".to_owned(),
            },
        };
        let mut runtime =
            DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0);
        runtime.focus_block_at_offset(1, 7).unwrap();
        let expected = runtime.input_session_identity().unwrap();

        runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected,
                input: RealtimeInput::UpdateComposition {
                    range: 7..7,
                    text: "nihao",
                    selected_range: Some(5..5),
                },
            })
            .unwrap();

        let projection = runtime.projection_for_window_planned();
        let block = projection
            .blocks
            .iter()
            .find(|block| block.block_id == 1)
            .expect("composition block is projected");
        let BlockPayloadView::Loaded(record) = &block.payload else {
            panic!("{kind:?}: composition block payload must be loaded");
        };
        let BlockPayload::Code { text, .. } = &record.payload else {
            panic!("{kind:?}: payload must stay a code payload");
        };
        assert_eq!(
            text, "fn mainnihao",
            "{kind:?}: the projected payload must contain the composition preview"
        );
        assert_eq!(
            block.marked_range,
            Some(7..12),
            "{kind:?}: the projected block must carry the marked range"
        );
    }
}
