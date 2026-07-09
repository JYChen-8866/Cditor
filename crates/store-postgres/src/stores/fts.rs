use sqlx::{PgPool, Row};

use cditor_core::ids::BlockId;
use cditor_core::rich_text::RichBlockKind;

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::queue::persistence::{
    PersistenceQueueTask, PersistenceTaskKind, PostgresPersistenceQueue,
};
use crate::types::{
    PgDocumentId, pg_block_id_from_runtime, rich_block_kind_to_db, runtime_block_id_from_pg,
};

#[derive(Debug, Clone)]
pub struct PostgresFtsStore {
    pool: PgPool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtsUpsertResult {
    Applied,
    DiscardedStaleVersion {
        current_version: u64,
        incoming_version: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FtsSearchResult {
    pub block_id: BlockId,
    pub content_version: u64,
    pub score: f32,
    pub snippet: String,
}

impl PostgresFtsStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn upsert_block_text(
        &self,
        document_id: PgDocumentId,
        block_id: BlockId,
        kind: &RichBlockKind,
        plain_text: &str,
        content_version: u64,
    ) -> PostgresStorageResult<FtsUpsertResult> {
        let pg_block_id = pg_block_id_from_runtime(block_id);
        let current_version = sqlx::query_scalar::<_, Option<i64>>(
            r#"
            SELECT GREATEST(
                COALESCE((SELECT content_version FROM blocks WHERE id = $1 AND document_id = $2 AND deleted_at IS NULL), 0),
                COALESCE((SELECT content_version FROM block_search WHERE block_id = $1 AND document_id = $2), 0)
            )
            "#,
        )
        .bind(pg_block_id)
        .bind(document_id)
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(0);
        let current_version =
            u64::try_from(current_version).map_err(|_| PostgresStorageError::CorruptData {
                message: format!("negative FTS current_version {current_version}"),
            })?;

        if content_version < current_version {
            return Ok(FtsUpsertResult::DiscardedStaleVersion {
                current_version,
                incoming_version: content_version,
            });
        }

        let content_version =
            i64::try_from(content_version).map_err(|_| PostgresStorageError::CorruptData {
                message: "FTS content_version exceeds PostgreSQL BIGINT range".to_owned(),
            })?;
        let kind = rich_block_kind_to_db(kind);

        sqlx::query(
            r#"
            INSERT INTO block_search (
                block_id,
                document_id,
                kind,
                plain_text,
                search_vector,
                content_version,
                indexed_at
            )
            VALUES ($1, $2, $3, $4, to_tsvector('simple', $4), $5, now())
            ON CONFLICT (block_id) DO UPDATE SET
                document_id = EXCLUDED.document_id,
                kind = EXCLUDED.kind,
                plain_text = EXCLUDED.plain_text,
                search_vector = EXCLUDED.search_vector,
                content_version = EXCLUDED.content_version,
                indexed_at = now()
            WHERE block_search.content_version <= EXCLUDED.content_version
            "#,
        )
        .bind(pg_block_id)
        .bind(document_id)
        .bind(kind)
        .bind(plain_text)
        .bind(content_version)
        .execute(&self.pool)
        .await?;

        Ok(FtsUpsertResult::Applied)
    }

    pub async fn delete_block_text(&self, block_id: BlockId) -> PostgresStorageResult<bool> {
        let result = sqlx::query("DELETE FROM block_search WHERE block_id = $1")
            .bind(pg_block_id_from_runtime(block_id))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn search(
        &self,
        document_id: PgDocumentId,
        query: &str,
        limit: i64,
    ) -> PostgresStorageResult<Vec<FtsSearchResult>> {
        if query.trim().is_empty() || limit <= 0 {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT
                block_id,
                content_version,
                ts_rank_cd(search_vector, plainto_tsquery('simple', $2))::real AS score,
                ts_headline('simple', plain_text, plainto_tsquery('simple', $2), 'StartSel=<mark>, StopSel=</mark>, MaxFragments=2') AS snippet
            FROM block_search
            WHERE document_id = $1 AND search_vector @@ plainto_tsquery('simple', $2)
            ORDER BY score DESC, indexed_at DESC, block_id
            LIMIT $3
            "#,
        )
        .bind(document_id)
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let pg_block_id = row.try_get("block_id")?;
                let block_id = runtime_block_id_from_pg(pg_block_id).ok_or_else(|| {
                    PostgresStorageError::CorruptData {
                        message: format!("block id {pg_block_id} is outside runtime namespace"),
                    }
                })?;
                let content_version: i64 = row.try_get("content_version")?;
                Ok(FtsSearchResult {
                    block_id,
                    content_version: u64::try_from(content_version).map_err(|_| {
                        PostgresStorageError::CorruptData {
                            message: format!("negative FTS content_version {content_version}"),
                        }
                    })?,
                    score: row.try_get::<f32, _>("score")?,
                    snippet: row.try_get("snippet")?,
                })
            })
            .collect()
    }

    pub async fn enqueue_repair_task(
        &self,
        queue: &PostgresPersistenceQueue,
        document_id: PgDocumentId,
        block_ids: Vec<BlockId>,
        error: &str,
    ) -> PostgresStorageResult<sqlx::types::Uuid> {
        let task = PersistenceQueueTask {
            kind: PersistenceTaskKind::RepairFts,
            payload: serde_json::json!({
                "reason": error,
                "block_ids": block_ids,
            }),
            affected_blocks: block_ids,
        };
        queue.enqueue(document_id, &task).await
    }
}

#[cfg(test)]
mod tests {
    use sqlx::types::Uuid;

    use super::*;
    use crate::{
        DocumentRow, PostgresDocumentStore, PostgresPayloadStore, PostgresPersistenceQueue,
        PostgresPoolConfig, create_pg_pool, pg_document_id_from_runtime, run_migrations,
    };
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};

    fn test_database_url() -> String {
        std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
    }

    async fn test_stores() -> (
        PostgresDocumentStore,
        PostgresPayloadStore,
        PostgresFtsStore,
        PostgresPersistenceQueue,
        DocumentRow,
        u64,
    ) {
        let pool = create_pg_pool(&PostgresPoolConfig::for_tests(test_database_url()))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let document_store = PostgresDocumentStore::new(pool.clone());
        let payload_store = PostgresPayloadStore::new(pool.clone());
        let fts_store = PostgresFtsStore::new(pool.clone());
        let queue = PostgresPersistenceQueue::new(pool);
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let runtime_document_id = 90_000 + suffix;
        let document = DocumentRow {
            id: pg_document_id_from_runtime(runtime_document_id),
            workspace_id: Uuid::from_u128(
                0x9600_0000_0000_0000_0000_0000_0000_0000 | runtime_document_id as u128,
            ),
            title: format!("FTS {runtime_document_id}"),
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
        document_store
            .save_block_index_records(
                document.id,
                &[
                    BlockIndexRecord::new(
                        base_block_id,
                        None,
                        0,
                        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                        0,
                    ),
                    BlockIndexRecord::new(
                        base_block_id + 1,
                        None,
                        0,
                        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                        0,
                    ),
                ],
                1,
            )
            .await
            .unwrap();
        (
            document_store,
            payload_store,
            fts_store,
            queue,
            document,
            base_block_id,
        )
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_fts_store_upserts_searches_and_deletes() {
        let (_document_store, _payload_store, fts_store, _queue, document, base) =
            test_stores().await;

        assert_eq!(
            fts_store
                .upsert_block_text(
                    document.id,
                    base,
                    &RichBlockKind::Paragraph,
                    "postgres rich text search architecture",
                    1,
                )
                .await
                .unwrap(),
            FtsUpsertResult::Applied
        );
        fts_store
            .upsert_block_text(
                document.id,
                base + 1,
                &RichBlockKind::Paragraph,
                "unrelated document block",
                1,
            )
            .await
            .unwrap();

        let results = fts_store
            .search(document.id, "rich search", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].block_id, base);
        assert!(results[0].snippet.contains("<mark>"));

        assert!(fts_store.delete_block_text(base).await.unwrap());
        assert!(
            fts_store
                .search(document.id, "rich search", 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_fts_store_discards_stale_content_version() {
        let (_document_store, payload_store, fts_store, _queue, document, base) =
            test_stores().await;
        payload_store
            .save_block_payloads(
                document.id,
                &[BlockPayloadRecord::rich_text(
                    base,
                    RichBlockKind::Paragraph,
                    "current version text",
                )],
            )
            .await
            .unwrap();

        let result = fts_store
            .upsert_block_text(document.id, base, &RichBlockKind::Paragraph, "old text", 0)
            .await
            .unwrap();

        assert_eq!(
            result,
            FtsUpsertResult::DiscardedStaleVersion {
                current_version: 1,
                incoming_version: 0,
            }
        );
        assert!(
            fts_store
                .search(document.id, "old", 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_fts_store_enqueues_repair_task_on_failure_path() {
        let (_document_store, _payload_store, fts_store, queue, document, base) =
            test_stores().await;

        let task_id = fts_store
            .enqueue_repair_task(
                &queue,
                document.id,
                vec![base, base + 1],
                "fts write failed",
            )
            .await
            .unwrap();
        let ready = queue.load_ready(document.id, 10).await.unwrap();

        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, task_id);
        assert_eq!(ready[0].task.kind, PersistenceTaskKind::RepairFts);
        assert_eq!(ready[0].task.affected_blocks, vec![base, base + 1]);
    }
}
