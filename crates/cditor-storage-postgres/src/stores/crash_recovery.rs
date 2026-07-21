use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use cditor_core::ids::BlockId;
use cditor_storage::optimistic_persistence::OptimisticPersistenceManager;

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::queue::persistence::{PersistenceQueueRow, PostgresPersistenceQueue};
use crate::types::{PgBlockId, PgDocumentId, pg_block_id_from_runtime, runtime_block_id_from_pg};

#[derive(Debug, Clone)]
pub struct PostgresCrashRecoveryStore {
    pool: PgPool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSnapshotRecord {
    pub document_id: PgDocumentId,
    pub structure_version: i64,
    pub content_version: i64,
    pub focused_block_id: Option<BlockId>,
    pub selection_json: Option<serde_json::Value>,
    pub scroll_anchor_json: Option<serde_json::Value>,
    pub render_window_json: Option<serde_json::Value>,
    pub dirty_blocks: Vec<DirtyBlockRecoveryRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyBlockRecoveryRecord {
    pub block_id: BlockId,
    pub persisted_version: u64,
    pub memory_version: u64,
    pub save_failed: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeSnapshotLoadStatus {
    Loaded,
    Missing,
    CorruptFallback { message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSnapshotLoadResult {
    pub status: RuntimeSnapshotLoadStatus,
    pub snapshot: Option<RuntimeSnapshotRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartupRecoveryResult {
    pub reset_running_tasks: u64,
    pub retryable_tasks: Vec<PersistenceQueueRow>,
    pub snapshot_status: RuntimeSnapshotLoadStatus,
    pub dirty_blocks_restored: usize,
}

impl PostgresCrashRecoveryStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn save_runtime_snapshot(
        &self,
        snapshot: &RuntimeSnapshotRecord,
    ) -> PostgresStorageResult<()> {
        let focused_block_id = snapshot.focused_block_id.map(pg_block_id_from_runtime);
        let dirty_blocks_json = serde_json::to_value(&snapshot.dirty_blocks)?;

        sqlx::query(
            r#"
            INSERT INTO runtime_snapshots (
                document_id,
                structure_version,
                content_version,
                focused_block_id,
                selection_json,
                scroll_anchor_json,
                render_window_json,
                dirty_blocks_json,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())
            ON CONFLICT (document_id) DO UPDATE SET
                structure_version = EXCLUDED.structure_version,
                content_version = EXCLUDED.content_version,
                focused_block_id = EXCLUDED.focused_block_id,
                selection_json = EXCLUDED.selection_json,
                scroll_anchor_json = EXCLUDED.scroll_anchor_json,
                render_window_json = EXCLUDED.render_window_json,
                dirty_blocks_json = EXCLUDED.dirty_blocks_json,
                created_at = now()
            "#,
        )
        .bind(snapshot.document_id)
        .bind(snapshot.structure_version)
        .bind(snapshot.content_version)
        .bind(focused_block_id)
        .bind(&snapshot.selection_json)
        .bind(&snapshot.scroll_anchor_json)
        .bind(&snapshot.render_window_json)
        .bind(dirty_blocks_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn load_runtime_snapshot(
        &self,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<RuntimeSnapshotLoadResult> {
        let row = sqlx::query(
            r#"
            SELECT
                document_id,
                structure_version,
                content_version,
                focused_block_id,
                selection_json,
                scroll_anchor_json,
                render_window_json,
                dirty_blocks_json
            FROM runtime_snapshots
            WHERE document_id = $1
            "#,
        )
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(RuntimeSnapshotLoadResult {
                status: RuntimeSnapshotLoadStatus::Missing,
                snapshot: None,
            });
        };

        match runtime_snapshot_from_row(row) {
            Ok(snapshot) => Ok(RuntimeSnapshotLoadResult {
                status: RuntimeSnapshotLoadStatus::Loaded,
                snapshot: Some(snapshot),
            }),
            Err(error) => Ok(RuntimeSnapshotLoadResult {
                status: RuntimeSnapshotLoadStatus::CorruptFallback {
                    message: error.to_string(),
                },
                snapshot: None,
            }),
        }
    }

    pub async fn load_pending_queue_for_startup(
        &self,
        queue: &PostgresPersistenceQueue,
        document_id: PgDocumentId,
        limit: i64,
    ) -> PostgresStorageResult<Vec<PersistenceQueueRow>> {
        queue.load_ready(document_id, limit).await
    }

    pub async fn reset_running_and_load_retry_safe_tasks(
        &self,
        queue: &PostgresPersistenceQueue,
        document_id: PgDocumentId,
        limit: i64,
    ) -> PostgresStorageResult<(u64, Vec<PersistenceQueueRow>)> {
        let reset = queue.reset_running_to_pending(document_id).await?;
        let retryable = queue.load_ready(document_id, limit).await?;
        Ok((reset, retryable))
    }

    pub fn restore_dirty_blocks_pin(
        &self,
        snapshot: &RuntimeSnapshotRecord,
        manager: &mut OptimisticPersistenceManager,
    ) -> usize {
        for dirty in &snapshot.dirty_blocks {
            manager.track_clean_block(dirty.block_id, dirty.persisted_version);
            manager.apply_memory_edit(dirty.block_id, dirty.memory_version);
            if dirty.save_failed {
                manager.save_failed(
                    dirty.block_id,
                    dirty.memory_version,
                    "recovered dirty block from runtime snapshot",
                );
            }
        }
        snapshot.dirty_blocks.len()
    }

    pub async fn recover_startup_state(
        &self,
        queue: &PostgresPersistenceQueue,
        document_id: PgDocumentId,
        manager: &mut OptimisticPersistenceManager,
        limit: i64,
    ) -> PostgresStorageResult<StartupRecoveryResult> {
        let snapshot = self.load_runtime_snapshot(document_id).await?;
        let dirty_blocks_restored = snapshot
            .snapshot
            .as_ref()
            .map(|snapshot| self.restore_dirty_blocks_pin(snapshot, manager))
            .unwrap_or(0);
        let (reset_running_tasks, retryable_tasks) = self
            .reset_running_and_load_retry_safe_tasks(queue, document_id, limit)
            .await?;

        Ok(StartupRecoveryResult {
            reset_running_tasks,
            retryable_tasks,
            snapshot_status: snapshot.status,
            dirty_blocks_restored,
        })
    }
}

fn runtime_snapshot_from_row(
    row: sqlx::postgres::PgRow,
) -> PostgresStorageResult<RuntimeSnapshotRecord> {
    let focused_block_id: Option<PgBlockId> = row.try_get("focused_block_id")?;
    let dirty_blocks_json: Option<serde_json::Value> = row.try_get("dirty_blocks_json")?;
    let dirty_blocks = match dirty_blocks_json {
        Some(value) => serde_json::from_value::<Vec<DirtyBlockRecoveryRecord>>(value)?,
        None => Vec::new(),
    };

    Ok(RuntimeSnapshotRecord {
        document_id: row.try_get("document_id")?,
        structure_version: row.try_get("structure_version")?,
        content_version: row.try_get("content_version")?,
        focused_block_id: focused_block_id
            .map(|block_id| {
                runtime_block_id_from_pg(block_id).ok_or_else(|| {
                    PostgresStorageError::CorruptData {
                        message: format!(
                            "focused block id {block_id} is outside runtime namespace"
                        ),
                    }
                })
            })
            .transpose()?,
        selection_json: row.try_get("selection_json")?,
        scroll_anchor_json: row.try_get("scroll_anchor_json")?,
        render_window_json: row.try_get("render_window_json")?,
        dirty_blocks,
    })
}

#[cfg(test)]
mod tests {
    use sqlx::types::Uuid;

    use super::*;
    use crate::{
        DocumentRow, PersistenceQueueTask, PostgresDocumentStore, PostgresPersistenceQueue,
        PostgresPoolConfig, create_pg_pool, pg_document_id_from_runtime, run_migrations,
    };
    use cditor_storage::optimistic_persistence::PersistenceState;

    fn test_database_url() -> String {
        std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
    }

    async fn test_stores() -> (
        PostgresDocumentStore,
        PostgresPersistenceQueue,
        PostgresCrashRecoveryStore,
        DocumentRow,
    ) {
        let pool = create_pg_pool(&PostgresPoolConfig::for_tests(test_database_url()))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let document_store = PostgresDocumentStore::new(pool.clone());
        let queue = PostgresPersistenceQueue::new(pool.clone());
        let recovery = PostgresCrashRecoveryStore::new(pool);
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let runtime_document_id = 110_000 + suffix;
        let document = DocumentRow {
            id: pg_document_id_from_runtime(runtime_document_id),
            workspace_id: Uuid::from_u128(
                0x9800_0000_0000_0000_0000_0000_0000_0000 | runtime_document_id as u128,
            ),
            title: format!("Crash Recovery {runtime_document_id}"),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        };
        document_store
            .save_document_metadata(&document)
            .await
            .unwrap();
        (document_store, queue, recovery, document)
    }

    fn snapshot(document_id: PgDocumentId) -> RuntimeSnapshotRecord {
        RuntimeSnapshotRecord {
            document_id,
            structure_version: 2,
            content_version: 3,
            focused_block_id: Some(42),
            selection_json: Some(serde_json::json!({ "anchor": 42 })),
            scroll_anchor_json: Some(serde_json::json!({ "block_id": 42, "y": 12.0 })),
            render_window_json: Some(serde_json::json!({ "start": 0, "end": 10 })),
            dirty_blocks: vec![DirtyBlockRecoveryRecord {
                block_id: 42,
                persisted_version: 1,
                memory_version: 3,
                save_failed: true,
                last_error: Some("network".to_owned()),
            }],
        }
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_crash_recovery_saves_and_loads_runtime_snapshot() {
        let (_document_store, _queue, recovery, document) = test_stores().await;
        let snapshot = snapshot(document.id);

        recovery.save_runtime_snapshot(&snapshot).await.unwrap();
        let loaded = recovery.load_runtime_snapshot(document.id).await.unwrap();

        assert_eq!(loaded.status, RuntimeSnapshotLoadStatus::Loaded);
        assert_eq!(loaded.snapshot, Some(snapshot));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_crash_recovery_loads_pending_queue_and_retry_safe_tasks() {
        let (_document_store, queue, recovery, document) = test_stores().await;
        let pending = queue
            .enqueue(document.id, &PersistenceQueueTask::save_block_payload(1, 2))
            .await
            .unwrap();
        let running = queue
            .enqueue(document.id, &PersistenceQueueTask::save_block_payload(2, 3))
            .await
            .unwrap();
        queue.mark_running(running).await.unwrap();

        let ready_before = recovery
            .load_pending_queue_for_startup(&queue, document.id, 10)
            .await
            .unwrap();
        assert_eq!(
            ready_before.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![pending]
        );

        let (reset, retryable) = recovery
            .reset_running_and_load_retry_safe_tasks(&queue, document.id, 10)
            .await
            .unwrap();
        assert_eq!(reset, 1);
        assert_eq!(retryable.len(), 2);
        assert!(retryable.iter().any(|row| row.id == running));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_crash_recovery_restores_dirty_block_pin() {
        let (_document_store, _queue, recovery, document) = test_stores().await;
        let snapshot = snapshot(document.id);
        recovery.save_runtime_snapshot(&snapshot).await.unwrap();
        let loaded = recovery.load_runtime_snapshot(document.id).await.unwrap();
        let mut manager = OptimisticPersistenceManager::default();

        let restored =
            recovery.restore_dirty_blocks_pin(loaded.snapshot.as_ref().unwrap(), &mut manager);

        assert_eq!(restored, 1);
        assert_eq!(
            manager.state(42).unwrap().state,
            PersistenceState::SaveFailed
        );
        assert!(manager.pinned_blocks().contains(&42));
        assert_eq!(manager.recovery_queue().front(), Some(&42));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_crash_recovery_corrupt_snapshot_falls_back_without_error() {
        let (_document_store, _queue, recovery, document) = test_stores().await;
        sqlx::query(
            r#"
            INSERT INTO runtime_snapshots (
                document_id,
                structure_version,
                content_version,
                dirty_blocks_json
            )
            VALUES ($1, 1, 1, '{"not":"an array"}'::jsonb)
            ON CONFLICT (document_id) DO UPDATE SET dirty_blocks_json = EXCLUDED.dirty_blocks_json
            "#,
        )
        .bind(document.id)
        .execute(recovery.pool())
        .await
        .unwrap();

        let loaded = recovery.load_runtime_snapshot(document.id).await.unwrap();
        assert!(matches!(
            loaded.status,
            RuntimeSnapshotLoadStatus::CorruptFallback { .. }
        ));
        assert!(loaded.snapshot.is_none());
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_crash_recovery_startup_combines_snapshot_and_queue_recovery() {
        let (_document_store, queue, recovery, document) = test_stores().await;
        recovery
            .save_runtime_snapshot(&snapshot(document.id))
            .await
            .unwrap();
        let task_id = queue
            .enqueue(
                document.id,
                &PersistenceQueueTask::save_block_payload(42, 3),
            )
            .await
            .unwrap();
        queue.mark_running(task_id).await.unwrap();
        let mut manager = OptimisticPersistenceManager::default();

        let result = recovery
            .recover_startup_state(&queue, document.id, &mut manager, 10)
            .await
            .unwrap();

        assert_eq!(result.reset_running_tasks, 1);
        assert_eq!(result.retryable_tasks.len(), 1);
        assert_eq!(result.dirty_blocks_restored, 1);
        assert_eq!(result.snapshot_status, RuntimeSnapshotLoadStatus::Loaded);
        assert!(manager.pinned_blocks().contains(&42));
    }
}
