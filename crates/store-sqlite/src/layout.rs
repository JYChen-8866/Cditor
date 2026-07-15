use std::collections::HashMap;

use sqlx::{QueryBuilder, Row, Sqlite};
use uuid::Uuid;

use cditor_core::document::BlockIndexRecord;
use cditor_core::ids::{BlockId, DocumentId};
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{StorageError, StorageResult};

use crate::error::sqlite_error;
use crate::ids::{block_id_from_sqlite, block_id_to_sqlite, document_id_to_sqlite};
use crate::storage::SqliteDocumentStorage;
use crate::util::{checked_i64, checked_u64};

#[derive(Debug)]
struct StoredBlockLayout {
    layout_key_hash: String,
    estimated_height: f64,
    measured_height: Option<f64>,
    content_version: u64,
    updated_at: i64,
}

impl SqliteDocumentStorage {
    pub(crate) async fn apply_block_layout_cache(
        &self,
        document_id: DocumentId,
        records: &mut [BlockIndexRecord],
        base_key: LayoutCacheKey,
    ) -> StorageResult<usize> {
        let mut by_block: HashMap<BlockId, Vec<StoredBlockLayout>> = HashMap::new();
        for chunk in records.chunks(500) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "SELECT block_id, layout_key_hash, estimated_height, measured_height, content_version, updated_at FROM block_layout WHERE document_id = ",
            );
            query.push_bind(document_id_to_sqlite(document_id));
            query.push(" AND block_id IN (");
            let mut separated = query.separated(", ");
            for record in chunk {
                separated.push_bind(block_id_to_sqlite(record.id));
            }
            separated.push_unseparated(") ORDER BY updated_at DESC");
            let rows = query
                .build()
                .fetch_all(&self.pool)
                .await
                .map_err(sqlite_error)?;
            for row in rows {
                let stored_id: Uuid = row.try_get("block_id").map_err(sqlite_error)?;
                let block_id = block_id_from_sqlite(stored_id).ok_or_else(|| {
                    StorageError::CorruptData(format!(
                        "layout block id {stored_id} is outside runtime namespace"
                    ))
                })?;
                by_block
                    .entry(block_id)
                    .or_default()
                    .push(StoredBlockLayout {
                        layout_key_hash: row.try_get("layout_key_hash").map_err(sqlite_error)?,
                        estimated_height: row.try_get("estimated_height").map_err(sqlite_error)?,
                        measured_height: row.try_get("measured_height").map_err(sqlite_error)?,
                        content_version: checked_u64(
                            row.try_get("content_version").map_err(sqlite_error)?,
                            "layout content_version",
                        )?,
                        updated_at: row.try_get("updated_at").map_err(sqlite_error)?,
                    });
            }
        }

        let mut hits = 0usize;
        for record in records {
            let Some(cached) = by_block.get_mut(&record.id) else {
                continue;
            };
            cached.sort_by_key(|row| std::cmp::Reverse(row.updated_at));
            let exact_hash = layout_key_for_record(base_key, record).hash_key();
            let exact = cached.iter().find(|row| row.layout_key_hash == exact_hash);
            if let Some(row) = exact {
                hits += 1;
                record.layout_meta.estimated_height = row.estimated_height;
                record.layout_meta.measured_height = row.measured_height;
                record.layout_meta.width_bucket = base_key.width_bucket;
                record.layout_meta.layout_version = row.content_version;
                record.layout_meta.dirty = row.measured_height.is_none();
            } else if let Some(row) = cached.first() {
                hits += 1;
                record.layout_meta.estimated_height =
                    row.measured_height.unwrap_or(row.estimated_height);
                record.layout_meta.measured_height = None;
                record.layout_meta.width_bucket = base_key.width_bucket;
                record.layout_meta.dirty = true;
            }
        }
        Ok(hits)
    }
}

pub(crate) async fn save_block_layouts(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    document_id: Uuid,
    records: &[BlockIndexRecord],
    base_key: LayoutCacheKey,
    now: i64,
) -> StorageResult<()> {
    for record in records {
        let key = layout_key_for_record(base_key, record);
        sqlx::query(
            r#"
            INSERT INTO block_layout (
                document_id, block_id, layout_key_hash, estimated_height,
                measured_height, content_version, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(document_id, block_id, layout_key_hash) DO UPDATE SET
                estimated_height = excluded.estimated_height,
                measured_height = excluded.measured_height,
                content_version = excluded.content_version,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(document_id)
        .bind(block_id_to_sqlite(record.id))
        .bind(key.hash_key())
        .bind(record.layout_meta.estimated_height)
        .bind(record.layout_meta.measured_height)
        .bind(checked_i64(key.content_version)?)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(sqlite_error)?;
    }
    Ok(())
}

fn layout_key_for_record(
    mut base_key: LayoutCacheKey,
    record: &BlockIndexRecord,
) -> LayoutCacheKey {
    if record.layout_meta.layout_version > 0 {
        base_key.content_version = record.layout_meta.layout_version;
    }
    base_key
}
