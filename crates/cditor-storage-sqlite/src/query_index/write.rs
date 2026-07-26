use std::collections::HashMap;

use cditor_core::ids::DocumentId;
use cditor_core::rich_text::BlockPayloadRecord;
use cditor_storage::{StorageError, StorageResult};
use sqlx::{QueryBuilder, Row, Sqlite, Transaction};
use uuid::Uuid;

use super::{backlink_kind_as_str, links_from_payload, sqlite_text_id};
use crate::error::sqlite_error;
use crate::ids::{block_id_to_sqlite, document_id_to_sqlite};
use crate::util::{checked_i64, unix_millis};

const WRITE_BATCH: usize = 512;

struct ProjectionRow<'a> {
    payload: &'a BlockPayloadRecord,
    block_uuid: Uuid,
    content_version: i64,
    fts_rowid: i64,
}

pub(crate) async fn update_query_projection_batch(
    transaction: &mut Transaction<'_, Sqlite>,
    document_id: DocumentId,
    payloads: &[BlockPayloadRecord],
) -> StorageResult<()> {
    if payloads.is_empty() {
        return Ok(());
    }
    let document_uuid = document_id_to_sqlite(document_id);
    let workspace_uuid: Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM documents WHERE id = ?")
            .bind(document_uuid)
            .fetch_one(&mut **transaction)
            .await
            .map_err(sqlite_error)?;
    let mut rowids = load_existing_rowids(transaction, document_uuid, payloads).await?;
    let mut next_rowid: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(fts_rowid), 0) FROM block_fts_state")
            .fetch_one(&mut **transaction)
            .await
            .map_err(sqlite_error)?;
    let mut rows = Vec::with_capacity(payloads.len());
    for payload in payloads {
        let block_uuid = block_id_to_sqlite(payload.block_id);
        let fts_rowid = match rowids.get(&block_uuid).copied().flatten() {
            Some(rowid) => rowid,
            None => {
                next_rowid = next_rowid.checked_add(1).ok_or_else(|| {
                    StorageError::CorruptData("FTS rowid space is exhausted".to_owned())
                })?;
                next_rowid
            }
        };
        rowids.insert(block_uuid, Some(fts_rowid));
        rows.push(ProjectionRow {
            payload,
            block_uuid,
            content_version: checked_i64(payload.content_version)?,
            fts_rowid,
        });
    }

    delete_fts_rows(transaction, &rows).await?;
    insert_fts_rows(transaction, workspace_uuid, document_uuid, &rows).await?;
    upsert_projection_state(transaction, document_uuid, &rows).await?;
    replace_backlinks(transaction, document_uuid, &rows).await
}

async fn load_existing_rowids(
    transaction: &mut Transaction<'_, Sqlite>,
    document_uuid: Uuid,
    payloads: &[BlockPayloadRecord],
) -> StorageResult<HashMap<Uuid, Option<i64>>> {
    let mut rowids = HashMap::with_capacity(payloads.len());
    for chunk in payloads.chunks(WRITE_BATCH) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT block_id, fts_rowid FROM block_fts_state WHERE document_id = ",
        );
        query.push_bind(document_uuid);
        query.push(" AND block_id IN (");
        let mut separated = query.separated(", ");
        for payload in chunk {
            separated.push_bind(block_id_to_sqlite(payload.block_id));
        }
        separated.push_unseparated(")");
        for row in query
            .build()
            .fetch_all(&mut **transaction)
            .await
            .map_err(sqlite_error)?
        {
            rowids.insert(
                row.try_get("block_id").map_err(sqlite_error)?,
                row.try_get("fts_rowid").map_err(sqlite_error)?,
            );
        }
    }
    Ok(rowids)
}

async fn delete_fts_rows(
    transaction: &mut Transaction<'_, Sqlite>,
    rows: &[ProjectionRow<'_>],
) -> StorageResult<()> {
    for chunk in rows.chunks(WRITE_BATCH) {
        let mut query = QueryBuilder::<Sqlite>::new("DELETE FROM block_fts WHERE rowid IN (");
        let mut separated = query.separated(", ");
        for row in chunk {
            separated.push_bind(row.fts_rowid);
        }
        separated.push_unseparated(")");
        query
            .build()
            .execute(&mut **transaction)
            .await
            .map_err(sqlite_error)?;
    }
    Ok(())
}

async fn insert_fts_rows(
    transaction: &mut Transaction<'_, Sqlite>,
    workspace_uuid: Uuid,
    document_uuid: Uuid,
    rows: &[ProjectionRow<'_>],
) -> StorageResult<()> {
    let workspace_key = sqlite_text_id(workspace_uuid);
    let document_key = sqlite_text_id(document_uuid);
    for chunk in rows.chunks(WRITE_BATCH) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT INTO block_fts \
             (rowid, workspace_id, document_id, block_id, content_version, plain_text) ",
        );
        query.push_values(chunk, |mut values, row| {
            values
                .push_bind(row.fts_rowid)
                .push_bind(&workspace_key)
                .push_bind(&document_key)
                .push_bind(sqlite_text_id(row.block_uuid))
                .push_bind(row.payload.content_version.to_string())
                .push_bind(row.payload.plain_text());
        });
        query
            .build()
            .execute(&mut **transaction)
            .await
            .map_err(sqlite_error)?;
    }
    Ok(())
}

async fn upsert_projection_state(
    transaction: &mut Transaction<'_, Sqlite>,
    document_uuid: Uuid,
    rows: &[ProjectionRow<'_>],
) -> StorageResult<()> {
    let now = unix_millis()?;
    for chunk in rows.chunks(WRITE_BATCH) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT INTO block_fts_state \
             (document_id, block_id, content_version, indexed_at, fts_rowid) ",
        );
        query.push_values(chunk, |mut values, row| {
            values
                .push_bind(document_uuid)
                .push_bind(row.block_uuid)
                .push_bind(row.content_version)
                .push_bind(now)
                .push_bind(row.fts_rowid);
        });
        query.push(
            " ON CONFLICT(document_id, block_id) DO UPDATE SET \
             content_version = excluded.content_version, indexed_at = excluded.indexed_at, \
             fts_rowid = excluded.fts_rowid",
        );
        query
            .build()
            .execute(&mut **transaction)
            .await
            .map_err(sqlite_error)?;
    }
    Ok(())
}

async fn replace_backlinks(
    transaction: &mut Transaction<'_, Sqlite>,
    document_uuid: Uuid,
    rows: &[ProjectionRow<'_>],
) -> StorageResult<()> {
    for chunk in rows.chunks(WRITE_BATCH) {
        let mut query =
            QueryBuilder::<Sqlite>::new("DELETE FROM document_links WHERE source_document_id = ");
        query.push_bind(document_uuid);
        query.push(" AND source_block_id IN (");
        let mut separated = query.separated(", ");
        for row in chunk {
            separated.push_bind(row.block_uuid);
        }
        separated.push_unseparated(")");
        query
            .build()
            .execute(&mut **transaction)
            .await
            .map_err(sqlite_error)?;
    }

    let now = unix_millis()?;
    let links = rows
        .iter()
        .flat_map(|row| {
            links_from_payload(row.payload)
                .into_iter()
                .map(move |link| (row.block_uuid, link))
        })
        .collect::<Vec<_>>();
    for chunk in links.chunks(WRITE_BATCH) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT OR IGNORE INTO document_links \
             (source_document_id, source_block_id, target_document_id, target_block_id, \
              link_kind, updated_at) ",
        );
        query.push_values(chunk, |mut values, (source_block_id, link)| {
            values
                .push_bind(document_uuid)
                .push_bind(*source_block_id)
                .push_bind(document_id_to_sqlite(link.document_id))
                .push_bind(link.block_id.map(block_id_to_sqlite))
                .push_bind(backlink_kind_as_str(link.kind))
                .push_bind(now);
        });
        query
            .build()
            .execute(&mut **transaction)
            .await
            .map_err(sqlite_error)?;
    }
    Ok(())
}
