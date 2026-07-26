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
        .layout
        .scroll
        .scroll_to_global_offset(
            400.0 * 32.0,
            cditor_viewport::scroll::ScrollOrigin::UserWheel,
        )
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
    assert_eq!(
        request
            .block_ids
            .iter()
            .copied()
            .collect::<HashSet<_>>()
            .len(),
        request.block_ids.len(),
        "interaction pins must not duplicate ids from the visible range"
    );
}

#[test]
fn visible_payload_apply_defers_large_text_model_hydration_until_focus() {
    let records = vec![BlockIndexRecord::new(
        1,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::Code {
            language: Some("rust".to_owned()),
        }),
        0,
    )];
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);
    let request = runtime
        .plan_payload_window_load_if_needed(0..1)
        .expect("cold code payload needs loading");
    let source = "fn main() {}\n".repeat(128 * 1024);

    assert_eq!(
        runtime.apply_payload_window_result(prepared_payload_result(
            request,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                payload: BlockPayload::Code {
                    language: Some("rust".to_owned()),
                    text: source,
                },
            }],
            Vec::new(),
        )),
        PayloadWindowApplyDecision::Applied
    );
    assert!(!runtime.document.text_models.contains_key(&1));

    let projection = runtime.projection_for_window_planned();
    assert!(matches!(
        projection.blocks[0].payload,
        BlockPayloadView::Loaded(_)
    ));
    assert!(!runtime.document.text_models.contains_key(&1));

    runtime.focus_block_at_offset(1, 0).unwrap();
    assert!(runtime.document.text_models.contains_key(&1));
}

#[test]
fn first_document_selection_on_cold_resident_payload_hydrates_and_focuses_text() {
    let records = vec![BlockIndexRecord::new(
        1,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
        0,
    )];
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);
    let request = runtime
        .plan_payload_window_load_if_needed(0..1)
        .expect("cold paragraph payload needs loading");

    assert_eq!(
        runtime.apply_payload_window_result(prepared_payload_result(
            request,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "before target after",
            )],
            Vec::new(),
        )),
        PayloadWindowApplyDecision::Applied
    );
    assert!(runtime.block_payload_record(1).is_some());
    assert!(!runtime.document.text_models.contains_key(&1));
    assert_eq!(runtime.focused_block_id(), None);

    let position = TextPosition {
        block_id: 1,
        offset: 9,
        affinity: TextAffinity::Upstream,
    };
    let outcome = runtime
        .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
            cditor_editor_protocol::command::EditorCommand::SetDocumentSelection {
                selection: DocumentSelection::caret(position),
            },
            cditor_editor_protocol::command::CommandSource::Toolbar,
        ))
        .expect("the first body click must activate a cold resident text payload");

    assert!(outcome.selection_changed);
    assert!(runtime.document.text_models.contains_key(&1));
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 1 })
    );
    assert_eq!(runtime.caret_position_for_block(1), Some(position));
}

#[test]
fn cross_block_selection_hydrates_both_cold_resident_text_endpoints() {
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
    let request = runtime
        .plan_payload_window_load_if_needed(0..2)
        .expect("both cold endpoint payloads need loading");

    assert_eq!(
        runtime.apply_payload_window_result(prepared_payload_result(
            request,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "anchor text"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "focus text"),
            ],
            Vec::new(),
        )),
        PayloadWindowApplyDecision::Applied
    );
    assert!(
        runtime
            .projection_for_window_planned()
            .blocks
            .iter()
            .all(|block| matches!(block.payload, BlockPayloadView::Loaded(_)))
    );
    assert!(!runtime.document.text_models.contains_key(&1));
    assert!(!runtime.document.text_models.contains_key(&2));

    let selection = DocumentSelection {
        anchor: TextPosition {
            block_id: 1,
            offset: 3,
            affinity: TextAffinity::Downstream,
        },
        focus: TextPosition {
            block_id: 2,
            offset: 5,
            affinity: TextAffinity::Upstream,
        },
    };
    let outcome = runtime
        .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
            cditor_editor_protocol::command::EditorCommand::SetDocumentSelection { selection },
            cditor_editor_protocol::command::CommandSource::Toolbar,
        ))
        .expect("cross-block selection must activate both cold resident endpoints");

    assert!(outcome.selection_changed);
    assert!(runtime.document.text_models.contains_key(&1));
    assert!(runtime.document.text_models.contains_key(&2));
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 2 })
    );
    assert_eq!(runtime.document_selection_snapshot(), Some(selection));
    assert_eq!(runtime.caret_position_for_block(2), Some(selection.focus));
}

#[test]
fn loaded_table_projects_before_its_editing_runtime_is_hydrated() {
    let records = vec![BlockIndexRecord::new(
        1,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::Table),
        0,
    )];
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);
    let request = runtime
        .plan_payload_window_load_if_needed(0..1)
        .expect("cold table payload needs loading");

    assert_eq!(
        runtime.apply_payload_window_result(prepared_payload_result(
            request,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Table,
                payload: default_table_payload("cell".to_owned()),
            }],
            Vec::new(),
        )),
        PayloadWindowApplyDecision::Applied
    );
    assert!(!runtime.document.table_runtimes.contains_key(&1));

    let projection = runtime.projection_for_window_planned();
    assert!(projection.blocks[0].table_view.is_some());
    assert!(!runtime.document.table_runtimes.contains_key(&1));

    runtime.focus_table_cell(1, 0, 0).unwrap();
    assert!(runtime.document.table_runtimes.contains_key(&1));
}

#[test]
fn payload_window_store_discards_stale_generation_result() {
    let mut runtime = runtime_with_paragraph_blocks(4);
    let stale = runtime.plan_payload_window_load(0..2);
    let current = runtime.plan_payload_window_load(2..4);
    assert_eq!(current.generation, 2);

    let decision =
        runtime.apply_payload_window_result(prepared_payload_result(stale, Vec::new(), Vec::new()));

    assert_eq!(
        decision,
        PayloadWindowApplyDecision::DiscardedStaleGeneration {
            expected: 2,
            actual: 1,
        }
    );
    assert_eq!(
        runtime.document.payload_window.block_range,
        0..4,
        "stale and current I/O plans must not replace the committed window"
    );
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

    let decision = runtime.apply_payload_window_result(prepared_payload_result(
        stale,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "one"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "two"),
        ],
        Vec::new(),
    ));

    assert_eq!(
        decision,
        PayloadWindowApplyDecision::DiscardedStaleGeneration {
            expected: current.generation,
            actual: current.generation - 1,
        }
    );
    assert_eq!(
        runtime.document.payload_window.get(1).unwrap().plain_text(),
        "one"
    );
    assert_eq!(
        runtime.document.payload_window.get(2).unwrap().plain_text(),
        "two"
    );
    assert!(!runtime.document.payload_window.loading.contains(&1));
    assert!(!runtime.document.payload_window.loading.contains(&2));
    assert!(runtime.document.payload_window.loading.contains(&5));
    assert!(runtime.document.payload_window.loading.contains(&6));
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

    runtime.apply_payload_window_result(prepared_payload_result(
        stale,
        vec![BlockPayloadRecord::rich_text(
            2,
            RichBlockKind::Paragraph,
            "stale",
        )],
        Vec::new(),
    ));

    assert!(runtime.document.payload_window.loading.contains(&2));
    assert!(runtime.document.payload_window.get(2).is_none());

    runtime.apply_payload_window_result(prepared_payload_result(
        current,
        vec![BlockPayloadRecord::rich_text(
            2,
            RichBlockKind::Paragraph,
            "current",
        )],
        Vec::new(),
    ));
    assert_eq!(
        runtime.document.payload_window.get(2).unwrap().plain_text(),
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

    let loaded_records = request
        .block_ids
        .iter()
        .map(|block_id| {
            BlockPayloadRecord::rich_text(*block_id, RichBlockKind::Paragraph, "loaded")
        })
        .collect();
    runtime.apply_payload_window_result(prepared_payload_result(
        request,
        loaded_records,
        Vec::new(),
    ));
    assert!(runtime.document.payload_window.loading.is_empty());
    assert!(runtime.plan_payload_window_load_if_needed(1..3).is_none());
    assert_eq!(runtime.document.payload_window.block_range, 0..0);
    let committed = runtime.projection_for_window_planned();
    assert!(!committed.render_window.is_placeholder());
    assert_eq!(
        runtime.document.payload_window.block_range,
        committed.render_window.block_range
    );
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
    assert_eq!(runtime.document.payload_window.block_range, 0..4);
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
    assert!(runtime.document.payload_window.loading.contains(&1));
    assert!(runtime.document.payload_window.loading.contains(&2));

    let decision = runtime.apply_payload_window_result(prepared_payload_result(
        request,
        Vec::new(),
        vec![1, 2],
    ));

    assert_eq!(decision, PayloadWindowApplyDecision::Applied);
    assert!(runtime.document.payload_window.loading.is_empty());
    assert!(runtime.document.payload_window.failed.contains_key(&1));
    assert!(runtime.document.payload_window.failed.contains_key(&2));
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
    assert_eq!(
        runtime.document.payload_window.failure_attempts.get(&1),
        Some(&3)
    );
    assert_eq!(
        runtime
            .document
            .payload_window
            .failed
            .get(&1)
            .map(String::as_str),
        Some("attempt 3")
    );

    let failed_projection = runtime.projection_for_window();
    let failure = failed_projection
        .placeholder_window_failure
        .expect("terminal failure is projected");
    assert_eq!(failure.message, "attempt 3");
    assert_eq!(failure.attempts, 3);
    assert_eq!(failure.max_attempts, 3);
    assert!(!failure.automatic_retry_pending);

    assert_eq!(runtime.retry_failed_payload_window(0..2), 2);
    assert!(runtime.document.payload_window.failed.is_empty());
    assert!(runtime.document.payload_window.failure_attempts.is_empty());
    assert!(runtime.plan_payload_window_load_if_needed(0..2).is_some());
}

#[path = "payload_window_store/viewport_loading.rs"]
mod viewport_loading;
