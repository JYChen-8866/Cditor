use serde::{Deserialize, Serialize};
use sqlx::types::{Json, Uuid};
use sqlx::{PgPool, Row};
use tokio::sync::{mpsc, oneshot};

use cditor_core::ids::BlockId;
use cditor_storage::optimistic_persistence::OptimisticPersistenceManager;

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::PgDocumentId;

#[derive(Debug, Clone)]
pub struct PostgresPersistenceQueue {
    pool: PgPool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceTaskKind {
    SaveBlockPayloads,
    SaveEditTransaction,
    SaveLayoutCache,
    UpdateFts,
    RepairFts,
    SyncOutbox,
    Custom(String),
}

impl PersistenceTaskKind {
    fn as_db(&self) -> String {
        match self {
            Self::SaveBlockPayloads => "save_block_payloads".to_owned(),
            Self::SaveEditTransaction => "save_edit_transaction".to_owned(),
            Self::SaveLayoutCache => "save_layout_cache".to_owned(),
            Self::UpdateFts => "update_fts".to_owned(),
            Self::RepairFts => "repair_fts".to_owned(),
            Self::SyncOutbox => "sync_outbox".to_owned(),
            Self::Custom(kind) => format!("custom:{kind}"),
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "save_block_payloads" => Self::SaveBlockPayloads,
            "save_edit_transaction" => Self::SaveEditTransaction,
            "save_layout_cache" => Self::SaveLayoutCache,
            "update_fts" => Self::UpdateFts,
            "repair_fts" => Self::RepairFts,
            "sync_outbox" => Self::SyncOutbox,
            _ => value
                .strip_prefix("custom:")
                .map(|kind| Self::Custom(kind.to_owned()))
                .unwrap_or_else(|| Self::Custom(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceQueueState {
    Pending,
    Running,
    Failed,
}

impl PersistenceQueueState {
    fn from_db(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistenceQueueTask {
    pub kind: PersistenceTaskKind,
    pub payload: serde_json::Value,
    pub affected_blocks: Vec<BlockId>,
}

impl PersistenceQueueTask {
    pub fn save_block_payload(block_id: BlockId, memory_version: u64) -> Self {
        Self {
            kind: PersistenceTaskKind::SaveBlockPayloads,
            payload: serde_json::json!({
                "block_id": block_id,
                "memory_version": memory_version,
            }),
            affected_blocks: vec![block_id],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersistenceQueueRow {
    pub id: Uuid,
    pub document_id: PgDocumentId,
    pub task: PersistenceQueueTask,
    pub state: PersistenceQueueState,
    pub attempt_count: i32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerProcessReport {
    pub loaded: usize,
    pub succeeded: usize,
    pub failed: usize,
}

pub enum PersistenceWorkerCommand {
    Enqueue {
        document_id: PgDocumentId,
        task: PersistenceQueueTask,
        respond_to: oneshot::Sender<PostgresStorageResult<Uuid>>,
    },
    ProcessReady {
        document_id: PgDocumentId,
        limit: i64,
        respond_to: oneshot::Sender<PostgresStorageResult<WorkerProcessReport>>,
    },
    ShutdownFlush {
        document_id: PgDocumentId,
        respond_to: oneshot::Sender<PostgresStorageResult<WorkerProcessReport>>,
    },
    Stop,
}

impl PostgresPersistenceQueue {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn enqueue(
        &self,
        document_id: PgDocumentId,
        task: &PersistenceQueueTask,
    ) -> PostgresStorageResult<Uuid> {
        let affected_blocks_json = serde_json::to_value(&task.affected_blocks)?;
        let row = sqlx::query(
            r#"
            INSERT INTO persistence_queue (
                document_id,
                task_kind,
                task_json,
                affected_blocks_json,
                state,
                attempt_count,
                updated_at
            )
            VALUES ($1, $2, $3, $4, 'pending', 0, now())
            RETURNING id
            "#,
        )
        .bind(document_id)
        .bind(task.kind.as_db())
        .bind(Json(&task.payload))
        .bind(affected_blocks_json)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get("id")?)
    }

    pub async fn enqueue_save_from_optimistic_manager(
        &self,
        document_id: PgDocumentId,
        manager: &mut OptimisticPersistenceManager,
        block_id: BlockId,
    ) -> PostgresStorageResult<Option<Uuid>> {
        let Some(event) = manager.begin_save(block_id) else {
            return Ok(None);
        };
        let cditor_storage::optimistic_persistence::PersistenceEvent::SaveStarted {
            saving_version,
            ..
        } = event
        else {
            return Ok(None);
        };
        let task = PersistenceQueueTask::save_block_payload(block_id, saving_version);
        self.enqueue(document_id, &task).await.map(Some)
    }

    pub async fn load_ready(
        &self,
        document_id: PgDocumentId,
        limit: i64,
    ) -> PostgresStorageResult<Vec<PersistenceQueueRow>> {
        let rows = sqlx::query(
            r#"
            SELECT id, document_id, task_kind, task_json, affected_blocks_json, state, attempt_count, last_error
            FROM persistence_queue
            WHERE document_id = $1
              AND (
                  state = 'pending'
                  OR (state = 'failed' AND (next_retry_at IS NULL OR next_retry_at <= now()))
              )
            ORDER BY created_at, id
            LIMIT $2
            "#,
        )
        .bind(document_id)
        .bind(limit.max(0))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_from_pg).collect()
    }

    pub async fn mark_running(&self, task_id: Uuid) -> PostgresStorageResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE persistence_queue
            SET state = 'running', attempt_count = attempt_count + 1, updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        ensure_affected(result.rows_affected(), "persistence_queue", task_id)
    }

    pub async fn mark_succeeded(&self, task_id: Uuid) -> PostgresStorageResult<()> {
        let result = sqlx::query("DELETE FROM persistence_queue WHERE id = $1")
            .bind(task_id)
            .execute(&self.pool)
            .await?;
        ensure_affected(result.rows_affected(), "persistence_queue", task_id)
    }

    pub async fn mark_failed(
        &self,
        task_id: Uuid,
        error: &str,
        retry_after_ms: i64,
    ) -> PostgresStorageResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE persistence_queue
            SET state = 'failed',
                last_error = $2,
                next_retry_at = now() + ($3::double precision / 1000.0) * interval '1 second',
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(task_id)
        .bind(error)
        .bind(retry_after_ms.max(0))
        .execute(&self.pool)
        .await?;
        ensure_affected(result.rows_affected(), "persistence_queue", task_id)
    }

    pub async fn reset_running_to_pending(
        &self,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE persistence_queue
            SET state = 'pending', updated_at = now(), next_retry_at = NULL
            WHERE document_id = $1 AND state = 'running'
            "#,
        )
        .bind(document_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn process_ready_as_success(
        &self,
        document_id: PgDocumentId,
        limit: i64,
    ) -> PostgresStorageResult<WorkerProcessReport> {
        let rows = self.load_ready(document_id, limit).await?;
        let mut report = WorkerProcessReport {
            loaded: rows.len(),
            succeeded: 0,
            failed: 0,
        };
        for row in rows {
            self.mark_running(row.id).await?;
            self.mark_succeeded(row.id).await?;
            report.succeeded += 1;
        }
        Ok(report)
    }

    pub async fn run_worker_loop(self, mut receiver: mpsc::Receiver<PersistenceWorkerCommand>) {
        while let Some(command) = receiver.recv().await {
            match command {
                PersistenceWorkerCommand::Enqueue {
                    document_id,
                    task,
                    respond_to,
                } => {
                    let _ = respond_to.send(self.enqueue(document_id, &task).await);
                }
                PersistenceWorkerCommand::ProcessReady {
                    document_id,
                    limit,
                    respond_to,
                } => {
                    let _ =
                        respond_to.send(self.process_ready_as_success(document_id, limit).await);
                }
                PersistenceWorkerCommand::ShutdownFlush {
                    document_id,
                    respond_to,
                } => {
                    let result = async {
                        self.reset_running_to_pending(document_id).await?;
                        self.process_ready_as_success(document_id, i64::MAX).await
                    }
                    .await;
                    let _ = respond_to.send(result);
                }
                PersistenceWorkerCommand::Stop => break,
            }
        }
    }
}

fn row_from_pg(row: sqlx::postgres::PgRow) -> PostgresStorageResult<PersistenceQueueRow> {
    let task_kind: String = row.try_get("task_kind")?;
    let task_json: serde_json::Value = row.try_get::<Json<serde_json::Value>, _>("task_json")?.0;
    let affected_blocks: Vec<BlockId> =
        serde_json::from_value(row.try_get("affected_blocks_json")?)?;
    let state: String = row.try_get("state")?;

    Ok(PersistenceQueueRow {
        id: row.try_get("id")?,
        document_id: row.try_get("document_id")?,
        task: PersistenceQueueTask {
            kind: PersistenceTaskKind::from_db(&task_kind),
            payload: task_json,
            affected_blocks,
        },
        state: PersistenceQueueState::from_db(&state),
        attempt_count: row.try_get("attempt_count")?,
        last_error: row.try_get("last_error")?,
    })
}

fn ensure_affected(rows: u64, entity: &'static str, id: Uuid) -> PostgresStorageResult<()> {
    if rows == 0 {
        Err(PostgresStorageError::NotFound {
            entity,
            id: id.to_string(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sqlx::types::Uuid;

    use super::*;
    use crate::{
        DocumentRow, PostgresDocumentStore, PostgresPoolConfig, create_pg_pool,
        pg_document_id_from_runtime, run_migrations,
    };
    use cditor_storage::optimistic_persistence::PersistenceState;

    fn test_database_url() -> String {
        std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
    }

    async fn test_stores() -> (PostgresDocumentStore, PostgresPersistenceQueue, DocumentRow) {
        let config = PostgresPoolConfig::for_tests(test_database_url());
        let pool = create_pg_pool(&config).await.unwrap();
        run_migrations(&pool).await.unwrap();
        let document_store = PostgresDocumentStore::new(pool.clone());
        let queue = PostgresPersistenceQueue::new(pool);
        let document = document_row(70_001 + random_suffix());
        document_store
            .save_document_metadata(&document)
            .await
            .unwrap();
        (document_store, queue, document)
    }

    fn random_suffix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64
    }

    fn document_row(document_id: u64) -> DocumentRow {
        DocumentRow {
            id: pg_document_id_from_runtime(document_id),
            workspace_id: Uuid::from_u128(
                0x9400_0000_0000_0000_0000_0000_0000_0000 | document_id as u128,
            ),
            title: format!("Persistence Queue {document_id}"),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        }
    }

    fn sample_task(block_id: BlockId) -> PersistenceQueueTask {
        PersistenceQueueTask::save_block_payload(block_id, 2)
    }

    #[test]
    fn task_kind_round_trips_db_string() {
        assert_eq!(
            PersistenceTaskKind::from_db(&PersistenceTaskKind::SaveBlockPayloads.as_db()),
            PersistenceTaskKind::SaveBlockPayloads
        );
        assert_eq!(
            PersistenceTaskKind::from_db("custom:thing"),
            PersistenceTaskKind::Custom("thing".to_owned())
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_persistence_queue_enqueues_and_loads_ready_tasks() {
        let (_document_store, queue, document) = test_stores().await;
        let id = queue.enqueue(document.id, &sample_task(1)).await.unwrap();

        let ready = queue.load_ready(document.id, 10).await.unwrap();

        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, id);
        assert_eq!(ready[0].task.affected_blocks, vec![1]);
        assert_eq!(ready[0].state, PersistenceQueueState::Pending);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_persistence_queue_failed_task_waits_until_retry() {
        let (_document_store, queue, document) = test_stores().await;
        let id = queue.enqueue(document.id, &sample_task(2)).await.unwrap();

        queue.mark_running(id).await.unwrap();
        queue.mark_failed(id, "network down", 60_000).await.unwrap();
        assert!(queue.load_ready(document.id, 10).await.unwrap().is_empty());

        queue.mark_failed(id, "retry now", 0).await.unwrap();
        let ready = queue.load_ready(document.id, 10).await.unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].state, PersistenceQueueState::Failed);
        assert_eq!(ready[0].last_error.as_deref(), Some("retry now"));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_persistence_queue_success_deletes_task() {
        let (_document_store, queue, document) = test_stores().await;
        let id = queue.enqueue(document.id, &sample_task(3)).await.unwrap();

        queue.mark_running(id).await.unwrap();
        queue.mark_succeeded(id).await.unwrap();

        assert!(queue.load_ready(document.id, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_persistence_worker_loop_processes_ready_and_shutdown_flush() {
        let (_document_store, queue, document) = test_stores().await;
        let (sender, receiver) = mpsc::channel(8);
        let worker_queue = queue.clone();
        let worker = tokio::spawn(async move { worker_queue.run_worker_loop(receiver).await });

        let (enqueue_tx, enqueue_rx) = oneshot::channel();
        sender
            .send(PersistenceWorkerCommand::Enqueue {
                document_id: document.id,
                task: sample_task(4),
                respond_to: enqueue_tx,
            })
            .await
            .unwrap();
        let _task_id = enqueue_rx.await.unwrap().unwrap();

        let (process_tx, process_rx) = oneshot::channel();
        sender
            .send(PersistenceWorkerCommand::ProcessReady {
                document_id: document.id,
                limit: 10,
                respond_to: process_tx,
            })
            .await
            .unwrap();
        let report = process_rx.await.unwrap().unwrap();
        assert_eq!(report.loaded, 1);
        assert_eq!(report.succeeded, 1);

        let stuck_id = queue.enqueue(document.id, &sample_task(5)).await.unwrap();
        queue.mark_running(stuck_id).await.unwrap();
        let (flush_tx, flush_rx) = oneshot::channel();
        sender
            .send(PersistenceWorkerCommand::ShutdownFlush {
                document_id: document.id,
                respond_to: flush_tx,
            })
            .await
            .unwrap();
        let flush_report = flush_rx.await.unwrap().unwrap();
        assert_eq!(flush_report.succeeded, 1);

        sender.send(PersistenceWorkerCommand::Stop).await.unwrap();
        worker.await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_persistence_queue_integrates_with_optimistic_manager_and_recovers_failure() {
        let (_document_store, queue, document) = test_stores().await;
        let mut manager = OptimisticPersistenceManager::default();
        manager.track_clean_block(42, 1);
        manager.apply_memory_edit(42, 2);

        let task_id = queue
            .enqueue_save_from_optimistic_manager(document.id, &mut manager, 42)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(manager.state(42).unwrap().state, PersistenceState::Saving);

        queue.mark_running(task_id).await.unwrap();
        queue
            .mark_failed(task_id, "postgres unavailable", 0)
            .await
            .unwrap();
        manager.save_failed(42, 2, "postgres unavailable");

        assert_eq!(
            manager.state(42).unwrap().state,
            PersistenceState::SaveFailed
        );
        assert!(manager.pinned_blocks().contains(&42));
        assert_eq!(manager.recovery_queue().front(), Some(&42));
        assert_eq!(queue.load_ready(document.id, 10).await.unwrap().len(), 1);
    }
}
