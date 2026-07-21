//! operation journal / outbox / checkpoint / crash marker（P7-003/004/009 基础）。
//!
//! - `append_transaction`：journal 条目 + outbox 条目在**同一个 SQLite 事务**
//!   内落盘；调用方把 materialized 行写入同一事务时即满足 P7-004 原子性。
//! - `journal_entries_after_checkpoint`：崩溃恢复的 replay 输入，配合
//!   Runtime `apply_external_transaction` 与 core `decode_transaction` 使用。
//! - `record_checkpoint`：materialized 状态吸收到某 journal id 并记录语义
//!   checksum；其后的 compact 可安全删除已确认且已 checkpoint 的旧条目。
//! - crash marker：启动置 dirty，干净退出置 clean；下次启动读取判定是否
//!   需要 replay。

use cditor_core::edit::ChangeOrigin;
use cditor_core::schema::SchemaVersion;
use cditor_storage::backend::StorageBackendKind;
use cditor_storage::{StorageError, StorageResult};
use sqlx::Row;

use crate::ids::document_id_to_sqlite;
use crate::storage::SqliteDocumentStorage;
use crate::util::unix_millis;
use cditor_core::ids::DocumentId;

/// journal 读回条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub journal_id: i64,
    pub document_id: DocumentId,
    pub transaction_id: u64,
    pub schema_version: SchemaVersion,
    pub envelope_json: String,
    pub origin: String,
    pub created_at: i64,
}

/// outbox 条目状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxState {
    Pending,
    Inflight,
    Acked,
    Rejected,
}

impl OutboxState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Inflight => "inflight",
            Self::Acked => "acked",
            Self::Rejected => "rejected",
        }
    }

    fn parse(text: &str) -> StorageResult<Self> {
        match text {
            "pending" => Ok(Self::Pending),
            "inflight" => Ok(Self::Inflight),
            "acked" => Ok(Self::Acked),
            "rejected" => Ok(Self::Rejected),
            other => Err(StorageError::CorruptData(format!(
                "unknown outbox state {other:?}"
            ))),
        }
    }
}

/// outbox 读回条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    pub outbox_id: i64,
    pub journal_id: i64,
    pub state: OutboxState,
    pub attempt_count: i64,
    pub last_error: Option<String>,
}

/// 启动时的恢复判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupRecovery {
    /// 首次启动或上次干净退出。
    CleanStart,
    /// 上次未干净退出：必须先 replay checkpoint 之后的 journal。
    CrashDetected { previous_started_at: i64 },
}

impl SqliteDocumentStorage {
    /// 原子追加：journal + （可选）outbox 同一事务。返回 journal id。
    ///
    /// `enqueue_outbox` 为 false 时用于不需要上行同步的来源（remote 回放、
    /// 迁移）。
    pub async fn append_transaction_to_journal(
        &self,
        document_id: DocumentId,
        transaction_id: u64,
        schema_version: SchemaVersion,
        envelope_json: &str,
        origin: ChangeOrigin,
        enqueue_outbox: bool,
    ) -> StorageResult<i64> {
        let _writer = self.writer_gate().acquire().await?;
        let now = unix_millis()?;
        let document_uuid = document_id_to_sqlite(document_id);
        let mut transaction = self.pool.begin().await.map_err(map_sqlx)?;

        let journal_id = sqlx::query(
            "INSERT INTO operation_journal \
             (document_id, transaction_id, schema_major, schema_minor, envelope_json, origin, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(document_uuid)
        .bind(transaction_id as i64)
        .bind(i64::from(schema_version.major))
        .bind(i64::from(schema_version.minor))
        .bind(envelope_json)
        .bind(origin.as_str())
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(map_sqlx)?
        .last_insert_rowid();

        if enqueue_outbox {
            sqlx::query(
                "INSERT INTO sync_outbox (journal_id, document_id, updated_at) VALUES (?, ?, ?)",
            )
            .bind(journal_id)
            .bind(document_uuid)
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(map_sqlx)?;
        }

        transaction.commit().await.map_err(map_sqlx)?;
        Ok(journal_id)
    }

    /// checkpoint 之后（不含）的 journal 条目，按写入顺序返回——replay 输入。
    pub async fn journal_entries_after_checkpoint(
        &self,
        document_id: DocumentId,
    ) -> StorageResult<Vec<JournalEntry>> {
        let document_uuid = document_id_to_sqlite(document_id);
        let checkpoint_id: i64 =
            sqlx::query("SELECT journal_id FROM journal_checkpoints WHERE document_id = ?")
                .bind(document_uuid)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx)?
                .map(|row| row.get::<i64, _>(0))
                .unwrap_or(0);

        let rows = sqlx::query(
            "SELECT id, transaction_id, schema_major, schema_minor, envelope_json, origin, created_at \
             FROM operation_journal WHERE document_id = ? AND id > ? ORDER BY id ASC",
        )
        .bind(document_uuid)
        .bind(checkpoint_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rows
            .into_iter()
            .map(|row| JournalEntry {
                journal_id: row.get(0),
                document_id,
                transaction_id: row.get::<i64, _>(1) as u64,
                schema_version: SchemaVersion::new(
                    row.get::<i64, _>(2) as u32,
                    row.get::<i64, _>(3) as u32,
                ),
                envelope_json: row.get(4),
                origin: row.get(5),
                created_at: row.get(6),
            })
            .collect())
    }

    /// 记录 checkpoint：materialized 状态已吸收到（含）`journal_id`。
    pub async fn record_journal_checkpoint(
        &self,
        document_id: DocumentId,
        journal_id: i64,
        materialized_checksum: u64,
    ) -> StorageResult<()> {
        let _writer = self.writer_gate().acquire().await?;
        sqlx::query(
            "INSERT INTO journal_checkpoints (document_id, journal_id, materialized_checksum, created_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(document_id) DO UPDATE SET \
             journal_id = excluded.journal_id, \
             materialized_checksum = excluded.materialized_checksum, \
             created_at = excluded.created_at",
        )
        .bind(document_id_to_sqlite(document_id))
        .bind(journal_id)
        .bind(materialized_checksum as i64)
        .bind(unix_millis()?)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    /// checkpoint 记录的 checksum（replay 后核对用）。
    pub async fn journal_checkpoint_checksum(
        &self,
        document_id: DocumentId,
    ) -> StorageResult<Option<(i64, u64)>> {
        Ok(sqlx::query(
            "SELECT journal_id, materialized_checksum FROM journal_checkpoints WHERE document_id = ?",
        )
        .bind(document_id_to_sqlite(document_id))
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .map(|row| (row.get::<i64, _>(0), row.get::<i64, _>(1) as u64)))
    }

    /// compact：删除既已 checkpoint 又已 acked（或无 outbox）的旧条目。
    pub async fn compact_journal(&self, document_id: DocumentId) -> StorageResult<u64> {
        let _writer = self.writer_gate().acquire().await?;
        let document_uuid = document_id_to_sqlite(document_id);
        let result = sqlx::query(
            "DELETE FROM operation_journal WHERE document_id = ? \
             AND id <= COALESCE((SELECT journal_id FROM journal_checkpoints WHERE document_id = ?), 0) \
             AND NOT EXISTS (SELECT 1 FROM sync_outbox WHERE sync_outbox.journal_id = operation_journal.id \
                             AND sync_outbox.state IN ('pending', 'inflight', 'rejected'))",
        )
        .bind(document_uuid)
        .bind(document_uuid)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(result.rows_affected())
    }

    /// 待上行的 outbox 条目（pending/rejected），按写入顺序。
    pub async fn outbox_entries(&self, document_id: DocumentId) -> StorageResult<Vec<OutboxEntry>> {
        let rows = sqlx::query(
            "SELECT id, journal_id, state, attempt_count, last_error FROM sync_outbox \
             WHERE document_id = ? ORDER BY id ASC",
        )
        .bind(document_id_to_sqlite(document_id))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        rows.into_iter()
            .map(|row| {
                Ok(OutboxEntry {
                    outbox_id: row.get(0),
                    journal_id: row.get(1),
                    state: OutboxState::parse(row.get::<String, _>(2).as_str())?,
                    attempt_count: row.get(3),
                    last_error: row.get(4),
                })
            })
            .collect()
    }

    /// outbox 状态迁移；inflight 会自增 attempt_count。
    pub async fn set_outbox_state(
        &self,
        outbox_id: i64,
        state: OutboxState,
        error: Option<&str>,
    ) -> StorageResult<()> {
        let _writer = self.writer_gate().acquire().await?;
        let attempted = i64::from(matches!(state, OutboxState::Inflight));
        sqlx::query(
            "UPDATE sync_outbox SET state = ?, attempt_count = attempt_count + ?, \
             last_error = ?, updated_at = ? WHERE id = ?",
        )
        .bind(state.as_str())
        .bind(attempted)
        .bind(error)
        .bind(unix_millis()?)
        .bind(outbox_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    /// 启动：读取上次会话的 crash marker 并将本次标记为 dirty。
    pub async fn begin_session_with_crash_marker(&self) -> StorageResult<StartupRecovery> {
        let _writer = self.writer_gate().acquire().await?;
        let previous =
            sqlx::query("SELECT started_at, clean_shutdown FROM crash_marker WHERE id = 1")
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx)?
                .map(|row| (row.get::<i64, _>(0), row.get::<i64, _>(1) != 0));

        sqlx::query(
            "INSERT INTO crash_marker (id, started_at, clean_shutdown) VALUES (1, ?, 0) \
             ON CONFLICT(id) DO UPDATE SET started_at = excluded.started_at, clean_shutdown = 0",
        )
        .bind(unix_millis()?)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(match previous {
            Some((started_at, false)) => StartupRecovery::CrashDetected {
                previous_started_at: started_at,
            },
            _ => StartupRecovery::CleanStart,
        })
    }

    /// 干净退出：置 clean 标记。
    pub async fn mark_clean_shutdown(&self) -> StorageResult<()> {
        let _writer = self.writer_gate().acquire().await?;
        sqlx::query("UPDATE crash_marker SET clean_shutdown = 1 WHERE id = 1")
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }
}

fn map_sqlx(error: sqlx::Error) -> StorageError {
    StorageError::Backend {
        backend: StorageBackendKind::Sqlite,
        message: error.to_string(),
    }
}
