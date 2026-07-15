use serde_json::json;
use sqlx::types::Uuid;
use sqlx::{PgPool, Row};

use cditor_core::edit::{EditOperation, EditTransaction, EditTransactionKind, TransactionId};

use super::sync_outbox::{PostgresSyncOutboxStore, SyncClientIdentity};
use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::{
    DbEditTransaction, PgDocumentId, decode_edit_transaction, encode_edit_transaction,
};

const TRANSACTION_ID_NAMESPACE: u128 = 0x3000_0000_0000_0000_0000_0000_0000_0000;
const SNAPSHOT_ID_NAMESPACE: u128 = 0x4000_0000_0000_0000_0000_0000_0000_0000;
const DEFAULT_LARGE_SNAPSHOT_BLOCK_THRESHOLD: usize = 1_000;

#[derive(Debug, Clone)]
pub struct PostgresTransactionStore {
    pool: PgPool,
    large_snapshot_block_threshold: usize,
    sync_identity: Option<SyncClientIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditTransactionVersions {
    pub structure_version_before: Option<i64>,
    pub structure_version_after: Option<i64>,
    pub content_version_after: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredEditTransaction {
    pub transaction: EditTransaction,
    pub versions: EditTransactionVersions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoSnapshotRow {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub payload_kind: String,
    pub block_count: usize,
    pub byte_len: i64,
    pub external_path: Option<String>,
    pub checksum: Option<String>,
}

impl PostgresTransactionStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            large_snapshot_block_threshold: DEFAULT_LARGE_SNAPSHOT_BLOCK_THRESHOLD,
            sync_identity: None,
        }
    }

    pub fn with_large_snapshot_block_threshold(mut self, threshold: usize) -> Self {
        self.large_snapshot_block_threshold = threshold;
        self
    }

    pub fn with_sync_identity(mut self, identity: SyncClientIdentity) -> Self {
        self.sync_identity = Some(identity);
        self
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn save_edit_transaction(
        &self,
        document_id: PgDocumentId,
        transaction: &EditTransaction,
        versions: EditTransactionVersions,
    ) -> PostgresStorageResult<()> {
        let mut tx = self.pool.begin().await?;
        self.save_edit_transaction_tx(&mut tx, document_id, transaction, &versions)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn save_edit_transaction_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        document_id: PgDocumentId,
        transaction: &EditTransaction,
        versions: &EditTransactionVersions,
    ) -> PostgresStorageResult<()> {
        self.insert_edit_transaction(tx, document_id, transaction, versions)
            .await?;
        self.maybe_insert_large_undo_snapshot(tx, document_id, transaction)
            .await?;
        self.enqueue_fts_update_task(tx, document_id, transaction)
            .await?;
        if let Some(identity) = self.sync_identity {
            PostgresSyncOutboxStore::insert_local_transaction_outbox_tx(
                tx,
                document_id,
                transaction,
                versions,
                identity,
            )
            .await?;
        }
        Ok(())
    }

    pub async fn load_recent_transactions(
        &self,
        document_id: PgDocumentId,
        limit: i64,
    ) -> PostgresStorageResult<Vec<StoredEditTransaction>> {
        let rows = sqlx::query(
            r#"
            SELECT
                ops_json,
                structure_version_before,
                structure_version_after,
                content_version_after
            FROM edit_transactions
            WHERE document_id = $1
            ORDER BY created_at DESC, sequence DESC NULLS LAST
            LIMIT $2
            "#,
        )
        .bind(document_id)
        .bind(limit.max(0))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let tx_json: serde_json::Value = row.try_get("ops_json")?;
                Ok(StoredEditTransaction {
                    transaction: decode_edit_transaction(tx_json)?,
                    versions: EditTransactionVersions {
                        structure_version_before: row.try_get("structure_version_before")?,
                        structure_version_after: row.try_get("structure_version_after")?,
                        content_version_after: row.try_get("content_version_after")?,
                    },
                })
            })
            .collect()
    }

    pub async fn load_undo_snapshots(
        &self,
        document_id: PgDocumentId,
        transaction_id: TransactionId,
    ) -> PostgresStorageResult<Vec<UndoSnapshotRow>> {
        let rows = sqlx::query(
            r#"
            SELECT id, transaction_id, payload_kind, block_count, byte_len, external_path, checksum
            FROM undo_snapshots
            WHERE document_id = $1 AND transaction_id = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(document_id)
        .bind(pg_transaction_id_from_runtime(transaction_id))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let block_count: i32 = row.try_get("block_count")?;
                Ok(UndoSnapshotRow {
                    id: row.try_get("id")?,
                    transaction_id: row.try_get("transaction_id")?,
                    payload_kind: row.try_get("payload_kind")?,
                    block_count: usize::try_from(block_count).map_err(|_| {
                        PostgresStorageError::CorruptData {
                            message: format!(
                                "undo snapshot has negative block_count {block_count}"
                            ),
                        }
                    })?,
                    byte_len: row.try_get("byte_len")?,
                    external_path: row.try_get("external_path")?,
                    checksum: row.try_get("checksum")?,
                })
            })
            .collect()
    }

    async fn insert_edit_transaction(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        document_id: PgDocumentId,
        transaction: &EditTransaction,
        versions: &EditTransactionVersions,
    ) -> PostgresStorageResult<()> {
        let db_tx = DbEditTransaction::from(transaction);
        let tx_json = encode_edit_transaction(transaction)?;
        let inverse_ops_json = serde_json::to_value(&db_tx.inverse_ops)?;
        let affected_blocks_json = serde_json::to_value(&db_tx.affected_blocks)?;
        let before_selection_json = db_tx
            .before_selection
            .map(serde_json::to_value)
            .transpose()?;
        let after_selection_json = db_tx
            .after_selection
            .map(serde_json::to_value)
            .transpose()?;
        let before_anchor_json = db_tx.before_anchor.map(serde_json::to_value).transpose()?;
        let after_anchor_json = db_tx.after_anchor.map(serde_json::to_value).transpose()?;

        sqlx::query(
            r#"
            INSERT INTO edit_transactions (
                id,
                document_id,
                transaction_kind,
                ops_json,
                inverse_ops_json,
                affected_blocks_json,
                before_selection_json,
                after_selection_json,
                before_anchor_json,
                after_anchor_json,
                structure_version_before,
                structure_version_after,
                content_version_after,
                sequence,
                created_at,
                persisted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, to_timestamp($15::double precision / 1000.0), now())
            ON CONFLICT (id) DO UPDATE SET
                document_id = EXCLUDED.document_id,
                transaction_kind = EXCLUDED.transaction_kind,
                ops_json = EXCLUDED.ops_json,
                inverse_ops_json = EXCLUDED.inverse_ops_json,
                affected_blocks_json = EXCLUDED.affected_blocks_json,
                before_selection_json = EXCLUDED.before_selection_json,
                after_selection_json = EXCLUDED.after_selection_json,
                before_anchor_json = EXCLUDED.before_anchor_json,
                after_anchor_json = EXCLUDED.after_anchor_json,
                structure_version_before = EXCLUDED.structure_version_before,
                structure_version_after = EXCLUDED.structure_version_after,
                content_version_after = EXCLUDED.content_version_after,
                sequence = EXCLUDED.sequence,
                persisted_at = now()
            "#,
        )
        .bind(pg_transaction_id_from_runtime(transaction.id))
        .bind(document_id)
        .bind(transaction_kind_to_db(transaction.kind))
        .bind(tx_json)
        .bind(inverse_ops_json)
        .bind(affected_blocks_json)
        .bind(before_selection_json)
        .bind(after_selection_json)
        .bind(before_anchor_json)
        .bind(after_anchor_json)
        .bind(versions.structure_version_before)
        .bind(versions.structure_version_after)
        .bind(versions.content_version_after)
        .bind(i64::try_from(transaction.id).map_err(|_| PostgresStorageError::CorruptData {
            message: format!("transaction id {} exceeds BIGINT range", transaction.id),
        })?)
        .bind(i64::try_from(transaction.timestamp).map_err(|_| {
            PostgresStorageError::CorruptData {
                message: format!("transaction timestamp {} exceeds BIGINT range", transaction.timestamp),
            }
        })?)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn enqueue_fts_update_task(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        document_id: PgDocumentId,
        transaction: &EditTransaction,
    ) -> PostgresStorageResult<()> {
        if transaction.affected_blocks.is_empty() {
            return Ok(());
        }
        let task_json = serde_json::json!({
            "transaction_id": transaction.id,
            "affected_blocks": transaction.affected_blocks,
            "content_version_after": transaction.timestamp,
        });
        let affected_blocks_json = serde_json::to_value(&transaction.affected_blocks)?;
        sqlx::query(
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
            VALUES ($1, 'update_fts', $2, $3, 'pending', 0, now())
            "#,
        )
        .bind(document_id)
        .bind(task_json)
        .bind(affected_blocks_json)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn maybe_insert_large_undo_snapshot(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        document_id: PgDocumentId,
        transaction: &EditTransaction,
    ) -> PostgresStorageResult<()> {
        let block_count = large_snapshot_block_count(transaction);
        if block_count <= self.large_snapshot_block_threshold {
            return Ok(());
        }

        let snapshot_json = json!({
            "type": "block_range_snapshot",
            "transaction_id": transaction.id,
            "block_count": block_count,
            "strategy": "metadata_only_v1"
        });
        let byte_len = i64::try_from(snapshot_json.to_string().len()).map_err(|_| {
            PostgresStorageError::CorruptData {
                message: "undo snapshot json length exceeds BIGINT".to_owned(),
            }
        })?;

        sqlx::query(
            r#"
            INSERT INTO undo_snapshots (
                id,
                document_id,
                transaction_id,
                payload_kind,
                block_count,
                byte_len,
                snapshot_json,
                checksum,
                created_at
            )
            VALUES ($1, $2, $3, 'block_range_snapshot', $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                document_id = EXCLUDED.document_id,
                transaction_id = EXCLUDED.transaction_id,
                payload_kind = EXCLUDED.payload_kind,
                block_count = EXCLUDED.block_count,
                byte_len = EXCLUDED.byte_len,
                snapshot_json = EXCLUDED.snapshot_json,
                checksum = EXCLUDED.checksum
            "#,
        )
        .bind(pg_snapshot_id_from_runtime(transaction.id))
        .bind(document_id)
        .bind(pg_transaction_id_from_runtime(transaction.id))
        .bind(
            i32::try_from(block_count).map_err(|_| PostgresStorageError::CorruptData {
                message: format!("undo snapshot block_count {block_count} exceeds INTEGER range"),
            })?,
        )
        .bind(byte_len)
        .bind(snapshot_json)
        .bind(format!("tx:{}:blocks:{block_count}", transaction.id))
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    #[cfg(test)]
    async fn save_edit_transaction_then_fail_for_rollback_test(
        &self,
        document_id: PgDocumentId,
        transaction: &EditTransaction,
        versions: EditTransactionVersions,
    ) -> PostgresStorageResult<()> {
        let mut tx = self.pool.begin().await?;
        self.insert_edit_transaction(&mut tx, document_id, transaction, &versions)
            .await?;
        sqlx::query(
            r#"
            INSERT INTO undo_snapshots (document_id, transaction_id, payload_kind, block_count, byte_len)
            VALUES ($1, $2, 'rollback_probe', 0, 0)
            "#,
        )
        .bind(document_id)
        .bind(Uuid::from_u128(0x9999_0000_0000_0000_0000_0000_0000_0000))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

pub fn pg_transaction_id_from_runtime(id: TransactionId) -> Uuid {
    Uuid::from_u128(TRANSACTION_ID_NAMESPACE | id as u128)
}

fn pg_snapshot_id_from_runtime(id: TransactionId) -> Uuid {
    Uuid::from_u128(SNAPSHOT_ID_NAMESPACE | id as u128)
}

fn transaction_kind_to_db(kind: EditTransactionKind) -> &'static str {
    match kind {
        EditTransactionKind::Typing => "typing",
        EditTransactionKind::CompositionCommit => "composition_commit",
        EditTransactionKind::Paste => "paste",
        EditTransactionKind::AiApply => "ai_apply",
        EditTransactionKind::DragDrop => "drag_drop",
        EditTransactionKind::Format => "format",
        EditTransactionKind::ExplicitCommand => "explicit_command",
        EditTransactionKind::BlockStructureChange => "block_structure_change",
    }
}

fn large_snapshot_block_count(transaction: &EditTransaction) -> usize {
    transaction
        .ops
        .iter()
        .chain(transaction.inverse_ops.iter())
        .map(|op| match op {
            EditOperation::InsertBlocks { blocks, .. } => blocks.len(),
            EditOperation::DeleteBlockRange { range } => range.len(),
            EditOperation::MoveBlockRange { range, .. } => range.len(),
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use sqlx::types::Uuid;

    use super::*;
    use crate::{
        DocumentRow, PostgresDocumentStore, PostgresPoolConfig, SyncClientIdentity, create_pg_pool,
        pg_document_id_from_runtime, run_migrations,
    };
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::edit::{DocumentSelection, ScrollAnchor, TextPosition};
    use cditor_core::rich_text::{RichBlockKind, kind_tag_for_rich_block_kind};

    fn test_database_url() -> String {
        std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
    }

    async fn test_stores() -> (PostgresDocumentStore, PostgresTransactionStore) {
        let config = PostgresPoolConfig::for_tests(test_database_url());
        let pool = create_pg_pool(&config).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (
            PostgresDocumentStore::new(pool.clone()),
            PostgresTransactionStore::new(pool),
        )
    }

    fn document_row(document_id: u64) -> DocumentRow {
        DocumentRow {
            id: pg_document_id_from_runtime(document_id),
            workspace_id: Uuid::from_u128(
                0x9300_0000_0000_0000_0000_0000_0000_0000 | document_id as u128,
            ),
            title: format!("Transaction Store {document_id}"),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        }
    }

    async fn seed_document(
        document_id: u64,
    ) -> (PostgresDocumentStore, PostgresTransactionStore, DocumentRow) {
        let (document_store, transaction_store) = test_stores().await;
        let document = document_row(document_id);
        document_store
            .save_document_metadata(&document)
            .await
            .unwrap();
        (document_store, transaction_store, document)
    }

    fn versions() -> EditTransactionVersions {
        EditTransactionVersions {
            structure_version_before: Some(1),
            structure_version_after: Some(2),
            content_version_after: Some(3),
        }
    }

    fn typing_tx(id: u64, block_id: u64, text: &str) -> EditTransaction {
        EditTransaction::insert_text(id, 1_700_000_000_000 + id, block_id, 0, text)
            .with_selection(
                Some(DocumentSelection::caret(TextPosition::downstream(
                    block_id, 0,
                ))),
                Some(DocumentSelection::caret(TextPosition::downstream(
                    block_id,
                    text.len(),
                ))),
            )
            .with_anchor(
                Some(ScrollAnchor {
                    block_id,
                    offset_in_block: 0.0,
                    viewport_y: 12.0,
                }),
                Some(ScrollAnchor {
                    block_id,
                    offset_in_block: 24.0,
                    viewport_y: 12.0,
                }),
            )
    }

    fn large_paste_tx(id: u64, start_block_id: u64, block_count: usize) -> EditTransaction {
        let paragraph = kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph);
        let blocks = (0..block_count)
            .map(|index| {
                BlockIndexRecord::new(start_block_id + index as u64, None, 0, paragraph, 0)
            })
            .collect::<Vec<_>>();
        EditTransaction::paste_blocks(id, 1_700_000_000_000 + id, 0, blocks)
    }

    #[test]
    fn large_snapshot_block_count_detects_bulk_operations() {
        assert_eq!(large_snapshot_block_count(&typing_tx(1, 1, "a")), 0);
        assert_eq!(
            large_snapshot_block_count(&large_paste_tx(2, 10, 1_500)),
            1_500
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_transaction_store_saves_and_loads_recent_transactions() {
        let (_document_store, transaction_store, document) = seed_document(60_001).await;
        let first = typing_tx(800_001, 1, "hello");
        let second = typing_tx(800_002, 1, "world");

        transaction_store
            .save_edit_transaction(document.id, &first, versions())
            .await
            .unwrap();
        transaction_store
            .save_edit_transaction(document.id, &second, versions())
            .await
            .unwrap();

        let loaded = transaction_store
            .load_recent_transactions(document.id, 2)
            .await
            .unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].transaction.id, second.id);
        assert_eq!(loaded[1].transaction, first);
        assert_eq!(loaded[0].versions.content_version_after, Some(3));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_transaction_store_saves_large_paste_undo_snapshot() {
        let (_document_store, transaction_store, document) = seed_document(60_002).await;
        let transaction_store = transaction_store.with_large_snapshot_block_threshold(10);
        let tx = large_paste_tx(800_003, 810_000, 32);

        transaction_store
            .save_edit_transaction(document.id, &tx, versions())
            .await
            .unwrap();

        let snapshots = transaction_store
            .load_undo_snapshots(document.id, tx.id)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].payload_kind, "block_range_snapshot");
        assert_eq!(snapshots[0].block_count, 32);
        assert!(snapshots[0].byte_len > 0);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_transaction_store_does_not_snapshot_small_typing_transaction() {
        let (_document_store, transaction_store, document) = seed_document(60_003).await;
        let tx = typing_tx(800_004, 1, "small");

        transaction_store
            .save_edit_transaction(document.id, &tx, versions())
            .await
            .unwrap();

        let snapshots = transaction_store
            .load_undo_snapshots(document.id, tx.id)
            .await
            .unwrap();
        assert!(snapshots.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_transaction_store_enqueues_fts_update_task_after_save() {
        let (_document_store, transaction_store, document) = seed_document(60_006).await;
        let tx = typing_tx(800_006, 42, "fts");

        transaction_store
            .save_edit_transaction(document.id, &tx, versions())
            .await
            .unwrap();

        let task_kind: String = sqlx::query_scalar(
            "SELECT task_kind FROM persistence_queue WHERE document_id = $1 AND task_kind = 'update_fts' ORDER BY created_at DESC LIMIT 1",
        )
        .bind(document.id)
        .fetch_one(transaction_store.pool())
        .await
        .unwrap();
        assert_eq!(task_kind, "update_fts");
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_transaction_store_generates_sync_outbox_when_enabled() {
        let (_document_store, transaction_store, document) = seed_document(60_007).await;
        let transaction_store = transaction_store.with_sync_identity(SyncClientIdentity {
            client_id: Uuid::from_u128(0xa300_0000_0000_0000_0000_0000_0000_0001),
            device_id: Uuid::from_u128(0xa400_0000_0000_0000_0000_0000_0000_0001),
        });
        let tx = typing_tx(800_007, 42, "sync");

        transaction_store
            .save_edit_transaction(document.id, &tx, versions())
            .await
            .unwrap();

        let row: (i64, String) = sqlx::query_as(
            "SELECT sequence, state FROM sync_outbox WHERE document_id = $1 AND local_transaction_id = $2",
        )
        .bind(document.id)
        .bind(pg_transaction_id_from_runtime(tx.id))
        .fetch_one(transaction_store.pool())
        .await
        .unwrap();
        assert_eq!(row, (1, "pending".to_owned()));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_transaction_store_rolls_back_when_second_write_fails() {
        let (_document_store, transaction_store, document) = seed_document(60_004).await;
        let tx = typing_tx(800_005, 1, "rollback");

        let error = transaction_store
            .save_edit_transaction_then_fail_for_rollback_test(document.id, &tx, versions())
            .await
            .unwrap_err();
        assert!(matches!(error, PostgresStorageError::Sqlx(_)));

        let rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM edit_transactions WHERE id = $1 AND document_id = $2",
        )
        .bind(pg_transaction_id_from_runtime(tx.id))
        .bind(document.id)
        .fetch_one(transaction_store.pool())
        .await
        .unwrap();
        assert_eq!(rows, 0);
    }
}
