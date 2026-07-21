//! SQLite-backed spill storage for large undo transactions.
//!
//! Runtime keeps a pending range snapshot in memory until this write succeeds.
//! Only then may the core undo stack replace it with the returned opaque
//! reference, so disk errors never destroy the last recoverable inverse.

use cditor_core::edit::{
    EditTransaction, ExternalUndoBlobRef, TransactionDecodeOutcome, UndoStack, decode_transaction,
    encode_transaction,
};
use cditor_core::ids::DocumentId;
use cditor_core::schema::VersionedEnvelope;
use cditor_storage::{StorageError, StorageResult};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::error::sqlite_error;
use crate::ids::document_id_to_sqlite;
use crate::storage::SqliteDocumentStorage;
use crate::util::{checked_i64, checked_u64, unix_millis};

const STORAGE_KEY_PREFIX: &str = "sqlite-undo:";
const UNDO_BLOB_CODEC: &str = "operation-envelope-json-v1";

impl SqliteDocumentStorage {
    pub async fn spill_next_undo_snapshot(
        &self,
        document_id: DocumentId,
        undo: &mut UndoStack,
    ) -> StorageResult<Option<ExternalUndoBlobRef>> {
        let Some(job) = undo.begin_externalization() else {
            return Ok(None);
        };
        let reference = match self
            .write_undo_blob(
                document_id,
                job.snapshot_id,
                job.block_count,
                &job.transaction,
            )
            .await
        {
            Ok(reference) => reference,
            Err(error) => {
                let restored = undo.abort_externalization(job);
                debug_assert!(restored, "spill failure must restore pending snapshot");
                return Err(error);
            }
        };
        if let Err(job) = undo.complete_externalization(job, reference.clone()) {
            // The stack changed while awaiting I/O. The just-written blob is
            // no longer reachable and must not leak indefinitely.
            let restored = undo.abort_externalization(job);
            debug_assert!(restored, "changed spill entry must restore its transaction");
            let _ = self
                .delete_undo_blob(document_id, reference.snapshot_id)
                .await;
            return Err(StorageError::Conflict(format!(
                "undo snapshot {} changed during spill",
                reference.snapshot_id
            )));
        }
        Ok(Some(reference))
    }

    pub async fn hydrate_undo_snapshot(
        &self,
        document_id: DocumentId,
        undo: &mut UndoStack,
        reference: &ExternalUndoBlobRef,
    ) -> StorageResult<bool> {
        let transaction = self.load_undo_blob(document_id, reference).await?;
        Ok(undo.hydrate_externalized(reference.snapshot_id, transaction))
    }

    pub async fn write_undo_blob(
        &self,
        document_id: DocumentId,
        snapshot_id: u64,
        block_count: usize,
        transaction: &EditTransaction,
    ) -> StorageResult<ExternalUndoBlobRef> {
        let envelope = encode_transaction(transaction)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        let payload = serde_json::to_vec(&envelope)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        let checksum = sha256_checksum(&payload);
        let now = unix_millis()?;
        let _writer = self.writer_gate().acquire().await?;
        sqlx::query(
            "INSERT INTO undo_blobs \
             (document_id, snapshot_id, block_count, codec, payload, checksum, encoded_len, created_at, last_accessed_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(document_id, snapshot_id) DO UPDATE SET \
             block_count = excluded.block_count, codec = excluded.codec, \
             payload = excluded.payload, checksum = excluded.checksum, \
             encoded_len = excluded.encoded_len, last_accessed_at = excluded.last_accessed_at",
        )
        .bind(document_id_to_sqlite(document_id))
        .bind(checked_i64(snapshot_id)?)
        .bind(checked_i64(block_count as u64)?)
        .bind(UNDO_BLOB_CODEC)
        .bind(&payload)
        .bind(&checksum)
        .bind(checked_i64(payload.len() as u64)?)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(sqlite_error)?;
        let row_id = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM undo_blobs WHERE document_id = ? AND snapshot_id = ?",
        )
        .bind(document_id_to_sqlite(document_id))
        .bind(checked_i64(snapshot_id)?)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlite_error)?;
        Ok(ExternalUndoBlobRef {
            snapshot_id,
            storage_key: format!("{STORAGE_KEY_PREFIX}{row_id}"),
            checksum,
            encoded_len: payload.len(),
            block_count,
        })
    }

    pub async fn load_undo_blob(
        &self,
        document_id: DocumentId,
        reference: &ExternalUndoBlobRef,
    ) -> StorageResult<EditTransaction> {
        let row_id = parse_storage_key(&reference.storage_key)?;
        let row = sqlx::query(
            "SELECT snapshot_id, block_count, codec, payload, checksum, encoded_len \
             FROM undo_blobs WHERE id = ? AND document_id = ?",
        )
        .bind(row_id)
        .bind(document_id_to_sqlite(document_id))
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?
        .ok_or_else(|| StorageError::NotFound {
            entity: "undo blob",
            id: reference.storage_key.clone(),
        })?;
        let snapshot_id = checked_u64(row.get(0), "undo snapshot id")?;
        let block_count = checked_u64(row.get(1), "undo block count")? as usize;
        let codec: String = row.get(2);
        let payload: Vec<u8> = row.get(3);
        let stored_checksum: String = row.get(4);
        let encoded_len = checked_u64(row.get(5), "undo encoded length")? as usize;
        if snapshot_id != reference.snapshot_id
            || block_count != reference.block_count
            || codec != UNDO_BLOB_CODEC
            || encoded_len != payload.len()
            || encoded_len != reference.encoded_len
        {
            return Err(StorageError::CorruptData(format!(
                "undo blob metadata mismatch for {}",
                reference.storage_key
            )));
        }
        let actual_checksum = sha256_checksum(&payload);
        if stored_checksum != actual_checksum || reference.checksum != actual_checksum {
            return Err(StorageError::CorruptData(format!(
                "undo blob checksum mismatch for {}",
                reference.storage_key
            )));
        }
        let envelope: VersionedEnvelope = serde_json::from_slice(&payload)
            .map_err(|error| StorageError::CorruptData(error.to_string()))?;
        let transaction = match decode_transaction(&envelope)
            .map_err(|error| StorageError::CorruptData(error.to_string()))?
        {
            TransactionDecodeOutcome::Compatible(transaction) => *transaction,
            other => {
                return Err(StorageError::CorruptData(format!(
                    "undo transaction is not compatible: {other:?}"
                )));
            }
        };
        let _writer = self.writer_gate().acquire().await?;
        sqlx::query("UPDATE undo_blobs SET last_accessed_at = ? WHERE id = ?")
            .bind(unix_millis()?)
            .bind(row_id)
            .execute(&self.pool)
            .await
            .map_err(sqlite_error)?;
        Ok(transaction)
    }

    pub async fn delete_undo_blob(
        &self,
        document_id: DocumentId,
        snapshot_id: u64,
    ) -> StorageResult<bool> {
        let _writer = self.writer_gate().acquire().await?;
        let result =
            sqlx::query("DELETE FROM undo_blobs WHERE document_id = ? AND snapshot_id = ?")
                .bind(document_id_to_sqlite(document_id))
                .bind(checked_i64(snapshot_id)?)
                .execute(&self.pool)
                .await
                .map_err(sqlite_error)?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn prune_undo_blobs(
        &self,
        document_id: DocumentId,
        retain_most_recent: usize,
    ) -> StorageResult<u64> {
        let _writer = self.writer_gate().acquire().await?;
        let result = sqlx::query(
            "DELETE FROM undo_blobs WHERE document_id = ? AND id NOT IN (\
             SELECT id FROM undo_blobs WHERE document_id = ? \
             ORDER BY last_accessed_at DESC, id DESC LIMIT ?)",
        )
        .bind(document_id_to_sqlite(document_id))
        .bind(document_id_to_sqlite(document_id))
        .bind(checked_i64(retain_most_recent as u64)?)
        .execute(&self.pool)
        .await
        .map_err(sqlite_error)?;
        Ok(result.rows_affected())
    }
}

fn parse_storage_key(storage_key: &str) -> StorageResult<i64> {
    storage_key
        .strip_prefix(STORAGE_KEY_PREFIX)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| StorageError::CorruptData(format!("invalid undo storage key {storage_key}")))
}

fn sha256_checksum(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
