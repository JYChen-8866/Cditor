use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;
use sqlx::{PgPool, Row};

use cditor_core::edit::EditTransaction;
use cditor_core::ids::BlockId;

use super::transaction::{EditTransactionVersions, pg_transaction_id_from_runtime};
use crate::error::PostgresStorageResult;
use crate::types::{PgDocumentId, encode_edit_transaction, pg_block_id_from_runtime};

#[derive(Debug, Clone)]
pub struct PostgresSyncOutboxStore {
    pool: PgPool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncClientIdentity {
    pub client_id: Uuid,
    pub device_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutboxState {
    Pending,
    Uploading,
    Acked,
    Failed,
}

impl SyncOutboxState {
    fn from_db(value: &str) -> Self {
        match value {
            "uploading" => Self::Uploading,
            "acked" => Self::Acked,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncOutboxRecord {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub document_id: PgDocumentId,
    pub local_transaction_id: Uuid,
    pub operation_kind: String,
    pub payload_json: serde_json::Value,
    pub affected_blocks: Vec<BlockId>,
    pub base_structure_version: Option<i64>,
    pub base_content_version: Option<i64>,
    pub client_id: Uuid,
    pub device_id: Uuid,
    pub sequence: i64,
    pub state: SyncOutboxState,
    pub attempt_count: i32,
    pub last_error: Option<String>,
    pub server_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStateRecord {
    pub document_id: PgDocumentId,
    pub client_id: Uuid,
    pub device_id: Uuid,
    pub last_local_sequence: i64,
    pub last_uploaded_sequence: i64,
    pub last_server_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTombstoneRecord {
    pub entity_id: Uuid,
    pub entity_kind: String,
    pub document_id: Option<PgDocumentId>,
    pub deleted_by_client_id: Option<Uuid>,
    pub deleted_by_device_id: Option<Uuid>,
    pub server_revision: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOutboxInsertResult {
    pub outbox_id: Uuid,
    pub sequence: i64,
}

impl PostgresSyncOutboxStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn insert_local_transaction_outbox(
        &self,
        document_id: PgDocumentId,
        transaction: &EditTransaction,
        versions: &EditTransactionVersions,
        identity: SyncClientIdentity,
    ) -> PostgresStorageResult<SyncOutboxInsertResult> {
        let mut tx = self.pool.begin().await?;
        let result = Self::insert_local_transaction_outbox_tx(
            &mut tx,
            document_id,
            transaction,
            versions,
            identity,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }

    pub(crate) async fn insert_local_transaction_outbox_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        document_id: PgDocumentId,
        transaction: &EditTransaction,
        versions: &EditTransactionVersions,
        identity: SyncClientIdentity,
    ) -> PostgresStorageResult<SyncOutboxInsertResult> {
        let sequence = Self::next_local_sequence_tx(tx, document_id, identity).await?;
        let payload_json = encode_edit_transaction(transaction)?;
        let affected_blocks_json = serde_json::to_value(&transaction.affected_blocks)?;
        let transaction_id = pg_transaction_id_from_runtime(transaction.id);
        let operation_kind = transaction_kind_to_sync_operation(transaction.kind);

        let row = sqlx::query(
            r#"
            INSERT INTO sync_outbox (
                workspace_id,
                document_id,
                local_transaction_id,
                operation_kind,
                payload_json,
                affected_blocks_json,
                base_structure_version,
                base_content_version,
                client_id,
                device_id,
                sequence,
                state,
                attempt_count,
                updated_at
            )
            SELECT
                d.workspace_id,
                $1,
                $2,
                $3,
                $4,
                $5,
                $6,
                $7,
                $8,
                $9,
                $10,
                'pending',
                0,
                now()
            FROM documents d
            WHERE d.id = $1
            RETURNING id, sequence
            "#,
        )
        .bind(document_id)
        .bind(transaction_id)
        .bind(operation_kind)
        .bind(payload_json)
        .bind(affected_blocks_json)
        .bind(versions.structure_version_before)
        .bind(None::<i64>)
        .bind(identity.client_id)
        .bind(identity.device_id)
        .bind(sequence)
        .fetch_one(&mut **tx)
        .await?;

        Ok(SyncOutboxInsertResult {
            outbox_id: row.try_get("id")?,
            sequence: row.try_get("sequence")?,
        })
    }

    async fn next_local_sequence_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        document_id: PgDocumentId,
        identity: SyncClientIdentity,
    ) -> PostgresStorageResult<i64> {
        let row = sqlx::query(
            r#"
            INSERT INTO sync_state (
                document_id,
                client_id,
                device_id,
                last_local_sequence,
                last_uploaded_sequence,
                updated_at
            )
            VALUES ($1, $2, $3, 1, 0, now())
            ON CONFLICT (document_id) DO UPDATE SET
                client_id = EXCLUDED.client_id,
                device_id = EXCLUDED.device_id,
                last_local_sequence = sync_state.last_local_sequence + 1,
                updated_at = now()
            RETURNING last_local_sequence
            "#,
        )
        .bind(document_id)
        .bind(identity.client_id)
        .bind(identity.device_id)
        .fetch_one(&mut **tx)
        .await?;

        Ok(row.try_get("last_local_sequence")?)
    }

    pub async fn load_sync_state(
        &self,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<Option<SyncStateRecord>> {
        let row = sqlx::query(
            r#"
            SELECT document_id, client_id, device_id, last_local_sequence, last_uploaded_sequence, last_server_revision
            FROM sync_state
            WHERE document_id = $1
            "#,
        )
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(sync_state_from_row).transpose()
    }

    pub async fn load_pending_outbox(
        &self,
        document_id: PgDocumentId,
        limit: i64,
    ) -> PostgresStorageResult<Vec<SyncOutboxRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                id,
                workspace_id,
                document_id,
                local_transaction_id,
                operation_kind,
                payload_json,
                affected_blocks_json,
                base_structure_version,
                base_content_version,
                client_id,
                device_id,
                sequence,
                state,
                attempt_count,
                last_error,
                server_revision
            FROM sync_outbox
            WHERE document_id = $1 AND state IN ('pending', 'failed')
            ORDER BY sequence, created_at, id
            LIMIT $2
            "#,
        )
        .bind(document_id)
        .bind(limit.max(0))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(outbox_from_row).collect()
    }

    pub async fn mark_uploading(&self, outbox_id: Uuid) -> PostgresStorageResult<()> {
        sqlx::query(
            r#"
            UPDATE sync_outbox
            SET state = 'uploading', updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(outbox_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn reset_uploading_to_pending(
        &self,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE sync_outbox
            SET state = 'pending', updated_at = now()
            WHERE document_id = $1 AND state = 'uploading'
            "#,
        )
        .bind(document_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn mark_uploaded(
        &self,
        document_id: PgDocumentId,
        sequence: i64,
        server_revision: &str,
    ) -> PostgresStorageResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            UPDATE sync_outbox
            SET state = 'acked', server_revision = $3, server_ack_at = now(), updated_at = now()
            WHERE document_id = $1 AND sequence = $2
            "#,
        )
        .bind(document_id)
        .bind(sequence)
        .bind(server_revision)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE sync_state
            SET
                last_uploaded_sequence = GREATEST(last_uploaded_sequence, $2),
                last_server_revision = $3,
                updated_at = now()
            WHERE document_id = $1
            "#,
        )
        .bind(document_id)
        .bind(sequence)
        .bind(server_revision)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn write_remote_tombstone(
        &self,
        tombstone: &RemoteTombstoneRecord,
    ) -> PostgresStorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO remote_tombstones (
                entity_id,
                entity_kind,
                document_id,
                deleted_by_client_id,
                deleted_by_device_id,
                deleted_at,
                server_revision
            )
            VALUES ($1, $2, $3, $4, $5, now(), $6)
            ON CONFLICT (entity_id) DO UPDATE SET
                entity_kind = EXCLUDED.entity_kind,
                document_id = EXCLUDED.document_id,
                deleted_by_client_id = EXCLUDED.deleted_by_client_id,
                deleted_by_device_id = EXCLUDED.deleted_by_device_id,
                deleted_at = now(),
                server_revision = EXCLUDED.server_revision
            "#,
        )
        .bind(tombstone.entity_id)
        .bind(&tombstone.entity_kind)
        .bind(tombstone.document_id)
        .bind(tombstone.deleted_by_client_id)
        .bind(tombstone.deleted_by_device_id)
        .bind(&tombstone.server_revision)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_remote_tombstone(
        &self,
        entity_id: Uuid,
    ) -> PostgresStorageResult<Option<RemoteTombstoneRecord>> {
        let row = sqlx::query(
            r#"
            SELECT entity_id, entity_kind, document_id, deleted_by_client_id, deleted_by_device_id, server_revision
            FROM remote_tombstones
            WHERE entity_id = $1
            "#,
        )
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(tombstone_from_row).transpose()
    }
}

fn outbox_from_row(row: sqlx::postgres::PgRow) -> PostgresStorageResult<SyncOutboxRecord> {
    let affected_blocks_json: serde_json::Value = row.try_get("affected_blocks_json")?;
    let affected_blocks = serde_json::from_value::<Vec<BlockId>>(affected_blocks_json)?;
    let state: String = row.try_get("state")?;

    Ok(SyncOutboxRecord {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        document_id: row.try_get("document_id")?,
        local_transaction_id: row.try_get("local_transaction_id")?,
        operation_kind: row.try_get("operation_kind")?,
        payload_json: row.try_get("payload_json")?,
        affected_blocks,
        base_structure_version: row.try_get("base_structure_version")?,
        base_content_version: row.try_get("base_content_version")?,
        client_id: row.try_get("client_id")?,
        device_id: row.try_get("device_id")?,
        sequence: row.try_get("sequence")?,
        state: SyncOutboxState::from_db(&state),
        attempt_count: row.try_get("attempt_count")?,
        last_error: row.try_get("last_error")?,
        server_revision: row.try_get("server_revision")?,
    })
}

fn sync_state_from_row(row: sqlx::postgres::PgRow) -> PostgresStorageResult<SyncStateRecord> {
    Ok(SyncStateRecord {
        document_id: row.try_get("document_id")?,
        client_id: row.try_get("client_id")?,
        device_id: row.try_get("device_id")?,
        last_local_sequence: row.try_get("last_local_sequence")?,
        last_uploaded_sequence: row.try_get("last_uploaded_sequence")?,
        last_server_revision: row.try_get("last_server_revision")?,
    })
}

fn tombstone_from_row(row: sqlx::postgres::PgRow) -> PostgresStorageResult<RemoteTombstoneRecord> {
    Ok(RemoteTombstoneRecord {
        entity_id: row.try_get("entity_id")?,
        entity_kind: row.try_get("entity_kind")?,
        document_id: row.try_get("document_id")?,
        deleted_by_client_id: row.try_get("deleted_by_client_id")?,
        deleted_by_device_id: row.try_get("deleted_by_device_id")?,
        server_revision: row.try_get("server_revision")?,
    })
}

fn transaction_kind_to_sync_operation(
    kind: cditor_core::edit::EditTransactionKind,
) -> &'static str {
    match kind {
        cditor_core::edit::EditTransactionKind::Typing => "typing",
        cditor_core::edit::EditTransactionKind::CompositionCommit => "composition_commit",
        cditor_core::edit::EditTransactionKind::Paste => "paste",
        cditor_core::edit::EditTransactionKind::AiApply => "ai_apply",
        cditor_core::edit::EditTransactionKind::DragDrop => "drag_drop",
        cditor_core::edit::EditTransactionKind::Format => "format",
        cditor_core::edit::EditTransactionKind::ExplicitCommand => "explicit_command",
        cditor_core::edit::EditTransactionKind::BlockStructureChange => "block_structure_change",
    }
}

pub fn pg_tombstone_block_entity_id(block_id: BlockId) -> Uuid {
    pg_block_id_from_runtime(block_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DocumentRow, PostgresDocumentStore, PostgresPoolConfig, PostgresTransactionStore,
        create_pg_pool, pg_document_id_from_runtime, run_migrations,
    };
    use cditor_core::edit::EditTransaction;

    fn test_database_url() -> String {
        std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
    }

    async fn test_stores() -> (
        PostgresDocumentStore,
        PostgresTransactionStore,
        PostgresSyncOutboxStore,
        DocumentRow,
    ) {
        let pool = create_pg_pool(&PostgresPoolConfig::for_tests(test_database_url()))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let runtime_document_id = 120_000 + suffix;
        let document = DocumentRow {
            id: pg_document_id_from_runtime(runtime_document_id),
            workspace_id: Uuid::from_u128(
                0x9900_0000_0000_0000_0000_0000_0000_0000 | runtime_document_id as u128,
            ),
            title: format!("Sync Outbox {runtime_document_id}"),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        };
        let document_store = PostgresDocumentStore::new(pool.clone());
        document_store
            .save_document_metadata(&document)
            .await
            .unwrap();
        (
            document_store,
            PostgresTransactionStore::new(pool.clone()),
            PostgresSyncOutboxStore::new(pool),
            document,
        )
    }

    fn identity(seed: u128) -> SyncClientIdentity {
        SyncClientIdentity {
            client_id: Uuid::from_u128(0xa100_0000_0000_0000_0000_0000_0000_0000 | seed),
            device_id: Uuid::from_u128(0xa200_0000_0000_0000_0000_0000_0000_0000 | seed),
        }
    }

    fn versions() -> EditTransactionVersions {
        EditTransactionVersions {
            structure_version_before: Some(1),
            structure_version_after: Some(2),
            content_version_after: Some(3),
        }
    }

    fn typing_tx(id: u64, block_id: u64) -> EditTransaction {
        EditTransaction::insert_text(id, 1_700_000_000_000 + id, block_id, 0, "sync")
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_sync_outbox_inserts_local_transaction_and_advances_sequence() {
        let (_document_store, transaction_store, outbox, document) = test_stores().await;
        let identity = identity(1);
        let tx = typing_tx(900_001, 12_001);
        transaction_store
            .save_edit_transaction(document.id, &tx, versions())
            .await
            .unwrap();

        let result = outbox
            .insert_local_transaction_outbox(document.id, &tx, &versions(), identity)
            .await
            .unwrap();
        let rows = outbox.load_pending_outbox(document.id, 10).await.unwrap();
        let state = outbox.load_sync_state(document.id).await.unwrap().unwrap();

        assert_eq!(result.sequence, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, result.outbox_id);
        assert_eq!(
            rows[0].local_transaction_id,
            pg_transaction_id_from_runtime(tx.id)
        );
        assert_eq!(rows[0].affected_blocks, vec![12_001]);
        assert_eq!(state.last_local_sequence, 1);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_sync_outbox_marks_uploaded_and_updates_sync_state() {
        let (_document_store, transaction_store, outbox, document) = test_stores().await;
        let identity = identity(2);
        let tx = typing_tx(900_002, 12_002);
        transaction_store
            .save_edit_transaction(document.id, &tx, versions())
            .await
            .unwrap();
        let result = outbox
            .insert_local_transaction_outbox(document.id, &tx, &versions(), identity)
            .await
            .unwrap();

        outbox
            .mark_uploaded(document.id, result.sequence, "server-rev-1")
            .await
            .unwrap();

        let pending = outbox.load_pending_outbox(document.id, 10).await.unwrap();
        let state = outbox.load_sync_state(document.id).await.unwrap().unwrap();
        assert!(pending.is_empty());
        assert_eq!(state.last_uploaded_sequence, 1);
        assert_eq!(state.last_server_revision.as_deref(), Some("server-rev-1"));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_sync_outbox_writes_remote_tombstone() {
        let (_document_store, _transaction_store, outbox, document) = test_stores().await;
        let identity = identity(3);
        let tombstone = RemoteTombstoneRecord {
            entity_id: pg_tombstone_block_entity_id(12_003),
            entity_kind: "block".to_owned(),
            document_id: Some(document.id),
            deleted_by_client_id: Some(identity.client_id),
            deleted_by_device_id: Some(identity.device_id),
            server_revision: Some("server-rev-delete".to_owned()),
        };

        outbox.write_remote_tombstone(&tombstone).await.unwrap();
        let loaded = outbox
            .load_remote_tombstone(tombstone.entity_id)
            .await
            .unwrap();

        assert_eq!(loaded, Some(tombstone));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_sync_outbox_crash_recovery_resets_uploading_to_pending() {
        let (_document_store, transaction_store, outbox, document) = test_stores().await;
        let identity = identity(4);
        let tx = typing_tx(900_004, 12_004);
        transaction_store
            .save_edit_transaction(document.id, &tx, versions())
            .await
            .unwrap();
        let result = outbox
            .insert_local_transaction_outbox(document.id, &tx, &versions(), identity)
            .await
            .unwrap();
        outbox.mark_uploading(result.outbox_id).await.unwrap();

        let reset = outbox
            .reset_uploading_to_pending(document.id)
            .await
            .unwrap();
        let pending = outbox.load_pending_outbox(document.id, 10).await.unwrap();

        assert_eq!(reset, 1);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, result.outbox_id);
        assert_eq!(pending[0].state, SyncOutboxState::Pending);
    }
}
