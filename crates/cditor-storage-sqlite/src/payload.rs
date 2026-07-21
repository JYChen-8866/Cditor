use std::collections::HashMap;

use sqlx::{QueryBuilder, Row, Sqlite};
use uuid::Uuid;

use cditor_core::ids::DocumentId;
use cditor_core::rich_text::{BlockPayloadRecord, kind_tag_for_rich_block_kind};
use cditor_storage::{LoadedPayloadBatch, StorageError, StorageResult};

use crate::error::{serialization_error, sqlite_error};
use crate::ids::{block_id_from_sqlite, block_id_to_sqlite, document_id_to_sqlite};
use crate::storage::SqliteDocumentStorage;
use crate::util::{checked_i64, checked_u64};

impl SqliteDocumentStorage {
    pub(crate) async fn load_payloads_inner(
        &self,
        document_id: DocumentId,
        block_ids: &[cditor_core::ids::BlockId],
    ) -> StorageResult<LoadedPayloadBatch> {
        if block_ids.is_empty() {
            return Ok(LoadedPayloadBatch {
                records: Vec::new(),
                missing_block_ids: Vec::new(),
            });
        }
        let mut by_id = HashMap::new();
        for chunk in block_ids.chunks(500) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "SELECT block_id, kind_json, payload_json, content_version FROM block_payloads WHERE document_id = ",
            );
            query.push_bind(document_id_to_sqlite(document_id));
            query.push(" AND block_id IN (");
            let mut separated = query.separated(", ");
            for block_id in chunk {
                separated.push_bind(block_id_to_sqlite(*block_id));
            }
            separated.push_unseparated(")");
            let rows = query
                .build()
                .fetch_all(&self.pool)
                .await
                .map_err(sqlite_error)?;
            for row in rows {
                let stored_id: Uuid = row.try_get("block_id").map_err(sqlite_error)?;
                let block_id = block_id_from_sqlite(stored_id).ok_or_else(|| {
                    StorageError::CorruptData(format!(
                        "payload block id {stored_id} is outside runtime namespace"
                    ))
                })?;
                let kind_json: String = row.try_get("kind_json").map_err(sqlite_error)?;
                let payload_json: String = row.try_get("payload_json").map_err(sqlite_error)?;
                by_id.insert(
                    block_id,
                    BlockPayloadRecord {
                        block_id,
                        content_version: checked_u64(
                            row.try_get("content_version").map_err(sqlite_error)?,
                            "content_version",
                        )?,
                        kind: serde_json::from_str(&kind_json).map_err(serialization_error)?,
                        payload: serde_json::from_str(&payload_json)
                            .map_err(serialization_error)?,
                    },
                );
            }
        }
        let mut records = Vec::with_capacity(by_id.len());
        let mut missing_block_ids = Vec::new();
        for block_id in block_ids {
            match by_id.remove(block_id) {
                Some(record) => records.push(record),
                None => missing_block_ids.push(*block_id),
            }
        }
        Ok(LoadedPayloadBatch {
            records,
            missing_block_ids,
        })
    }
}

pub(crate) async fn insert_payload(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    document_id: Uuid,
    payload: &BlockPayloadRecord,
    now: i64,
) -> StorageResult<()> {
    let block_id = block_id_to_sqlite(payload.block_id);
    let content_version = checked_i64(payload.content_version)?;
    let kind_json = serde_json::to_string(&payload.kind).map_err(serialization_error)?;
    let payload_json = serde_json::to_string(&payload.payload).map_err(serialization_error)?;
    let updated = sqlx::query(
        "UPDATE blocks SET kind_tag = ?, content_version = ?, updated_at = ? WHERE id = ? AND document_id = ? AND deleted_at IS NULL",
    )
    .bind(i64::from(kind_tag_for_rich_block_kind(&payload.kind)))
    .bind(content_version)
    .bind(now)
    .bind(block_id)
    .bind(document_id)
    .execute(&mut **transaction)
    .await
    .map_err(sqlite_error)?;
    if updated.rows_affected() == 0 {
        return Err(StorageError::NotFound {
            entity: "block",
            id: payload.block_id.to_string(),
        });
    }
    sqlx::query(
        r#"
        INSERT INTO block_payloads (
            block_id, document_id, kind_json, payload_json, plain_text,
            content_version, byte_len, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(document_id, block_id) DO UPDATE SET
            document_id = excluded.document_id,
            kind_json = excluded.kind_json,
            payload_json = excluded.payload_json,
            plain_text = excluded.plain_text,
            content_version = excluded.content_version,
            byte_len = excluded.byte_len,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(block_id)
    .bind(document_id)
    .bind(kind_json)
    .bind(&payload_json)
    .bind(payload.plain_text())
    .bind(content_version)
    .bind(i64::try_from(payload_json.len()).map_err(|_| {
        StorageError::CorruptData(format!(
            "block {} payload exceeds SQLite INTEGER byte length",
            payload.block_id
        ))
    })?)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(sqlite_error)?;
    Ok(())
}
