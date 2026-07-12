#[cfg(test)]
mod tests {
    use sqlx::types::Uuid;

    use crate::{
        DocumentRow, EditTransactionVersions, PostgresDocumentStore, PostgresFtsStore,
        PostgresLayoutCacheStore, PostgresPayloadStore, PostgresPoolConfig,
        PostgresTransactionStore, create_pg_pool, pg_document_id_from_runtime, run_migrations,
    };
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::edit::EditTransaction;
    use cditor_core::layout::{HeightConfidence, HeightEstimate};
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
    use cditor_storage::layout_cache::{BlockLayoutRow, CacheSource, LayoutCacheKey};
    use cditor_storage::optimistic_persistence::{OptimisticPersistenceManager, PersistenceState};

    struct IntegrationStores {
        document: PostgresDocumentStore,
        payload: PostgresPayloadStore,
        layout: PostgresLayoutCacheStore,
        transaction: PostgresTransactionStore,
        fts: PostgresFtsStore,
    }

    fn test_database_url() -> String {
        std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
    }

    async fn stores() -> IntegrationStores {
        let pool = create_pg_pool(&PostgresPoolConfig::for_tests(test_database_url()))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        IntegrationStores {
            document: PostgresDocumentStore::new(pool.clone()),
            payload: PostgresPayloadStore::new(pool.clone()),
            layout: PostgresLayoutCacheStore::new(pool.clone()),
            transaction: PostgresTransactionStore::new(pool.clone()),
            fts: PostgresFtsStore::new(pool),
        }
    }

    fn unique_runtime_document_id(seed: u64) -> u64 {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        seed + suffix
    }

    fn document_row(runtime_document_id: u64) -> DocumentRow {
        DocumentRow {
            id: pg_document_id_from_runtime(runtime_document_id),
            workspace_id: Uuid::from_u128(
                0x9a00_0000_0000_0000_0000_0000_0000_0000 | runtime_document_id as u128,
            ),
            title: format!("Postgres Integration {runtime_document_id}"),
            structure_version: 1,
            content_version: 1,
            layout_version: 1,
            schema_version: 1,
        }
    }

    fn paragraph_tag() -> u16 {
        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph)
    }

    fn demo_records(block_base: u64) -> Vec<BlockIndexRecord> {
        vec![
            BlockIndexRecord::new(block_base, None, 0, paragraph_tag(), 0),
            BlockIndexRecord::new(block_base + 1, None, 0, paragraph_tag(), 0),
            BlockIndexRecord::new(block_base + 2, Some(block_base + 1), 1, paragraph_tag(), 0),
        ]
    }

    fn demo_payloads(block_base: u64) -> Vec<BlockPayloadRecord> {
        vec![
            BlockPayloadRecord::rich_text(
                block_base,
                RichBlockKind::Paragraph,
                "demo document title",
            ),
            BlockPayloadRecord::rich_text(
                block_base + 1,
                RichBlockKind::Paragraph,
                "hello postgres integration",
            ),
            BlockPayloadRecord::rich_text(
                block_base + 2,
                RichBlockKind::Paragraph,
                "nested searchable child",
            ),
        ]
    }

    async fn seed_demo_document(
        stores: &IntegrationStores,
        runtime_document_id: u64,
        block_base: u64,
    ) -> DocumentRow {
        let document = document_row(runtime_document_id);
        stores
            .document
            .save_document_metadata(&document)
            .await
            .unwrap();
        stores
            .document
            .save_block_index_records(document.id, &demo_records(block_base), 1)
            .await
            .unwrap();
        stores
            .payload
            .save_block_payloads(document.id, &demo_payloads(block_base))
            .await
            .unwrap();
        document
    }

    fn versions() -> EditTransactionVersions {
        EditTransactionVersions {
            structure_version_before: Some(1),
            structure_version_after: Some(1),
            content_version_after: Some(2),
        }
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_integration_demo_document_save_open_edit_and_reopen() {
        let stores = stores().await;
        let runtime_document_id = unique_runtime_document_id(130_000);
        let block_base = runtime_document_id * 10;
        let document = seed_demo_document(&stores, runtime_document_id, block_base).await;

        let opened_metadata = stores
            .document
            .load_document_metadata(document.id)
            .await
            .unwrap();
        let opened_index = stores
            .document
            .load_block_index_records(document.id)
            .await
            .unwrap();
        let opened_payloads = stores
            .payload
            .load_block_payloads(&[block_base, block_base + 1, block_base + 2])
            .await
            .unwrap();
        assert_eq!(opened_metadata.title, document.title);
        assert_eq!(opened_index.len(), 3);
        assert_eq!(opened_payloads.records.len(), 3);

        let edited_payload = BlockPayloadRecord::rich_text(
            block_base + 1,
            RichBlockKind::Paragraph,
            "hello postgres integration edited",
        );
        stores
            .payload
            .save_block_payloads(document.id, &[edited_payload])
            .await
            .unwrap();
        stores
            .transaction
            .save_edit_transaction(
                document.id,
                &EditTransaction::insert_text(
                    910_001 + runtime_document_id,
                    1_700_000_000_000,
                    block_base + 1,
                    27,
                    " edited",
                ),
                versions(),
            )
            .await
            .unwrap();

        let reopened = stores
            .payload
            .load_block_payloads(&[block_base + 1])
            .await
            .unwrap();
        assert_eq!(
            reopened.records[0].plain_text(),
            "hello postgres integration edited"
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL; inserts 100k blocks"]
    async fn postgres_integration_100k_open_loads_index_without_full_payloads() {
        let stores = stores().await;
        let runtime_document_id = unique_runtime_document_id(140_000);
        let block_base = runtime_document_id * 1_000;
        let document = document_row(runtime_document_id);
        stores
            .document
            .save_document_metadata(&document)
            .await
            .unwrap();
        let records = (0..100_000_u64)
            .map(|offset| BlockIndexRecord::new(block_base + offset, None, 0, paragraph_tag(), 0))
            .collect::<Vec<_>>();
        stores
            .document
            .save_block_index_records(document.id, &records, 1)
            .await
            .unwrap();
        let first_window_payloads = (0..5_u64)
            .map(|offset| {
                BlockPayloadRecord::rich_text(
                    block_base + offset,
                    RichBlockKind::Paragraph,
                    format!("visible payload {offset}"),
                )
            })
            .collect::<Vec<_>>();
        stores
            .payload
            .save_block_payloads(document.id, &first_window_payloads)
            .await
            .unwrap();

        let loaded_index = stores
            .document
            .load_block_index_records(document.id)
            .await
            .unwrap();
        let payload_window = stores
            .payload
            .load_block_payloads(&[
                block_base,
                block_base + 1,
                block_base + 2,
                block_base + 3,
                block_base + 4,
            ])
            .await
            .unwrap();
        let payload_rows_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM block_payloads WHERE document_id = $1")
                .bind(document.id)
                .fetch_one(stores.payload.pool())
                .await
                .unwrap();

        assert_eq!(loaded_index.len(), 100_000);
        assert_eq!(payload_window.records.len(), 5);
        assert_eq!(payload_rows_count, 5);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_integration_layout_cache_survives_store_reopen() {
        let stores = stores().await;
        let runtime_document_id = unique_runtime_document_id(150_000);
        let block_base = runtime_document_id * 10;
        let document = seed_demo_document(&stores, runtime_document_id, block_base).await;
        let key = LayoutCacheKey {
            width_bucket: 800,
            exact_width_px: 800,
            content_version: 1,
            attrs_version: 1,
            style_version: 1,
            font_version: 1,
            theme_version: 1,
            scale_factor_milli: 1000,
        };
        let row = BlockLayoutRow::new(
            block_base,
            key,
            HeightEstimate {
                height: 64.0,
                confidence: HeightConfidence::Exact,
                max_error_hint: 0.0,
            },
        );
        stores
            .layout
            .save_block_layout(document.id, &row)
            .await
            .unwrap();

        let reopened_layout_store = PostgresLayoutCacheStore::new(stores.layout.pool().clone());
        let cached = reopened_layout_store
            .load_block_height(block_base, key)
            .await
            .unwrap();

        assert_eq!(cached.height, 64.0);
        assert_eq!(cached.source, CacheSource::ExactMatch);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_integration_large_paste_writes_transaction_and_undo_snapshot() {
        let stores = stores().await;
        let runtime_document_id = unique_runtime_document_id(160_000);
        let block_base = runtime_document_id * 10;
        let document = seed_demo_document(&stores, runtime_document_id, block_base).await;
        let large_blocks = (0..32)
            .map(|offset| {
                BlockIndexRecord::new(block_base + 10_000 + offset, None, 0, paragraph_tag(), 0)
            })
            .collect::<Vec<_>>();
        let tx = EditTransaction::paste_blocks(
            920_001 + runtime_document_id,
            1_700_000_000_001,
            3,
            large_blocks,
        );
        let transaction_store = stores
            .transaction
            .clone()
            .with_large_snapshot_block_threshold(10);

        transaction_store
            .save_edit_transaction(document.id, &tx, versions())
            .await
            .unwrap();
        let recent = transaction_store
            .load_recent_transactions(document.id, 1)
            .await
            .unwrap();
        let snapshots = transaction_store
            .load_undo_snapshots(document.id, tx.id)
            .await
            .unwrap();

        assert_eq!(recent[0].transaction.id, tx.id);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].block_count, 32);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_integration_search_result_can_jump_to_block() {
        let stores = stores().await;
        let runtime_document_id = unique_runtime_document_id(170_000);
        let block_base = runtime_document_id * 10;
        let document = seed_demo_document(&stores, runtime_document_id, block_base).await;
        let index_records = stores
            .document
            .load_block_index_records(document.id)
            .await
            .unwrap();
        let document_index =
            cditor_core::document::DocumentIndex::new(runtime_document_id, index_records, 1)
                .unwrap();

        let results = stores
            .fts
            .search(document.id, "searchable", 10)
            .await
            .unwrap();

        assert_eq!(results[0].block_id, block_base + 2);
        assert_eq!(document_index.index_of(results[0].block_id), Some(2));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_integration_save_failure_keeps_close_guard_active() {
        let stores = stores().await;
        let runtime_document_id = unique_runtime_document_id(180_000);
        let block_base = runtime_document_id * 10;
        seed_demo_document(&stores, runtime_document_id, block_base).await;
        let mut manager = OptimisticPersistenceManager::default();
        manager.track_clean_block(block_base, 1);
        manager.apply_memory_edit(block_base, 2);
        manager.begin_save(block_base);
        manager.save_failed(block_base, 2, "postgres unavailable");

        let report = manager.close_guard_report();
        assert!(!report.can_close_without_prompt);
        assert_eq!(report.save_failed_blocks, vec![block_base]);
        assert_eq!(
            manager.state(block_base).unwrap().state,
            PersistenceState::SaveFailed
        );
        assert!(manager.pinned_blocks().contains(&block_base));
    }

    // P-007: 保存后重新打开，表格结构一致
    mod table;
}
