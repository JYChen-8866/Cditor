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

    let projection = runtime.projection_for_window();

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

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn payload_window_store_loads_requested_window_from_postgres() {
    let (document_store, payload_store, _layout_store, document, base_block_id) =
        postgres_runtime_fixture(81_001).await;
    let records = sample_index_records(base_block_id, 4);
    let payloads = sample_payloads(base_block_id, 4);
    document_store
        .save_block_index_records(document.id, &records, 1)
        .await
        .unwrap();
    payload_store
        .save_block_payloads(document.id, &payloads)
        .await
        .unwrap();
    let mut runtime = DocumentRuntime::from_index_records_with_window(
        81_001,
        records,
        Vec::new(),
        1,
        720.0,
        0..0,
    );

    let decision = runtime
        .load_payload_window_from_store(&payload_store, 1..3)
        .await
        .unwrap();

    assert_eq!(decision, PayloadWindowApplyDecision::Applied);
    assert_eq!(runtime.payload_window.block_range, 1..3);
    assert_eq!(runtime.payload_window.payloads.len(), 2);
    assert!(
        runtime
            .payload_window
            .payloads
            .contains_key(&(base_block_id + 1))
    );
    assert!(
        runtime
            .payload_window
            .payloads
            .contains_key(&(base_block_id + 2))
    );
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn runtime_from_store_loads_metadata_snapshot_layout_and_initial_payload_window() {
    let (document_store, payload_store, layout_store, document, base_block_id) =
        postgres_runtime_fixture(80_001).await;
    let records = sample_index_records(base_block_id, 4);
    let payloads = sample_payloads(base_block_id, 4);
    document_store
        .save_block_index_records(document.id, &records, 1)
        .await
        .unwrap();
    payload_store
        .save_block_payloads(document.id, &payloads)
        .await
        .unwrap();
    document_store
        .save_document_index_snapshot(document.id, 0, 1, &records)
        .await
        .unwrap();
    let layout_key = runtime_store_layout_key();
    layout_store
        .save_block_layout(
            document.id,
            &cditor_storage::layout_cache::BlockLayoutRow::new(
                base_block_id,
                layout_key,
                HeightEstimate::new(123.0, HeightConfidence::Exact, 0.0),
            ),
        )
        .await
        .unwrap();

    let (runtime, report) = DocumentRuntime::from_store(
        document.id,
        &document_store,
        &payload_store,
        &layout_store,
        DocumentRuntimeFromStoreOptions {
            initial_payload_window_blocks: 2,
            layout_key,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(report.document_title, document.title);
    assert_eq!(report.index_source, DocumentRuntimeIndexSource::Snapshot);
    assert_eq!(report.total_blocks, 4);
    assert_eq!(report.payloads_loaded, 2);
    assert_eq!(report.payloads_missing, 0);
    assert_eq!(report.layout_cache_hits, 1);
    assert_eq!(runtime.index.total_count(), 4);
    assert_eq!(runtime.payload_window.block_range, 0..2);
    assert_eq!(runtime.payload_window.payloads.len(), 2);
    assert_eq!(runtime.index.layout_meta[0].measured_height, Some(123.0));
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn runtime_from_store_rebuilds_from_blocks_when_snapshot_is_stale() {
    let (document_store, payload_store, layout_store, document, base_block_id) =
        postgres_runtime_fixture(80_002).await;
    let stale_records = sample_index_records(base_block_id, 2);
    document_store
        .save_block_index_records(document.id, &stale_records, 1)
        .await
        .unwrap();
    document_store
        .save_document_index_snapshot(document.id, 0, 1, &stale_records)
        .await
        .unwrap();

    let current_records = sample_index_records(base_block_id, 3);
    let current_payloads = sample_payloads(base_block_id, 1);
    document_store
        .save_block_index_records(document.id, &current_records, 2)
        .await
        .unwrap();
    payload_store
        .save_block_payloads(document.id, &current_payloads)
        .await
        .unwrap();

    let (runtime, report) = DocumentRuntime::from_store(
        document.id,
        &document_store,
        &payload_store,
        &layout_store,
        DocumentRuntimeFromStoreOptions {
            initial_payload_window_blocks: 2,
            layout_key: runtime_store_layout_key(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(report.index_source, DocumentRuntimeIndexSource::Blocks);
    assert_eq!(runtime.index.total_count(), 3);
    assert_eq!(runtime.index.structure_version, 2);
    assert_eq!(report.payloads_loaded, 1);
    assert_eq!(report.payloads_missing, 1);
}

async fn postgres_runtime_fixture(
    document_id: u64,
) -> (
    cditor_storage_postgres::PostgresDocumentStore,
    cditor_storage_postgres::PostgresPayloadStore,
    cditor_storage_postgres::PostgresLayoutCacheStore,
    cditor_storage_postgres::DocumentRow,
    BlockId,
) {
    use cditor_storage_postgres::{
        DocumentRow, PostgresDocumentStore, PostgresLayoutCacheStore, PostgresPayloadStore,
        PostgresPoolConfig, create_pg_pool, pg_document_id_from_runtime, run_migrations,
    };
    use sqlx::types::Uuid;

    let database_url = std::env::var("CDITOR_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned());
    let pool = create_pg_pool(&PostgresPoolConfig::for_tests(database_url))
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let document_store = PostgresDocumentStore::new(pool.clone());
    let payload_store = PostgresPayloadStore::new(pool.clone());
    let layout_store = PostgresLayoutCacheStore::new(pool);
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64;
    let runtime_document_id = document_id + suffix;
    let document = DocumentRow {
        id: pg_document_id_from_runtime(runtime_document_id),
        workspace_id: Uuid::from_u128(
            0x9500_0000_0000_0000_0000_0000_0000_0000 | runtime_document_id as u128,
        ),
        title: format!("Runtime Store {runtime_document_id}"),
        structure_version: 1,
        content_version: 1,
        layout_version: 0,
        schema_version: 1,
    };
    document_store
        .save_document_metadata(&document)
        .await
        .unwrap();
    let base_block_id = runtime_document_id * 10;
    (
        document_store,
        payload_store,
        layout_store,
        document,
        base_block_id,
    )
}

fn sample_index_records(base_block_id: BlockId, count: usize) -> Vec<BlockIndexRecord> {
    (0..count)
        .map(|index| {
            BlockIndexRecord::new(
                base_block_id + index as u64,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(BlockLayoutMeta::new(base_block_id + index as u64, 32.0))
        })
        .collect()
}

fn sample_payloads(base_block_id: BlockId, count: usize) -> Vec<BlockPayloadRecord> {
    (0..count)
        .map(|index| {
            BlockPayloadRecord::rich_text(
                base_block_id + index as u64,
                RichBlockKind::Paragraph,
                format!("payload {index}"),
            )
        })
        .collect()
}

fn runtime_store_layout_key() -> LayoutCacheKey {
    LayoutCacheKey {
        width_bucket: 10,
        exact_width_px: 800,
        content_version: 1,
        attrs_version: 0,
        style_version: 0,
        font_version: 0,
        theme_version: 0,
        scale_factor_milli: 1000,
    }
}
