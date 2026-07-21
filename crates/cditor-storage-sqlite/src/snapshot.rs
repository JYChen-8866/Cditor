use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite};
use uuid::Uuid;

use cditor_core::document::BlockIndexRecord;
use cditor_core::ids::DocumentId;
use cditor_storage::StorageResult;

use crate::error::{serialization_error, sqlite_error};
use crate::ids::document_id_to_sqlite;
use crate::storage::SqliteDocumentStorage;

const INDEX_SNAPSHOT_FORMAT: &str = "block_index_json_v1";

#[derive(Debug, Serialize, Deserialize)]
struct SqliteDocumentIndexSnapshot {
    format: String,
    records: Vec<BlockIndexRecord>,
}

impl SqliteDocumentStorage {
    pub(crate) async fn load_index_snapshot(
        &self,
        document_id: DocumentId,
        visible_index_version: i64,
        structure_version: u64,
    ) -> StorageResult<Option<Vec<BlockIndexRecord>>> {
        let row = sqlx::query(
            r#"
            SELECT snapshot_json, block_count
            FROM document_index_snapshot
            WHERE document_id = ?
              AND visible_index_version = ?
              AND structure_version = ?
            "#,
        )
        .bind(document_id_to_sqlite(document_id))
        .bind(visible_index_version)
        .bind(crate::util::checked_i64(structure_version)?)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let json: String = row.try_get("snapshot_json").map_err(sqlite_error)?;
        let block_count: i64 = row.try_get("block_count").map_err(sqlite_error)?;

        // Index snapshots are derived data. Any malformed or stale snapshot is
        // treated as a cache miss so the authoritative blocks table can rebuild it.
        let Ok(snapshot) = serde_json::from_str::<SqliteDocumentIndexSnapshot>(&json) else {
            return Ok(None);
        };
        let mut block_ids = HashSet::with_capacity(snapshot.records.len());
        if snapshot.format != INDEX_SNAPSHOT_FORMAT
            || usize::try_from(block_count).ok() != Some(snapshot.records.len())
            || snapshot.records.iter().any(|record| {
                let height = record.layout_meta.effective_height();
                !block_ids.insert(record.id)
                    || record.id != record.layout_meta.block_id
                    || !height.is_finite()
                    || height <= 0.0
            })
        {
            return Ok(None);
        }
        Ok(Some(snapshot.records))
    }
}

pub(crate) async fn save_index_snapshot(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    document_id: Uuid,
    visible_index_version: i64,
    structure_version: u64,
    records: &[BlockIndexRecord],
    now: i64,
) -> StorageResult<()> {
    let snapshot = SqliteDocumentIndexSnapshot {
        format: INDEX_SNAPSHOT_FORMAT.to_owned(),
        records: records.to_vec(),
    };
    let snapshot_json = serde_json::to_string(&snapshot).map_err(serialization_error)?;
    sqlx::query(
        r#"
        INSERT INTO document_index_snapshot (
            document_id, visible_index_version, structure_version,
            snapshot_json, block_count, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(document_id, visible_index_version, structure_version) DO UPDATE SET
            snapshot_json = excluded.snapshot_json,
            block_count = excluded.block_count,
            created_at = excluded.created_at
        "#,
    )
    .bind(document_id)
    .bind(visible_index_version)
    .bind(crate::util::checked_i64(structure_version)?)
    .bind(snapshot_json)
    .bind(i64::try_from(records.len()).map_err(|_| {
        cditor_storage::StorageError::CorruptData(
            "document index snapshot block count exceeds SQLite INTEGER".to_owned(),
        )
    })?)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(sqlite_error)?;
    Ok(())
}
