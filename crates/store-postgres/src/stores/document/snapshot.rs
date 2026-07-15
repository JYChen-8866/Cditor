use serde::{Deserialize, Serialize};
use sqlx::Row;

use cditor_core::document::BlockIndexRecord;
use cditor_core::layout::BlockLayoutMeta;

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::PgDocumentId;

use super::PostgresDocumentStore;

impl PostgresDocumentStore {
    pub async fn save_document_index_snapshot(
        &self,
        document_id: PgDocumentId,
        visible_index_version: i64,
        structure_version: i64,
        records: &[BlockIndexRecord],
    ) -> PostgresStorageResult<()> {
        let mut tx = self.pool.begin().await?;
        self.save_document_index_snapshot_tx(
            &mut tx,
            document_id,
            visible_index_version,
            structure_version,
            records,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn save_document_index_snapshot_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        document_id: PgDocumentId,
        visible_index_version: i64,
        structure_version: i64,
        records: &[BlockIndexRecord],
    ) -> PostgresStorageResult<()> {
        let snapshot = DbDocumentIndexSnapshot {
            records: records
                .iter()
                .copied()
                .map(DbBlockIndexSnapshotRecord::from)
                .collect(),
        };
        let snapshot_bytes = serde_json::to_vec(&snapshot)?;
        let block_count =
            i32::try_from(records.len()).map_err(|_| PostgresStorageError::CorruptData {
                message: format!(
                    "document index snapshot block_count {} exceeds INTEGER range",
                    records.len()
                ),
            })?;

        sqlx::query(
            r#"
            INSERT INTO document_index_snapshot (
                document_id,
                visible_index_version,
                structure_version,
                snapshot_format,
                snapshot_bytes,
                block_count,
                created_at
            )
            VALUES ($1, $2, $3, 'block_index_json_v1', $4, $5, now())
            ON CONFLICT (document_id, visible_index_version, structure_version) DO UPDATE SET
                snapshot_format = EXCLUDED.snapshot_format,
                snapshot_bytes = EXCLUDED.snapshot_bytes,
                block_count = EXCLUDED.block_count,
                created_at = now()
            "#,
        )
        .bind(document_id)
        .bind(visible_index_version)
        .bind(structure_version)
        .bind(snapshot_bytes)
        .bind(block_count)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    pub async fn load_document_index_snapshot(
        &self,
        document_id: PgDocumentId,
        visible_index_version: i64,
        structure_version: i64,
    ) -> PostgresStorageResult<Option<Vec<BlockIndexRecord>>> {
        let row = sqlx::query(
            r#"
            SELECT snapshot_format, snapshot_bytes, block_count
            FROM document_index_snapshot
            WHERE document_id = $1 AND visible_index_version = $2 AND structure_version = $3
            "#,
        )
        .bind(document_id)
        .bind(visible_index_version)
        .bind(structure_version)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let snapshot_format: String = row.try_get("snapshot_format")?;
        if snapshot_format != "block_index_json_v1" {
            return Err(PostgresStorageError::CorruptData {
                message: format!("unsupported document index snapshot format {snapshot_format}"),
            });
        }
        let snapshot_bytes: Vec<u8> = row.try_get("snapshot_bytes")?;
        let expected_block_count: i32 = row.try_get("block_count")?;
        let snapshot: DbDocumentIndexSnapshot = serde_json::from_slice(&snapshot_bytes)?;
        if snapshot.records.len() != usize::try_from(expected_block_count).unwrap_or(usize::MAX) {
            return Err(PostgresStorageError::CorruptData {
                message: format!(
                    "document index snapshot block_count mismatch: expected {expected_block_count}, found {}",
                    snapshot.records.len()
                ),
            });
        }

        Ok(Some(
            snapshot
                .records
                .into_iter()
                .map(BlockIndexRecord::from)
                .collect(),
        ))
    }

    pub async fn has_document_index_snapshot(
        &self,
        document_id: PgDocumentId,
        visible_index_version: i64,
        structure_version: i64,
    ) -> PostgresStorageResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM document_index_snapshot
                WHERE document_id = $1
                  AND visible_index_version = $2
                  AND structure_version = $3
            )
            "#,
        )
        .bind(document_id)
        .bind(visible_index_version)
        .bind(structure_version)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DbDocumentIndexSnapshot {
    records: Vec<DbBlockIndexSnapshotRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct DbBlockIndexSnapshotRecord {
    id: u64,
    parent_id: Option<u64>,
    depth: u16,
    kind_tag: u16,
    flags: u32,
    estimated_height: u64,
    measured_height: Option<u64>,
    width_bucket: u16,
    layout_version: u64,
    dirty: bool,
}

impl From<BlockIndexRecord> for DbBlockIndexSnapshotRecord {
    fn from(record: BlockIndexRecord) -> Self {
        Self {
            id: record.id,
            parent_id: record.parent_id,
            depth: record.depth,
            kind_tag: record.kind_tag,
            flags: record.flags,
            estimated_height: record.layout_meta.estimated_height.to_bits(),
            measured_height: record.layout_meta.measured_height.map(f64::to_bits),
            width_bucket: record.layout_meta.width_bucket,
            layout_version: record.layout_meta.layout_version,
            dirty: record.layout_meta.dirty,
        }
    }
}

impl From<DbBlockIndexSnapshotRecord> for BlockIndexRecord {
    fn from(record: DbBlockIndexSnapshotRecord) -> Self {
        BlockIndexRecord::new(
            record.id,
            record.parent_id,
            record.depth,
            record.kind_tag,
            record.flags,
        )
        .with_layout_meta(BlockLayoutMeta {
            block_id: record.id,
            estimated_height: f64::from_bits(record.estimated_height),
            measured_height: record.measured_height.map(f64::from_bits),
            width_bucket: record.width_bucket,
            layout_version: record.layout_version,
            dirty: record.dirty,
        })
    }
}
