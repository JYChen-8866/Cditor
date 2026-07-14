use super::*;
use crate::{
    DocumentRow, PostgresPoolConfig, create_pg_pool, pg_document_id_from_runtime, run_migrations,
};
use cditor_core::rich_text::RichBlockKind;
use cditor_core::rich_text::kind_tag_for_rich_block_kind;
use sqlx::types::Uuid;

fn test_database_url() -> String {
    std::env::var("CDITOR_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
}

async fn test_store() -> PostgresDocumentStore {
    let config = PostgresPoolConfig::for_tests(test_database_url());
    let pool = create_pg_pool(&config).await.unwrap();
    run_migrations(&pool).await.unwrap();
    PostgresDocumentStore::new(pool)
}

fn document_row(document_id: DocumentId, title: &str) -> DocumentRow {
    DocumentRow {
        id: pg_document_id_from_runtime(document_id),
        workspace_id: Uuid::from_u128(
            0x9000_0000_0000_0000_0000_0000_0000_0000 | document_id as u128,
        ),
        title: title.to_owned(),
        structure_version: 1,
        content_version: 1,
        layout_version: 0,
        schema_version: 1,
    }
}

fn sample_records(base_block_id: u64) -> Vec<BlockIndexRecord> {
    vec![
        BlockIndexRecord::new(
            base_block_id,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Heading { level: 1 }),
            0,
        ),
        BlockIndexRecord::new(
            base_block_id + 1,
            Some(base_block_id),
            1,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            1,
        ),
        BlockIndexRecord::new(
            base_block_id + 2,
            Some(base_block_id),
            1,
            kind_tag_for_rich_block_kind(&RichBlockKind::Todo { checked: false }),
            0,
        ),
    ]
}

#[test]
fn sort_keys_preserve_lexicographic_order() {
    assert!(sort_key_for_index(9) < sort_key_for_index(10));
    assert!(sort_key_for_index(99) < sort_key_for_index(100));
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_document_store_saves_and_loads_document_metadata() {
    let store = test_store().await;
    let document = document_row(30_001, "Postgres document metadata");

    store.save_document_metadata(&document).await.unwrap();

    let loaded = store.load_document_metadata(document.id).await.unwrap();
    assert_eq!(loaded, document);
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_document_store_saves_and_loads_block_structure_index() {
    let store = test_store().await;
    let document = document_row(30_002, "Postgres block index");
    let records = sample_records(300_020);

    store.save_document_metadata(&document).await.unwrap();
    store
        .save_block_index_records(document.id, &records, 2)
        .await
        .unwrap();

    let loaded = store.load_block_index_records(document.id).await.unwrap();
    assert_eq!(loaded.len(), records.len());
    assert_eq!(loaded[0].id, records[0].id);
    assert_eq!(loaded[1].parent_id, Some(300_020));
    assert_eq!(loaded[1].flags, 1);
    assert_eq!(loaded[2].kind_tag, records[2].kind_tag);

    let metadata = store.load_document_metadata(document.id).await.unwrap();
    assert_eq!(metadata.structure_version, 2);
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_document_store_soft_delete_excludes_block_from_index() {
    let store = test_store().await;
    let document = document_row(30_003, "Postgres soft delete");
    let records = sample_records(300_030);

    store.save_document_metadata(&document).await.unwrap();
    store
        .save_block_index_records(document.id, &records, 2)
        .await
        .unwrap();
    store
        .soft_delete_block(pg_block_id_from_runtime(300_031))
        .await
        .unwrap();

    let loaded = store.load_block_index_records(document.id).await.unwrap();
    assert_eq!(
        loaded.iter().map(|record| record.id).collect::<Vec<_>>(),
        vec![300_030, 300_032]
    );
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_document_store_detects_structure_version_conflicts() {
    let store = test_store().await;
    let document = document_row(30_004, "Postgres structure version");

    store.save_document_metadata(&document).await.unwrap();
    store
        .update_document_structure_version(document.id, 1, 2)
        .await
        .unwrap();

    let conflict = store
        .update_document_structure_version(document.id, 1, 3)
        .await
        .unwrap_err();
    assert!(matches!(conflict, PostgresStorageError::Conflict { .. }));
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL; inserts and loads 100k blocks"]
async fn postgres_document_store_loads_100k_block_index_without_payloads() {
    let store = test_store().await;
    let document = document_row(30_005, "Postgres 100k block index");
    let paragraph = kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph);
    let first_block_id = 500_000;
    let records = (first_block_id..first_block_id + 100_000)
        .map(|id| BlockIndexRecord::new(id, None, 0, paragraph, 0))
        .collect::<Vec<_>>();

    store.save_document_metadata(&document).await.unwrap();
    store
        .save_block_index_records(document.id, &records, 2)
        .await
        .unwrap();

    let loaded = store.load_block_index_records(document.id).await.unwrap();
    assert_eq!(loaded.len(), 100_000);
    assert_eq!(loaded.first().map(|record| record.id), Some(first_block_id));
    assert_eq!(
        loaded.last().map(|record| record.id),
        Some(first_block_id + 99_999)
    );
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn postgres_document_index_snapshot_implements_sync_document_index_store() {
    let store = test_store().await;
    let document = document_row(30_006, "Postgres document index snapshot");
    let records = sample_records(300_060);

    store.save_document_metadata(&document).await.unwrap();
    store
        .save_block_index_records(document.id, &records, 7)
        .await
        .unwrap();

    let snapshot = PostgresDocumentIndexSnapshot::load(&store, document.id)
        .await
        .unwrap();
    let index = DocumentIndex::from_store(30_006, &snapshot).unwrap();

    assert_eq!(index.total_count(), records.len());
    assert_eq!(index.structure_version, 7);
    assert_eq!(index.id_at(2), Some(300_062));
}
