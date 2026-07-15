use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use cditor_core::document::BlockIndexRecord;
use cditor_core::layout::BlockLayoutMeta;
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
use cditor_storage::{
    DocumentStorage, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch, StorageBackendKind,
    StorageCapabilities, StorageDocumentMetadata, StorageError, StorageResult, StorageSaveBatch,
    StorageSaveOutcome,
};

use crate::codec::{decode_attrs, encode_attrs, encode_transaction};
use crate::config::{SqliteDurability, SqliteStorageOptions};
use crate::error::sqlite_error;
use crate::ids::{
    block_id_from_sqlite, block_id_to_sqlite, document_id_from_sqlite, document_id_to_sqlite,
};
use crate::layout::save_block_layouts;
use crate::page_layout::save_page_layout_snapshot;
use crate::payload::insert_payload;
use crate::snapshot::save_index_snapshot;
use crate::util::{
    checked_i64, checked_u16, checked_u32, checked_u64, row_version, sort_key, unix_millis,
};
use crate::writer::SqliteWriterGate;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub struct SqliteDocumentStorage {
    pub(crate) pool: SqlitePool,
    options: SqliteStorageOptions,
    writer: SqliteWriterGate,
}

impl SqliteDocumentStorage {
    pub async fn open(options: SqliteStorageOptions) -> StorageResult<Self> {
        prepare_path(&options)?;
        let writer = SqliteWriterGate::for_path(&options.path, options.busy_timeout)?;
        let _writer_guard = writer.acquire().await?;
        let synchronous = match options.durability {
            SqliteDurability::Full => SqliteSynchronous::Full,
            SqliteDurability::Balanced => SqliteSynchronous::Normal,
        };
        let connect = SqliteConnectOptions::new()
            .filename(&options.path)
            .create_if_missing(options.create_if_missing)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(synchronous)
            .busy_timeout(options.busy_timeout);
        let pool = SqlitePoolOptions::new()
            .max_connections(options.max_connections)
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys = ON")
                        .execute(&mut *connection)
                        .await?;
                    sqlx::query("PRAGMA temp_store = MEMORY")
                        .execute(&mut *connection)
                        .await?;
                    sqlx::query("PRAGMA wal_autocheckpoint = 1000")
                        .execute(&mut *connection)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(connect)
            .await
            .map_err(sqlite_error)?;
        MIGRATOR
            .run(&pool)
            .await
            .map_err(|error| StorageError::Migration {
                backend: StorageBackendKind::Sqlite,
                message: error.to_string(),
            })?;
        Ok(Self {
            pool,
            options,
            writer,
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn options(&self) -> &SqliteStorageOptions {
        &self.options
    }

    async fn ensure_minimal_document(&self, request: &LoadDocumentRequest) -> StorageResult<()> {
        let _writer_guard = self.writer.acquire().await?;
        let now = unix_millis()?;
        let workspace_id = Uuid::from_u128(request.workspace_id as u128);
        let document_id = document_id_to_sqlite(request.document_id);
        let block_id = block_id_to_sqlite(1);
        let mut transaction = self.pool.begin().await.map_err(sqlite_error)?;

        sqlx::query(
            "INSERT OR IGNORE INTO workspaces (id, name, created_at, updated_at) VALUES (?, 'Default Workspace', ?, ?)",
        )
        .bind(workspace_id)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(sqlite_error)?;
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO documents (
                id, workspace_id, title, structure_version, content_version,
                layout_version, schema_version, created_at, updated_at
            ) VALUES (?, ?, 'Untitled', 1, 1, 0, 1, ?, ?)
            "#,
        )
        .bind(document_id)
        .bind(workspace_id)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(sqlite_error)?;

        let block_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM blocks WHERE document_id = ? AND deleted_at IS NULL",
        )
        .bind(document_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(sqlite_error)?;
        if block_count == 0 {
            let kind = RichBlockKind::Paragraph;
            let payload = BlockPayloadRecord::rich_text(1, kind.clone(), "");
            sqlx::query(
                r#"
                INSERT INTO blocks (
                    id, document_id, parent_id, sort_key, depth, kind_tag, flags,
                    content_version, structure_version, updated_at
                ) VALUES (?, ?, NULL, ?, 0, ?, 0, 1, 1, ?)
                "#,
            )
            .bind(block_id)
            .bind(document_id)
            .bind(sort_key(0))
            .bind(i64::from(kind_tag_for_rich_block_kind(&kind)))
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(sqlite_error)?;
            insert_payload(&mut transaction, document_id, &payload, now).await?;
        }
        transaction.commit().await.map_err(sqlite_error)
    }

    async fn load_metadata(
        &self,
        document_id: cditor_core::ids::DocumentId,
    ) -> StorageResult<StorageDocumentMetadata> {
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, title, structure_version, content_version,
                   layout_version, schema_version
            FROM documents
            WHERE id = ? AND deleted_at IS NULL
            "#,
        )
        .bind(document_id_to_sqlite(document_id))
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?
        .ok_or_else(|| StorageError::NotFound {
            entity: "document",
            id: document_id.to_string(),
        })?;
        let stored_id: Uuid = row.try_get("id").map_err(sqlite_error)?;
        let workspace_id: Uuid = row.try_get("workspace_id").map_err(sqlite_error)?;
        Ok(StorageDocumentMetadata {
            document_id: document_id_from_sqlite(stored_id).ok_or_else(|| {
                StorageError::CorruptData(format!(
                    "document id {stored_id} is outside runtime namespace"
                ))
            })?,
            workspace_id: u64::try_from(workspace_id.as_u128()).map_err(|_| {
                StorageError::CorruptData(format!(
                    "workspace id {workspace_id} is outside runtime namespace"
                ))
            })?,
            title: row.try_get("title").map_err(sqlite_error)?,
            structure_version: row_version(&row, "structure_version")?,
            content_version: row_version(&row, "content_version")?,
            layout_version: row_version(&row, "layout_version")?,
            schema_version: row_version(&row, "schema_version")?,
        })
    }

    async fn load_records(
        &self,
        document_id: cditor_core::ids::DocumentId,
    ) -> StorageResult<Vec<BlockIndexRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, parent_id, depth, kind_tag, flags, estimated_height,
                   measured_height, width_bucket, layout_version, layout_dirty
            FROM blocks
            WHERE document_id = ? AND deleted_at IS NULL
            ORDER BY sort_key
            "#,
        )
        .bind(document_id_to_sqlite(document_id))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_error)?;
        rows.into_iter()
            .map(|row| {
                let stored_id: Uuid = row.try_get("id").map_err(sqlite_error)?;
                let block_id = block_id_from_sqlite(stored_id).ok_or_else(|| {
                    StorageError::CorruptData(format!(
                        "block id {stored_id} is outside runtime namespace"
                    ))
                })?;
                let parent_id = row
                    .try_get::<Option<Uuid>, _>("parent_id")
                    .map_err(sqlite_error)?
                    .map(|id| {
                        block_id_from_sqlite(id).ok_or_else(|| {
                            StorageError::CorruptData(format!(
                                "parent block id {id} is outside runtime namespace"
                            ))
                        })
                    })
                    .transpose()?;
                Ok(BlockIndexRecord {
                    id: block_id,
                    parent_id,
                    depth: checked_u16(row.try_get("depth").map_err(sqlite_error)?, "depth")?,
                    kind_tag: checked_u16(
                        row.try_get("kind_tag").map_err(sqlite_error)?,
                        "kind_tag",
                    )?,
                    flags: checked_u32(row.try_get("flags").map_err(sqlite_error)?, "flags")?,
                    layout_meta: BlockLayoutMeta {
                        block_id,
                        estimated_height: row.try_get("estimated_height").map_err(sqlite_error)?,
                        measured_height: row.try_get("measured_height").map_err(sqlite_error)?,
                        width_bucket: checked_u16(
                            row.try_get("width_bucket").map_err(sqlite_error)?,
                            "width_bucket",
                        )?,
                        layout_version: checked_u64(
                            row.try_get("layout_version").map_err(sqlite_error)?,
                            "layout_version",
                        )?,
                        dirty: row
                            .try_get::<i64, _>("layout_dirty")
                            .map_err(sqlite_error)?
                            != 0,
                    },
                })
            })
            .collect()
    }

    async fn load_attrs(
        &self,
        document_id: cditor_core::ids::DocumentId,
    ) -> StorageResult<
        Vec<(
            cditor_core::ids::BlockId,
            cditor_core::rich_text::BlockAttrs,
        )>,
    > {
        let rows = sqlx::query(
            r#"
            SELECT a.block_id, a.attrs_json
            FROM block_attrs a
            INNER JOIN blocks b
                ON b.document_id = a.document_id AND b.id = a.block_id
            WHERE b.document_id = ? AND b.deleted_at IS NULL
            "#,
        )
        .bind(document_id_to_sqlite(document_id))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_error)?;
        rows.into_iter()
            .map(|row| {
                let id: Uuid = row.try_get("block_id").map_err(sqlite_error)?;
                let id = block_id_from_sqlite(id).ok_or_else(|| {
                    StorageError::CorruptData("block attrs id is outside runtime namespace".into())
                })?;
                let json: String = row.try_get("attrs_json").map_err(sqlite_error)?;
                Ok((id, decode_attrs(&json)?))
            })
            .collect()
    }
}

#[async_trait]
impl DocumentStorage for SqliteDocumentStorage {
    fn backend_kind(&self) -> StorageBackendKind {
        StorageBackendKind::Sqlite
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::SQLITE
    }

    async fn load_document(&self, request: LoadDocumentRequest) -> StorageResult<LoadedDocument> {
        self.ensure_minimal_document(&request).await?;
        let metadata = self.load_metadata(request.document_id).await?;
        let snapshot = self
            .load_index_snapshot(
                request.document_id,
                request.visible_index_version,
                metadata.structure_version,
            )
            .await?;
        let (mut records, index_from_snapshot) = match snapshot {
            Some(records) => (records, true),
            None => (self.load_records(request.document_id).await?, false),
        };
        let layout_cache_hits = self
            .apply_block_layout_cache(request.document_id, &mut records, request.layout_key)
            .await?;
        let page_layout_snapshot = self
            .load_page_layout_snapshot(
                request.document_id,
                request.visible_index_version,
                metadata.structure_version,
                request.layout_key,
                request.page_policy_version,
            )
            .await?;
        let block_attrs = self.load_attrs(request.document_id).await?;
        let initial_payload_window_end = records.len().min(request.initial_payload_window_blocks);
        let loaded = self
            .load_payloads_inner(
                request.document_id,
                &records
                    .iter()
                    .take(initial_payload_window_end)
                    .map(|record| record.id)
                    .collect::<Vec<_>>(),
            )
            .await?;
        if !loaded.missing_block_ids.is_empty() {
            return Err(StorageError::CorruptData(format!(
                "document {} is missing {} payloads in its initial window",
                request.document_id,
                loaded.missing_block_ids.len()
            )));
        }
        Ok(LoadedDocument {
            metadata,
            records,
            block_attrs,
            initial_payloads: loaded.records,
            initial_payload_window_end,
            index_from_snapshot,
            layout_cache_hits,
            page_layout_snapshot,
        })
    }

    async fn load_payloads(
        &self,
        document_id: cditor_core::ids::DocumentId,
        block_ids: &[cditor_core::ids::BlockId],
    ) -> StorageResult<LoadedPayloadBatch> {
        self.load_payloads_inner(document_id, block_ids).await
    }

    async fn commit(&self, batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
        let _writer_guard = self.writer.acquire().await?;
        let now = unix_millis()?;
        let document_id = document_id_to_sqlite(batch.document_id);
        let saved_structure_version = batch.saved_structure_version();
        let saved_payload_versions = batch
            .payloads
            .iter()
            .map(|payload| (payload.block_id, payload.content_version))
            .collect();
        let structure_version = checked_i64(batch.structure_version)?;
        let mut transaction = self.pool.begin().await.map_err(sqlite_error)?;

        if let Some(snapshot) = &batch.page_layout_snapshot {
            validate_page_layout_batch(&batch, snapshot)?;
        }

        if !batch.index_records.is_empty() {
            sqlx::query("UPDATE blocks SET deleted_at = ? WHERE document_id = ?")
                .bind(now)
                .bind(document_id)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            for (index, record) in batch.index_records.iter().enumerate() {
                sqlx::query(
                    r#"
                    INSERT INTO blocks (
                        id, document_id, parent_id, sort_key, depth, kind_tag, flags,
                        content_version, structure_version, estimated_height, measured_height,
                        width_bucket, layout_version, layout_dirty, updated_at, deleted_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, NULL)
                    ON CONFLICT(document_id, id) DO UPDATE SET
                        document_id = excluded.document_id,
                        parent_id = excluded.parent_id,
                        sort_key = excluded.sort_key,
                        depth = excluded.depth,
                        kind_tag = excluded.kind_tag,
                        flags = excluded.flags,
                        structure_version = excluded.structure_version,
                        estimated_height = excluded.estimated_height,
                        measured_height = excluded.measured_height,
                        width_bucket = excluded.width_bucket,
                        layout_version = excluded.layout_version,
                        layout_dirty = excluded.layout_dirty,
                        updated_at = excluded.updated_at,
                        deleted_at = NULL
                    "#,
                )
                .bind(block_id_to_sqlite(record.id))
                .bind(document_id)
                .bind(record.parent_id.map(block_id_to_sqlite))
                .bind(sort_key(index))
                .bind(i64::from(record.depth))
                .bind(i64::from(record.kind_tag))
                .bind(i64::from(record.flags))
                .bind(structure_version)
                .bind(record.layout_meta.estimated_height)
                .bind(record.layout_meta.measured_height)
                .bind(i64::from(record.layout_meta.width_bucket))
                .bind(checked_i64(record.layout_meta.layout_version)?)
                .bind(i64::from(record.layout_meta.dirty))
                .bind(now)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            }
            sqlx::query("UPDATE documents SET structure_version = ?, updated_at = ? WHERE id = ?")
                .bind(structure_version)
                .bind(now)
                .bind(document_id)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            save_index_snapshot(
                &mut transaction,
                document_id,
                cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION,
                batch.structure_version,
                &batch.index_records,
                now,
            )
            .await?;
            if let Some(layout_key) = batch.layout_key {
                save_block_layouts(
                    &mut transaction,
                    document_id,
                    &batch.index_records,
                    layout_key,
                    now,
                )
                .await?;
                if let Some(layout_version) = batch
                    .index_records
                    .iter()
                    .map(|record| record.layout_meta.layout_version)
                    .max()
                {
                    sqlx::query(
                        "UPDATE documents SET layout_version = max(layout_version, ?), updated_at = ? WHERE id = ?",
                    )
                    .bind(checked_i64(layout_version)?)
                    .bind(now)
                    .bind(document_id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(sqlite_error)?;
                }
            }
        }

        for (block_id, attrs) in &batch.block_attrs {
            sqlx::query(
                r#"
                INSERT INTO block_attrs (document_id, block_id, attrs_json, updated_at)
                VALUES (?, ?, ?, ?)
                ON CONFLICT(document_id, block_id) DO UPDATE SET
                    attrs_json = excluded.attrs_json,
                    updated_at = excluded.updated_at
                "#,
            )
            .bind(document_id)
            .bind(block_id_to_sqlite(*block_id))
            .bind(encode_attrs(attrs)?)
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(sqlite_error)?;
        }
        if let Some(snapshot) = &batch.page_layout_snapshot {
            save_page_layout_snapshot(&mut transaction, document_id, snapshot, now).await?;
        }
        for payload in &batch.payloads {
            insert_payload(&mut transaction, document_id, payload, now).await?;
        }
        if let Some(max_content_version) = batch
            .payloads
            .iter()
            .map(|payload| payload.content_version)
            .max()
        {
            sqlx::query(
                "UPDATE documents SET content_version = max(content_version, ?), updated_at = ? WHERE id = ?",
            )
            .bind(checked_i64(max_content_version)?)
            .bind(now)
            .bind(document_id)
            .execute(&mut *transaction)
            .await
            .map_err(sqlite_error)?;
        }
        for edit in &batch.transactions {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO edit_transactions (
                    document_id, transaction_id, transaction_json, structure_version, created_at
                ) VALUES (?, ?, ?, ?, ?)
                "#,
            )
            .bind(document_id)
            .bind(edit.id.to_string())
            .bind(encode_transaction(edit)?)
            .bind(structure_version)
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(sqlite_error)?;
        }

        transaction.commit().await.map_err(sqlite_error)?;
        Ok(StorageSaveOutcome {
            saved_structure_version,
            saved_payload_versions,
        })
    }

    async fn flush(&self) -> StorageResult<()> {
        let _writer_guard = self.writer.acquire().await?;
        sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
            .execute(&self.pool)
            .await
            .map_err(sqlite_error)?;
        Ok(())
    }
}

fn validate_page_layout_batch(
    batch: &StorageSaveBatch,
    snapshot: &cditor_storage::StoragePageLayoutSnapshot,
) -> StorageResult<()> {
    if snapshot.visible_index_version < 0 {
        return Err(StorageError::CorruptData(
            "page layout visible index version cannot be negative".to_owned(),
        ));
    }
    if snapshot.structure_version != batch.structure_version {
        return Err(StorageError::CorruptData(format!(
            "page layout structure version {} does not match save batch {}",
            snapshot.structure_version, batch.structure_version
        )));
    }
    if batch
        .layout_key
        .is_none_or(|key| key.hash_key() != snapshot.layout_key_hash)
    {
        return Err(StorageError::CorruptData(
            "page layout key does not match save batch layout key".to_owned(),
        ));
    }
    Ok(())
}

fn prepare_path(options: &SqliteStorageOptions) -> StorageResult<()> {
    if options.path.as_os_str().is_empty() {
        return Err(StorageError::InvalidConfiguration(
            "SQLite database path cannot be empty".to_owned(),
        ));
    }
    if !options.create_if_missing && !options.path.exists() {
        return Err(StorageError::InvalidConfiguration(format!(
            "SQLite database does not exist: {}",
            options.path.display()
        )));
    }
    if !(1..=8).contains(&options.max_connections) {
        return Err(StorageError::InvalidConfiguration(format!(
            "SQLite max_connections must be between 1 and 8, got {}",
            options.max_connections
        )));
    }
    if let Some(parent) = options
        .path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| StorageError::Io(error.to_string()))?;
    }
    Ok(())
}
