use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::{EditTransaction, ExternalUndoBlobRef};
use cditor_core::layout::BlockLayoutMeta;
use cditor_storage::query_index::{
    BacklinkRecord, FtsApplyResult, LocalIndexRebuildRequest, LocalSearchHit, LocalSearchRequest,
};
use cditor_storage::{
    AssetManifestRecord, AssetReference, AssetUploadMutation, DocumentLoadProgress,
    DocumentStorage, EmergencyLogAppendOutcome, EmergencyLogEntry, LoadDocumentRequest,
    LoadedDocument, LoadedPayloadBatch, MaterializedCheckpoint, MaterializedRebuildPlan,
    ProvisionalAssetRequest, StorageBackendKind, StorageCapabilities, StorageDocumentMetadata,
    StorageError, StorageResult, StorageSaveBatch, StorageSaveOutcome,
};

mod document_load;

use crate::asset_manifest::materialize_asset_operations;
use crate::codec::{decode_attrs, encode_attrs, encode_transaction};
use crate::config::{SqliteStorageOptions, prepare_path};
use crate::error::sqlite_error;
use crate::ids::{
    block_id_from_sqlite, block_id_to_sqlite, document_id_from_sqlite, document_id_to_sqlite,
};
use crate::journal::materialize_transaction_in_journal;
use crate::layout::save_block_layouts;
use crate::migration::{MIGRATOR, SqliteMigrationManager, connect_pool};
use crate::page_layout::{save_page_layout_snapshot, validate_page_layout_batch};
use crate::payload::insert_payload;
use crate::query_index::{prune_deleted_query_projection, update_query_projection_batch};
use crate::snapshot::save_index_snapshot;
use crate::util::{
    checked_i64, checked_u16, checked_u32, checked_u64, row_version, sort_key, unix_millis,
};
use crate::writer::SqliteWriterGate;

#[derive(Debug, Clone)]
pub struct SqliteDocumentStorage {
    pub(crate) pool: SqlitePool,
    options: SqliteStorageOptions,
    writer: SqliteWriterGate,
    last_migration_report: Option<crate::migration::SqliteMigrationReport>,
}

impl SqliteDocumentStorage {
    pub(crate) fn from_recovery_pool(pool: SqlitePool, options: SqliteStorageOptions) -> Self {
        let writer = SqliteWriterGate::for_path(&options.path, options.busy_timeout)
            .expect("validated recovery path must have a writer identity");
        Self {
            pool,
            options,
            writer,
            last_migration_report: None,
        }
    }

    pub(crate) fn writer_gate(&self) -> &SqliteWriterGate {
        &self.writer
    }

    pub async fn open(options: SqliteStorageOptions) -> StorageResult<Self> {
        prepare_path(&options)?;
        let writer = SqliteWriterGate::for_path(&options.path, options.busy_timeout)?;
        let _writer_guard = writer.acquire().await?;
        let last_migration_report = SqliteMigrationManager::migrate_if_needed(&options).await?;
        let pool = connect_pool(&options, options.create_if_missing).await?;
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
            last_migration_report,
        })
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn test_pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn options(&self) -> &SqliteStorageOptions {
        &self.options
    }

    /// 本次 `open` 实际执行 migration 时生成的 dry-run/备份/校验报告。
    pub fn last_migration_report(&self) -> Option<&crate::migration::SqliteMigrationReport> {
        self.last_migration_report.as_ref()
    }

    pub(crate) async fn load_metadata(
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

    pub(crate) async fn load_records(
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

    pub(crate) async fn load_attrs(
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

    async fn write_undo_blob(
        &self,
        document_id: cditor_core::ids::DocumentId,
        snapshot_id: u64,
        block_count: usize,
        transaction: &EditTransaction,
    ) -> StorageResult<ExternalUndoBlobRef> {
        SqliteDocumentStorage::write_undo_blob(
            self,
            document_id,
            snapshot_id,
            block_count,
            transaction,
        )
        .await
    }

    async fn append_emergency_transactions(
        &self,
        document_id: cditor_core::ids::DocumentId,
        transactions: &[EditTransaction],
    ) -> StorageResult<EmergencyLogAppendOutcome> {
        self.append_emergency_transaction_batch(document_id, transactions)
            .await
    }

    async fn load_emergency_transactions(
        &self,
        document_id: cditor_core::ids::DocumentId,
    ) -> StorageResult<Vec<EmergencyLogEntry>> {
        self.load_unmaterialized_emergency_transactions(document_id)
            .await
    }

    async fn acknowledge_emergency_transactions(
        &self,
        document_id: cditor_core::ids::DocumentId,
        through_sequence: u64,
    ) -> StorageResult<u64> {
        self.acknowledge_emergency_transaction_batch(document_id, through_sequence)
            .await
    }

    async fn create_materialized_checkpoint(
        &self,
        document_id: cditor_core::ids::DocumentId,
    ) -> StorageResult<MaterializedCheckpoint> {
        SqliteDocumentStorage::create_materialized_checkpoint(self, document_id).await
    }

    async fn load_materialized_rebuild_plan(
        &self,
        document_id: cditor_core::ids::DocumentId,
    ) -> StorageResult<Option<MaterializedRebuildPlan>> {
        SqliteDocumentStorage::load_materialized_rebuild_plan(self, document_id).await
    }

    async fn search_local(
        &self,
        request: LocalSearchRequest,
    ) -> StorageResult<Vec<LocalSearchHit>> {
        SqliteDocumentStorage::search_local(self, request).await
    }

    async fn backlinks(
        &self,
        target_document_id: cditor_core::ids::DocumentId,
        target_block_id: Option<cditor_core::ids::BlockId>,
        limit: usize,
    ) -> StorageResult<Vec<BacklinkRecord>> {
        SqliteDocumentStorage::backlinks(self, target_document_id, target_block_id, limit).await
    }

    async fn rebuild_local_query_index(
        &self,
        request: LocalIndexRebuildRequest,
    ) -> StorageResult<FtsApplyResult> {
        SqliteDocumentStorage::rebuild_local_query_index(self, request).await
    }

    async fn create_provisional_asset(
        &self,
        request: ProvisionalAssetRequest,
    ) -> StorageResult<AssetManifestRecord> {
        SqliteDocumentStorage::create_provisional_asset(self, request).await
    }

    async fn asset_manifest(
        &self,
        asset_id: cditor_core::ids::AssetId,
    ) -> StorageResult<Option<AssetManifestRecord>> {
        SqliteDocumentStorage::asset_manifest(self, asset_id).await
    }

    async fn update_asset_upload(
        &self,
        asset_id: cditor_core::ids::AssetId,
        mutation: AssetUploadMutation,
    ) -> StorageResult<AssetManifestRecord> {
        SqliteDocumentStorage::update_asset_upload(self, asset_id, mutation).await
    }

    async fn pending_asset_uploads(
        &self,
        workspace_id: u64,
        limit: usize,
    ) -> StorageResult<Vec<AssetManifestRecord>> {
        SqliteDocumentStorage::pending_asset_uploads(self, workspace_id, limit).await
    }

    async fn asset_references(
        &self,
        asset_id: cditor_core::ids::AssetId,
    ) -> StorageResult<Vec<AssetReference>> {
        SqliteDocumentStorage::asset_references(self, asset_id).await
    }

    async fn load_undo_blob(
        &self,
        document_id: cditor_core::ids::DocumentId,
        reference: &ExternalUndoBlobRef,
    ) -> StorageResult<EditTransaction> {
        SqliteDocumentStorage::load_undo_blob(self, document_id, reference).await
    }

    async fn delete_undo_blob(
        &self,
        document_id: cditor_core::ids::DocumentId,
        snapshot_id: u64,
    ) -> StorageResult<bool> {
        SqliteDocumentStorage::delete_undo_blob(self, document_id, snapshot_id).await
    }

    async fn prune_undo_blobs(
        &self,
        document_id: cditor_core::ids::DocumentId,
        keep_recent: usize,
    ) -> StorageResult<u64> {
        SqliteDocumentStorage::prune_undo_blobs(self, document_id, keep_recent).await
    }

    async fn load_document(&self, request: LoadDocumentRequest) -> StorageResult<LoadedDocument> {
        self.load_document_inner(request, &mut |_| {}).await
    }

    async fn load_document_with_progress(
        &self,
        request: LoadDocumentRequest,
        progress: &mut (dyn FnMut(DocumentLoadProgress) + Send),
    ) -> StorageResult<LoadedDocument> {
        self.load_document_inner(request, progress).await
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
        #[cfg(test)]
        crate::fault_injection::pause_at_commit_point("transaction_opened");

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
        update_query_projection_batch(&mut transaction, batch.document_id, &batch.payloads).await?;
        if !batch.index_records.is_empty() {
            prune_deleted_query_projection(&mut transaction, batch.document_id).await?;
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
        #[cfg(test)]
        crate::fault_injection::pause_at_commit_point("materialized_written");
        for edit in &batch.transactions {
            materialize_transaction_in_journal(&mut transaction, batch.document_id, edit, now)
                .await?;
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
        materialize_asset_operations(
            &mut transaction,
            batch.document_id,
            &batch.transactions,
            now,
        )
        .await?;
        #[cfg(test)]
        crate::fault_injection::pause_at_commit_point("journal_outbox_written");

        transaction.commit().await.map_err(sqlite_error)?;
        #[cfg(test)]
        crate::fault_injection::pause_at_commit_point("sqlite_commit_returned");
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
