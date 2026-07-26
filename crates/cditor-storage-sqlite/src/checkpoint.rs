use std::collections::HashSet;

use cditor_core::document::BlockIndexRecord;
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::rich_text::{BlockAttrs, BlockPayloadRecord};
use cditor_core::schema::VersionedEnvelope;
use cditor_storage::{
    EmergencyLogEntry, MATERIALIZED_CHECKPOINT_FORMAT, MATERIALIZED_CHECKPOINT_VERSION,
    MaterializedCheckpoint, MaterializedDocumentState, MaterializedRebuildPlan,
    StorageDocumentMetadata, StorageError, StorageResult,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::error::{serialization_error, sqlite_error};
use crate::ids::document_id_to_sqlite;
use crate::storage::SqliteDocumentStorage;
use crate::util::{checked_i64, unix_millis};

#[derive(Debug, Serialize, Deserialize)]
struct StoredMaterializedCheckpoint {
    format: String,
    version: u32,
    journal_sequence: u64,
    metadata: StoredDocumentMetadata,
    records: Vec<BlockIndexRecord>,
    block_attrs: Vec<(BlockId, BlockAttrs)>,
    payloads: Vec<BlockPayloadRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredDocumentMetadata {
    document_id: DocumentId,
    workspace_id: u64,
    title: String,
    structure_version: u64,
    content_version: u64,
    layout_version: u64,
    schema_version: u64,
}

impl SqliteDocumentStorage {
    pub async fn create_materialized_checkpoint(
        &self,
        document_id: DocumentId,
    ) -> StorageResult<MaterializedCheckpoint> {
        let _writer = self.writer_gate().acquire().await?;
        let metadata = self.load_metadata(document_id).await?;
        let records = self.load_records(document_id).await?;
        let block_attrs = self.load_attrs(document_id).await?;
        let block_ids = records.iter().map(|record| record.id).collect::<Vec<_>>();
        let loaded = self.load_payloads_inner(document_id, &block_ids).await?;
        if !loaded.missing_block_ids.is_empty() {
            return Err(StorageError::CorruptData(format!(
                "cannot checkpoint document {document_id}: {} payloads are missing",
                loaded.missing_block_ids.len()
            )));
        }

        let document_uuid = document_id_to_sqlite(document_id);
        let journal_sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(journal.id), 0) FROM operation_journal AS journal \
             INNER JOIN edit_transactions AS materialized \
               ON materialized.document_id = journal.document_id \
              AND materialized.transaction_id = CAST(journal.transaction_id AS TEXT) \
             WHERE journal.document_id = ?",
        )
        .bind(document_uuid)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlite_error)?;
        let journal_sequence = u64::try_from(journal_sequence).map_err(|_| {
            StorageError::CorruptData("checkpoint journal sequence cannot be negative".to_owned())
        })?;
        let stored = StoredMaterializedCheckpoint {
            format: MATERIALIZED_CHECKPOINT_FORMAT.to_owned(),
            version: MATERIALIZED_CHECKPOINT_VERSION,
            journal_sequence,
            metadata: StoredDocumentMetadata::from(metadata.clone()),
            records: records.clone(),
            block_attrs: block_attrs.clone(),
            payloads: loaded.records.clone(),
        };
        validate_stored_checkpoint(&stored, document_id)?;
        let snapshot_json = serde_json::to_string(&stored).map_err(serialization_error)?;
        let checksum = checkpoint_checksum(snapshot_json.as_bytes());
        let now = unix_millis()?;
        let mut transaction = self.pool.begin().await.map_err(sqlite_error)?;
        sqlx::query(
            "INSERT INTO runtime_snapshots \
             (document_id, structure_version, content_version, snapshot_json, created_at) \
             VALUES (?, ?, ?, ?, ?) ON CONFLICT(document_id) DO UPDATE SET \
             structure_version = excluded.structure_version, \
             content_version = excluded.content_version, \
             snapshot_json = excluded.snapshot_json, created_at = excluded.created_at",
        )
        .bind(document_uuid)
        .bind(checked_i64(metadata.structure_version)?)
        .bind(checked_i64(metadata.content_version)?)
        .bind(&snapshot_json)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(sqlite_error)?;
        sqlx::query(
            "INSERT INTO journal_checkpoints \
             (document_id, journal_id, materialized_checksum, created_at) VALUES (?, ?, ?, ?) \
             ON CONFLICT(document_id) DO UPDATE SET journal_id = excluded.journal_id, \
             materialized_checksum = excluded.materialized_checksum, \
             created_at = excluded.created_at",
        )
        .bind(document_uuid)
        .bind(checked_i64(journal_sequence)?)
        .bind(checksum as i64)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(sqlite_error)?;
        transaction.commit().await.map_err(sqlite_error)?;

        Ok(MaterializedCheckpoint {
            format: stored.format,
            version: stored.version,
            journal_sequence,
            checksum,
            state: MaterializedDocumentState {
                metadata,
                records,
                block_attrs,
                payloads: loaded.records,
            },
        })
    }

    pub async fn load_materialized_rebuild_plan(
        &self,
        document_id: DocumentId,
    ) -> StorageResult<Option<MaterializedRebuildPlan>> {
        let row = sqlx::query(
            "SELECT snapshot.snapshot_json, checkpoint.journal_id, \
                    checkpoint.materialized_checksum \
             FROM runtime_snapshots AS snapshot \
             INNER JOIN journal_checkpoints AS checkpoint \
               ON checkpoint.document_id = snapshot.document_id \
             WHERE snapshot.document_id = ?",
        )
        .bind(document_id_to_sqlite(document_id))
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let snapshot_json: String = row.try_get(0).map_err(sqlite_error)?;
        let stored_checksum = row.try_get::<i64, _>(2).map_err(sqlite_error)? as u64;
        let actual_checksum = checkpoint_checksum(snapshot_json.as_bytes());
        if actual_checksum != stored_checksum {
            return Err(StorageError::CorruptData(format!(
                "materialized checkpoint checksum mismatch for document {document_id}"
            )));
        }
        let stored: StoredMaterializedCheckpoint =
            serde_json::from_str(&snapshot_json).map_err(serialization_error)?;
        validate_stored_checkpoint(&stored, document_id)?;
        let row_sequence = row.try_get::<i64, _>(1).map_err(sqlite_error)?;
        let row_sequence = u64::try_from(row_sequence).map_err(|_| {
            StorageError::CorruptData("checkpoint journal sequence cannot be negative".to_owned())
        })?;
        if row_sequence != stored.journal_sequence {
            return Err(StorageError::CorruptData(format!(
                "checkpoint journal sequence mismatch: row {row_sequence}, snapshot {}",
                stored.journal_sequence
            )));
        }

        let entries = self.journal_entries_after_checkpoint(document_id).await?;
        let mut operations = Vec::with_capacity(entries.len());
        for entry in entries {
            let envelope: VersionedEnvelope =
                serde_json::from_str(&entry.envelope_json).map_err(serialization_error)?;
            if envelope.version != entry.schema_version {
                return Err(StorageError::CorruptData(format!(
                    "journal {} schema version disagrees with its envelope",
                    entry.journal_id
                )));
            }
            operations.push(EmergencyLogEntry {
                sequence: u64::try_from(entry.journal_id).map_err(|_| {
                    StorageError::CorruptData("journal id cannot be negative".to_owned())
                })?,
                transaction_id: entry.transaction_id,
                envelope,
            });
        }

        Ok(Some(MaterializedRebuildPlan {
            checkpoint: MaterializedCheckpoint {
                format: stored.format,
                version: stored.version,
                journal_sequence: stored.journal_sequence,
                checksum: stored_checksum,
                state: MaterializedDocumentState {
                    metadata: stored.metadata.into(),
                    records: stored.records,
                    block_attrs: stored.block_attrs,
                    payloads: stored.payloads,
                },
            },
            operations,
        }))
    }
}

fn validate_stored_checkpoint(
    checkpoint: &StoredMaterializedCheckpoint,
    expected_document_id: DocumentId,
) -> StorageResult<()> {
    if checkpoint.format != MATERIALIZED_CHECKPOINT_FORMAT {
        return Err(StorageError::CorruptData(format!(
            "unknown materialized checkpoint format {:?}",
            checkpoint.format
        )));
    }
    if checkpoint.version != MATERIALIZED_CHECKPOINT_VERSION {
        return Err(StorageError::CorruptData(format!(
            "unsupported materialized checkpoint version {}",
            checkpoint.version
        )));
    }
    if checkpoint.metadata.document_id != expected_document_id {
        return Err(StorageError::CorruptData(
            "materialized checkpoint belongs to another document".to_owned(),
        ));
    }
    let record_ids = checkpoint
        .records
        .iter()
        .map(|record| record.id)
        .collect::<HashSet<_>>();
    if record_ids.len() != checkpoint.records.len() {
        return Err(StorageError::CorruptData(
            "materialized checkpoint contains duplicate block ids".to_owned(),
        ));
    }
    let payload_ids = checkpoint
        .payloads
        .iter()
        .map(|payload| payload.block_id)
        .collect::<HashSet<_>>();
    if payload_ids != record_ids {
        return Err(StorageError::CorruptData(
            "materialized checkpoint payload coverage does not match its block index".to_owned(),
        ));
    }
    if checkpoint
        .block_attrs
        .iter()
        .any(|(block_id, _)| !record_ids.contains(block_id))
    {
        return Err(StorageError::CorruptData(
            "materialized checkpoint attrs reference a missing block".to_owned(),
        ));
    }
    Ok(())
}

fn checkpoint_checksum(bytes: &[u8]) -> u64 {
    let digest = Sha256::digest(bytes);
    u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 prefix is eight bytes"),
    )
}

impl From<StorageDocumentMetadata> for StoredDocumentMetadata {
    fn from(metadata: StorageDocumentMetadata) -> Self {
        Self {
            document_id: metadata.document_id,
            workspace_id: metadata.workspace_id,
            title: metadata.title,
            structure_version: metadata.structure_version,
            content_version: metadata.content_version,
            layout_version: metadata.layout_version,
            schema_version: metadata.schema_version,
        }
    }
}

impl From<StoredDocumentMetadata> for StorageDocumentMetadata {
    fn from(metadata: StoredDocumentMetadata) -> Self {
        Self {
            document_id: metadata.document_id,
            workspace_id: metadata.workspace_id,
            title: metadata.title,
            structure_version: metadata.structure_version,
            content_version: metadata.content_version,
            layout_version: metadata.layout_version,
            schema_version: metadata.schema_version,
        }
    }
}
