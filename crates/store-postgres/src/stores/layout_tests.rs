use sqlx::types::Uuid;

use super::*;
use crate::{
    DocumentRow, PostgresDocumentStore, PostgresPayloadStore, PostgresPoolConfig, create_pg_pool,
    pg_document_id_from_runtime, run_migrations,
};
use cditor_core::document::BlockIndexRecord;
use cditor_core::layout::HeightEstimate;
use cditor_core::rich_text::{RichBlockKind, kind_tag_for_rich_block_kind};

fn test_database_url() -> String {
    std::env::var("CDITOR_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
}

async fn test_stores() -> (
    PostgresDocumentStore,
    PostgresPayloadStore,
    PostgresLayoutCacheStore,
) {
    let config = PostgresPoolConfig::for_tests(test_database_url());
    let pool = create_pg_pool(&config).await.unwrap();
    run_migrations(&pool).await.unwrap();
    (
        PostgresDocumentStore::new(pool.clone()),
        PostgresPayloadStore::new(pool.clone()),
        PostgresLayoutCacheStore::new(pool),
    )
}

fn document_row(document_id: u64) -> DocumentRow {
    DocumentRow {
        id: pg_document_id_from_runtime(document_id),
        workspace_id: Uuid::from_u128(
            0x9200_0000_0000_0000_0000_0000_0000_0000 | document_id as u128,
        ),
        title: format!("Layout Store {document_id}"),
        structure_version: 1,
        content_version: 1,
        layout_version: 0,
        schema_version: 1,
    }
}

async fn seed_document_with_block(
    document_id: u64,
    block_id: u64,
    content_version: u64,
) -> (
    PostgresDocumentStore,
    PostgresPayloadStore,
    PostgresLayoutCacheStore,
    DocumentRow,
) {
    let (document_store, payload_store, layout_store) = test_stores().await;
    let document = document_row(document_id);
    document_store
        .save_document_metadata(&document)
        .await
        .unwrap();
    document_store
        .save_block_index_records(
            document.id,
            &[BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )],
            2,
        )
        .await
        .unwrap();
    if content_version > 1 {
        sqlx::query("UPDATE blocks SET content_version = $2 WHERE id = $1")
            .bind(pg_block_id_from_runtime(block_id))
            .bind(i64::try_from(content_version).unwrap())
            .execute(layout_store.pool())
            .await
            .unwrap();
    }
    (document_store, payload_store, layout_store, document)
}

fn key(content_version: u64, width_bucket: u16, font_version: u64) -> LayoutCacheKey {
    LayoutCacheKey {
        width_bucket,
        exact_width_px: u32::from(width_bucket) * 80,
        content_version,
        attrs_version: 0,
        style_version: 0,
        font_version,
        theme_version: 1,
        scale_factor_milli: 1000,
    }
}

fn block_layout_row(block_id: BlockId, key: LayoutCacheKey, height: f64) -> BlockLayoutRow {
    let mut row = BlockLayoutRow::new(
        block_id,
        key,
        HeightEstimate::new(height, HeightConfidence::Exact, 0.0),
    );
    row.measured_at = Some(1);
    row.line_count = Some(3);
    row.layout_cost = 5;
    row
}

#[test]
fn conversion_helpers_reject_negative_values() {
    assert!(u64_from_i64(-1, "version").is_err());
    assert!(u16_from_i32(-1, "width_bucket").is_err());
    assert!(u8_from_i32(300, "confidence").is_err());
}

#[test]
fn batch_block_layout_query_prioritizes_exact_hash_in_one_round_trip() {
    let sql = batch_block_layout_select_sql();

    assert!(sql.contains("DISTINCT ON (block_id)"));
    assert!(sql.contains("block_id = ANY($1)"));
    assert!(sql.contains("(layout_key_hash = $2) DESC"));
    assert!(sql.contains("measured_at DESC NULLS LAST"));
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_layout_store_saves_and_loads_exact_block_height() {
    let block_id = 700_000;
    let (_document_store, _payload_store, layout_store, document) =
        seed_document_with_block(50_001, block_id, 2).await;
    let key = key(2, 10, 1);
    let row = block_layout_row(block_id, key, 128.0);

    layout_store
        .save_block_layout(document.id, &row)
        .await
        .unwrap();

    let cached = layout_store.load_block_height(block_id, key).await.unwrap();
    assert_eq!(cached.height, 128.0);
    assert_eq!(cached.confidence, HeightConfidence::Exact);
    assert_eq!(cached.source, CacheSource::ExactMatch);

    let batch = layout_store
        .load_block_heights(&[block_id, block_id + 1], key)
        .await
        .unwrap();
    assert_eq!(batch.get(&block_id), Some(&cached));
    assert!(!batch.contains_key(&(block_id + 1)));
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_layout_store_falls_back_to_historical_block_height() {
    let block_id = 700_010;
    let (_document_store, _payload_store, layout_store, document) =
        seed_document_with_block(50_002, block_id, 2).await;
    let old_key = key(2, 10, 1);
    let requested_key = key(3, 10, 1);
    let row = block_layout_row(block_id, old_key, 144.0);

    layout_store
        .save_block_layout(document.id, &row)
        .await
        .unwrap();

    let cached = layout_store
        .load_block_height(block_id, requested_key)
        .await
        .unwrap();
    assert_eq!(cached.height, 144.0);
    assert_eq!(cached.confidence, HeightConfidence::Historical);
    assert_eq!(cached.source, CacheSource::VersionMismatch);
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_layout_store_rejects_stale_block_layout_content_version() {
    let block_id = 700_020;
    let (_document_store, _payload_store, layout_store, document) =
        seed_document_with_block(50_003, block_id, 5).await;
    let stale_row = block_layout_row(block_id, key(4, 10, 1), 100.0);

    let error = layout_store
        .save_block_layout(document.id, &stale_row)
        .await
        .unwrap_err();

    assert!(matches!(error, PostgresStorageError::Conflict { .. }));
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_layout_store_saves_and_loads_page_layout_with_historical_fallback() {
    let block_id = 700_030;
    let (_document_store, _payload_store, layout_store, _document) =
        seed_document_with_block(50_004, block_id, 1).await;
    let page = PageLayoutRow {
        document_id: 50_004,
        visible_index_version: 1,
        structure_version: 2,
        layout_key_hash: "layout-v1".to_owned(),
        page_policy_version: 1,
        page_index: 0,
        block_start_index: 0,
        block_count: 1,
        first_block_id: Some(block_id),
        last_block_id: Some(block_id),
        height: 256.0,
        measured_ratio: 1.0,
        confidence: HeightConfidence::Exact,
        max_error_hint: 0.0,
        dirty: false,
        updated_at: 1,
    };

    layout_store.save_page_layout(&page).await.unwrap();

    let exact = layout_store
        .load_page_height(50_004, 1, 2, "layout-v1", 1, 0)
        .await
        .unwrap();
    assert_eq!(exact.height, 256.0);
    assert_eq!(exact.source, CacheSource::ExactMatch);

    let historical = layout_store
        .load_page_height(50_004, 1, 3, "layout-v1", 1, 0)
        .await
        .unwrap();
    assert_eq!(historical.height, 256.0);
    assert_eq!(historical.confidence, HeightConfidence::Historical);
    assert_eq!(historical.source, CacheSource::VersionMismatch);
    assert!(historical.dirty);
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_layout_store_flushes_height_write_batch_from_debouncer() {
    let block_id = 700_040;
    let (_document_store, _payload_store, layout_store, document) =
        seed_document_with_block(50_005, block_id, 2).await;
    let key = key(2, 12, 1);
    let block_row = block_layout_row(block_id, key, 188.0);
    let page_row = PageLayoutRow {
        document_id: 50_005,
        visible_index_version: 1,
        structure_version: 2,
        layout_key_hash: "debounced-layout".to_owned(),
        page_policy_version: 1,
        page_index: 0,
        block_start_index: 0,
        block_count: 1,
        first_block_id: Some(block_id),
        last_block_id: Some(block_id),
        height: 188.0,
        measured_ratio: 1.0,
        confidence: HeightConfidence::Exact,
        max_error_hint: 0.0,
        dirty: false,
        updated_at: 1,
    };

    layout_store
        .flush_height_writes(
            document.id,
            &[
                HeightWrite::Block(block_row.clone()),
                HeightWrite::Page(page_row.clone()),
            ],
        )
        .await
        .unwrap();

    let block = layout_store.load_block_height(block_id, key).await.unwrap();
    let page = layout_store
        .load_page_height(50_005, 1, 2, "debounced-layout", 1, 0)
        .await
        .unwrap();

    assert_eq!(block.height, 188.0);
    assert_eq!(block.source, CacheSource::ExactMatch);
    assert_eq!(page.height, 188.0);
    assert_eq!(page.source, CacheSource::ExactMatch);
}
