use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan};
use cditor_storage::query_index::{
    BacklinkKind, BacklinkRecord, FtsApplyResult, LocalIndexRebuildRequest, LocalSearchHit,
    LocalSearchRequest,
};
use cditor_storage::{StorageError, StorageResult};
use sqlx::{Row, Sqlite, Transaction};
use uuid::Uuid;

use crate::error::sqlite_error;
use crate::ids::{
    block_id_from_sqlite, block_id_to_sqlite, document_id_from_sqlite, document_id_to_sqlite,
};
use crate::storage::SqliteDocumentStorage;

mod write;

pub(crate) use write::update_query_projection_batch;

impl SqliteDocumentStorage {
    pub async fn rebuild_local_query_index(
        &self,
        request: LocalIndexRebuildRequest,
    ) -> StorageResult<FtsApplyResult> {
        if request.max_blocks == 0 {
            return Ok(FtsApplyResult {
                applied: 0,
                remaining: stale_query_projection_count(&self.pool, request.document_id).await?,
            });
        }
        let _writer = self.writer_gate().acquire().await?;
        let document_uuid = document_id_to_sqlite(request.document_id);
        if request.reset {
            let mut transaction = self.pool.begin().await.map_err(sqlite_error)?;
            sqlx::query("DELETE FROM block_fts WHERE document_id = ?")
                .bind(sqlite_text_id(document_uuid))
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            sqlx::query("DELETE FROM block_fts_state WHERE document_id = ?")
                .bind(document_uuid)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            sqlx::query("DELETE FROM document_links WHERE source_document_id = ?")
                .bind(document_uuid)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            transaction.commit().await.map_err(sqlite_error)?;
        }
        let limit = i64::try_from(request.max_blocks.min(4_096)).map_err(|_| {
            StorageError::CorruptData("query-index rebuild batch exceeds SQLite range".into())
        })?;
        let rows = sqlx::query(
            "SELECT blocks.id FROM blocks \
             INNER JOIN block_payloads AS payload \
               ON payload.document_id = blocks.document_id AND payload.block_id = blocks.id \
             LEFT JOIN block_fts_state AS state \
               ON state.document_id = blocks.document_id AND state.block_id = blocks.id \
             WHERE blocks.document_id = ? AND blocks.deleted_at IS NULL \
               AND (state.block_id IS NULL OR state.content_version != payload.content_version) \
             ORDER BY blocks.sort_key LIMIT ?",
        )
        .bind(document_uuid)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_error)?;
        let block_ids = rows
            .into_iter()
            .map(|row| {
                let id: Uuid = row.try_get(0).map_err(sqlite_error)?;
                block_id_from_sqlite(id).ok_or_else(|| {
                    StorageError::CorruptData("query-index block id is invalid".into())
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;
        let loaded = self
            .load_payloads_inner(request.document_id, &block_ids)
            .await?;
        if !loaded.missing_block_ids.is_empty() {
            return Err(StorageError::CorruptData(
                "query-index rebuild payload disappeared during a writer-gated batch".into(),
            ));
        }
        let applied = loaded.records.len();
        let mut transaction = self.pool.begin().await.map_err(sqlite_error)?;
        update_query_projection_batch(&mut transaction, request.document_id, &loaded.records)
            .await?;
        prune_deleted_query_projection(&mut transaction, request.document_id).await?;
        transaction.commit().await.map_err(sqlite_error)?;
        Ok(FtsApplyResult {
            applied,
            remaining: stale_query_projection_count(&self.pool, request.document_id).await?,
        })
    }

    pub async fn search_local(
        &self,
        request: LocalSearchRequest,
    ) -> StorageResult<Vec<LocalSearchHit>> {
        if request.limit == 0 {
            return Ok(Vec::new());
        }
        let Some(query) = normalized_fts_query(&request.query) else {
            return Ok(Vec::new());
        };
        let workspace_id = sqlite_text_id(Uuid::from_u128(request.workspace_id as u128));
        let document_id = request
            .document_id
            .map(|id| sqlite_text_id(document_id_to_sqlite(id)));
        let limit = i64::try_from(request.limit.min(1_000))
            .map_err(|_| StorageError::CorruptData("search limit exceeds SQLite range".into()))?;
        let rows = sqlx::query(
            "SELECT document_id, block_id, CAST(content_version AS INTEGER), \
                    bm25(block_fts), snippet(block_fts, 4, '', '', '...', 16) \
             FROM block_fts WHERE block_fts MATCH ? AND workspace_id = ? \
               AND (? IS NULL OR document_id = ?) ORDER BY bm25(block_fts), rowid LIMIT ?",
        )
        .bind(query)
        .bind(workspace_id)
        .bind(document_id.as_deref())
        .bind(document_id.as_deref())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_error)?;
        rows.into_iter().map(search_hit_from_row).collect()
    }

    pub async fn backlinks(
        &self,
        target_document_id: DocumentId,
        target_block_id: Option<BlockId>,
        limit: usize,
    ) -> StorageResult<Vec<BacklinkRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT links.source_document_id, links.source_block_id, \
                    links.target_document_id, links.target_block_id, links.link_kind, \
                    CASE WHEN links.target_block_id IS NULL THEN EXISTS( \
                        SELECT 1 FROM documents target WHERE target.id = links.target_document_id \
                          AND target.deleted_at IS NULL) ELSE EXISTS( \
                        SELECT 1 FROM blocks target WHERE target.document_id = links.target_document_id \
                          AND target.id = links.target_block_id AND target.deleted_at IS NULL) END \
             FROM document_links AS links WHERE links.target_document_id = ? \
               AND (? IS NULL OR links.target_block_id = ?) \
             ORDER BY links.updated_at DESC, links.source_document_id, links.source_block_id \
             LIMIT ?",
        )
        .bind(document_id_to_sqlite(target_document_id))
        .bind(target_block_id.map(block_id_to_sqlite))
        .bind(target_block_id.map(block_id_to_sqlite))
        .bind(i64::try_from(limit.min(1_000)).map_err(|_| {
            StorageError::CorruptData("backlink limit exceeds SQLite range".into())
        })?)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_error)?;
        rows.into_iter().map(backlink_from_row).collect()
    }
}

async fn stale_query_projection_count(
    pool: &sqlx::SqlitePool,
    document_id: DocumentId,
) -> StorageResult<usize> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM blocks \
         INNER JOIN block_payloads AS payload \
           ON payload.document_id = blocks.document_id AND payload.block_id = blocks.id \
         LEFT JOIN block_fts_state AS state \
           ON state.document_id = blocks.document_id AND state.block_id = blocks.id \
         WHERE blocks.document_id = ? AND blocks.deleted_at IS NULL \
           AND (state.block_id IS NULL OR state.content_version != payload.content_version)",
    )
    .bind(document_id_to_sqlite(document_id))
    .fetch_one(pool)
    .await
    .map_err(sqlite_error)?;
    usize::try_from(count)
        .map_err(|_| StorageError::CorruptData("query-index stale count is invalid".into()))
}

pub(crate) async fn prune_deleted_query_projection(
    transaction: &mut Transaction<'_, Sqlite>,
    document_id: DocumentId,
) -> StorageResult<()> {
    let document_uuid = document_id_to_sqlite(document_id);
    let document_key = sqlite_text_id(document_uuid);
    sqlx::query(
        "DELETE FROM block_fts WHERE document_id = ? AND block_id NOT IN \
         (SELECT lower(hex(id)) FROM blocks WHERE document_id = ? AND deleted_at IS NULL)",
    )
    .bind(document_key)
    .bind(document_uuid)
    .execute(&mut **transaction)
    .await
    .map_err(sqlite_error)?;
    sqlx::query(
        "DELETE FROM block_fts_state WHERE document_id = ? AND block_id IN \
         (SELECT id FROM blocks WHERE document_id = ? AND deleted_at IS NOT NULL)",
    )
    .bind(document_uuid)
    .bind(document_uuid)
    .execute(&mut **transaction)
    .await
    .map_err(sqlite_error)?;
    sqlx::query(
        "DELETE FROM document_links WHERE source_document_id = ? AND source_block_id IN \
         (SELECT id FROM blocks WHERE document_id = ? AND deleted_at IS NOT NULL)",
    )
    .bind(document_uuid)
    .bind(document_uuid)
    .execute(&mut **transaction)
    .await
    .map_err(sqlite_error)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct InternalLink {
    document_id: DocumentId,
    block_id: Option<BlockId>,
    kind: BacklinkKind,
}

fn backlink_kind_as_str(kind: BacklinkKind) -> &'static str {
    match kind {
        BacklinkKind::InlineLink => "inline_link",
        BacklinkKind::Embed => "embed",
    }
}

fn parse_backlink_kind(value: &str) -> StorageResult<BacklinkKind> {
    match value {
        "inline_link" => Ok(BacklinkKind::InlineLink),
        "embed" => Ok(BacklinkKind::Embed),
        other => Err(StorageError::CorruptData(format!(
            "unknown backlink kind {other:?}"
        ))),
    }
}

fn links_from_payload(payload: &BlockPayloadRecord) -> Vec<InternalLink> {
    let mut links = Vec::new();
    match &payload.payload {
        BlockPayload::RichText { spans } => collect_span_links(spans, &mut links),
        BlockPayload::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    collect_span_links(&cell.spans, &mut links);
                }
            }
        }
        BlockPayload::Image(image) => collect_span_links(&image.caption.spans, &mut links),
        BlockPayload::Collection(collection) => {
            collect_span_links(&collection.title.spans, &mut links);
        }
        BlockPayload::Embed(embed) => {
            if let Some((document_id, block_id)) = parse_internal_href(&embed.url) {
                links.push(InternalLink {
                    document_id,
                    block_id,
                    kind: BacklinkKind::Embed,
                });
            }
        }
        _ => {}
    }
    links.sort_by_key(|link| (link.document_id, link.block_id, link.kind as u8));
    links.dedup();
    links
}

fn collect_span_links(spans: &[InlineSpan], links: &mut Vec<InternalLink>) {
    for mark in spans.iter().flat_map(|span| &span.marks) {
        if let InlineMark::Link { href } = mark
            && let Some((document_id, block_id)) = parse_internal_href(href)
        {
            links.push(InternalLink {
                document_id,
                block_id,
                kind: BacklinkKind::InlineLink,
            });
        }
    }
}

fn parse_internal_href(href: &str) -> Option<(DocumentId, Option<BlockId>)> {
    let path = href.strip_prefix("cditor://document/")?;
    let mut segments = path.split('/');
    let document_id = segments.next()?.parse().ok()?;
    let block_id = match (segments.next(), segments.next(), segments.next()) {
        (None, None, None) => None,
        (Some("block"), Some(block_id), None) => Some(block_id.parse().ok()?),
        _ => return None,
    };
    Some((document_id, block_id))
}

fn normalized_fts_query(query: &str) -> Option<String> {
    let tokens = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"*", token.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    (!tokens.is_empty()).then(|| tokens.join(" AND "))
}

fn sqlite_text_id(id: Uuid) -> String {
    id.simple().to_string()
}

fn search_hit_from_row(row: sqlx::sqlite::SqliteRow) -> StorageResult<LocalSearchHit> {
    let document_uuid = parse_text_id(
        row.try_get::<String, _>(0).map_err(sqlite_error)?,
        "document",
    )?;
    let block_uuid = parse_text_id(row.try_get::<String, _>(1).map_err(sqlite_error)?, "block")?;
    Ok(LocalSearchHit {
        document_id: document_id_from_sqlite(document_uuid).ok_or_else(|| {
            StorageError::CorruptData("search document id is outside runtime namespace".into())
        })?,
        block_id: block_id_from_sqlite(block_uuid).ok_or_else(|| {
            StorageError::CorruptData("search block id is outside runtime namespace".into())
        })?,
        content_version: u64::try_from(row.try_get::<i64, _>(2).map_err(sqlite_error)?).map_err(
            |_| StorageError::CorruptData("search content version cannot be negative".into()),
        )?,
        rank: row.try_get(3).map_err(sqlite_error)?,
        snippet: row.try_get(4).map_err(sqlite_error)?,
    })
}

fn backlink_from_row(row: sqlx::sqlite::SqliteRow) -> StorageResult<BacklinkRecord> {
    let source_document: Uuid = row.try_get(0).map_err(sqlite_error)?;
    let source_block: Uuid = row.try_get(1).map_err(sqlite_error)?;
    let target_document: Uuid = row.try_get(2).map_err(sqlite_error)?;
    let target_block: Option<Uuid> = row.try_get(3).map_err(sqlite_error)?;
    Ok(BacklinkRecord {
        source_document_id: document_id_from_sqlite(source_document).ok_or_else(|| {
            StorageError::CorruptData("backlink source document id is invalid".into())
        })?,
        source_block_id: block_id_from_sqlite(source_block).ok_or_else(|| {
            StorageError::CorruptData("backlink source block id is invalid".into())
        })?,
        target_document_id: document_id_from_sqlite(target_document).ok_or_else(|| {
            StorageError::CorruptData("backlink target document id is invalid".into())
        })?,
        target_block_id: target_block
            .map(|id| {
                block_id_from_sqlite(id).ok_or_else(|| {
                    StorageError::CorruptData("backlink target block id is invalid".into())
                })
            })
            .transpose()?,
        kind: parse_backlink_kind(row.try_get::<String, _>(4).map_err(sqlite_error)?.as_str())?,
        resolved: row.try_get::<i64, _>(5).map_err(sqlite_error)? != 0,
    })
}

fn parse_text_id(value: String, entity: &str) -> StorageResult<Uuid> {
    Uuid::parse_str(&value)
        .map_err(|_| StorageError::CorruptData(format!("search {entity} id is invalid")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_href_parser_requires_stable_canonical_ids() {
        assert_eq!(parse_internal_href("cditor://document/7"), Some((7, None)));
        assert_eq!(
            parse_internal_href("cditor://document/7/block/9"),
            Some((7, Some(9)))
        );
        assert_eq!(parse_internal_href("https://example.com"), None);
        assert_eq!(parse_internal_href("cditor://document/title"), None);
    }

    #[test]
    fn fts_query_quotes_user_syntax_and_keeps_unicode_tokens() {
        assert_eq!(
            normalized_fts_query("hello OR 世界"),
            Some("\"hello\"* AND \"OR\"* AND \"世界\"*".to_owned())
        );
        assert_eq!(normalized_fts_query("***"), None);
    }
}
