use async_trait::async_trait;
use sqlx::PgPool;
use sqlx::types::Uuid;

use cditor_core::document::BlockIndexRecord;
use cditor_core::layout::{BlockLayoutMeta, PageLayout, PageLayoutIndex, PagePolicy};
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
use cditor_storage::layout_cache::CacheSource;
use cditor_storage::{
    DocumentStorage, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch, StorageBackendKind,
    StorageCapabilities, StorageDocumentMetadata, StorageError, StoragePageLayoutPage,
    StoragePageLayoutSnapshot, StorageResult, StorageSaveBatch, StorageSaveOutcome,
};

use crate::error::PostgresStorageError;
use crate::types::{
    DocumentRow, PgDocumentId, pg_document_id_from_runtime, runtime_document_id_from_pg,
};
use crate::{
    EditTransactionVersions, PostgresDocumentStore, PostgresLayoutCacheStore, PostgresPayloadStore,
    PostgresPoolConfig, PostgresTransactionStore, create_pg_pool, run_migrations,
};

#[derive(Debug, Clone)]
pub struct PostgresDocumentStorage {
    pool: PgPool,
    document_store: PostgresDocumentStore,
    payload_store: PostgresPayloadStore,
    layout_store: PostgresLayoutCacheStore,
}

impl PostgresDocumentStorage {
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            document_store: PostgresDocumentStore::new(pool.clone()),
            payload_store: PostgresPayloadStore::new(pool.clone()),
            layout_store: PostgresLayoutCacheStore::new(pool.clone()),
            pool,
        }
    }

    pub async fn from_url(url: impl Into<String>) -> StorageResult<Self> {
        let pool = create_pg_pool(&PostgresPoolConfig::new(url))
            .await
            .map_err(storage_error)?;
        run_migrations(&pool).await.map_err(storage_error)?;
        Ok(Self::from_pool(pool))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn document_store(&self) -> &PostgresDocumentStore {
        &self.document_store
    }

    pub fn payload_store(&self) -> &PostgresPayloadStore {
        &self.payload_store
    }

    async fn ensure_minimal_document(&self, request: &LoadDocumentRequest) -> StorageResult<()> {
        let document_id = pg_document_id_from_runtime(request.document_id);
        match self
            .document_store
            .load_document_metadata(document_id)
            .await
        {
            Ok(_) => return Ok(()),
            Err(PostgresStorageError::NotFound { .. }) => {}
            Err(error) => return Err(storage_error(error)),
        }

        self.document_store
            .save_document_metadata(&DocumentRow {
                id: document_id,
                workspace_id: Uuid::from_u128(request.workspace_id as u128),
                title: "Untitled".to_owned(),
                structure_version: 1,
                content_version: 1,
                layout_version: 0,
                schema_version: 1,
            })
            .await
            .map_err(storage_error)?;
        let records = vec![BlockIndexRecord::new(
            1,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
            0,
        )];
        self.document_store
            .save_block_index_records(document_id, &records, 1)
            .await
            .map_err(storage_error)?;
        self.payload_store
            .save_block_payloads(
                document_id,
                &[BlockPayloadRecord::rich_text(
                    1,
                    RichBlockKind::Paragraph,
                    "",
                )],
            )
            .await
            .map_err(storage_error)
    }

    async fn load_document_inner(
        &self,
        request: LoadDocumentRequest,
    ) -> StorageResult<LoadedDocument> {
        self.ensure_minimal_document(&request).await?;
        let document_id = pg_document_id_from_runtime(request.document_id);
        let metadata = self
            .document_store
            .load_document_metadata(document_id)
            .await
            .map_err(storage_error)?;
        let block_attrs = self
            .document_store
            .load_block_attrs(document_id)
            .await
            .map_err(storage_error)?;
        let runtime_document_id = runtime_document_id_from_pg(metadata.id).ok_or_else(|| {
            StorageError::CorruptData(format!(
                "document id {} is outside runtime namespace",
                metadata.id
            ))
        })?;
        let structure_version = version(metadata.structure_version)?;

        let snapshot_records = self
            .document_store
            .load_document_index_snapshot(
                document_id,
                request.visible_index_version,
                metadata.structure_version,
            )
            .await
            .map_err(storage_error)?;
        let (mut records, index_from_snapshot) = match snapshot_records {
            Some(records) => (records, true),
            None => (
                self.document_store
                    .load_block_index_records(document_id)
                    .await
                    .map_err(storage_error)?,
                false,
            ),
        };

        let cached_heights = self
            .layout_store
            .load_block_heights(
                &records.iter().map(|record| record.id).collect::<Vec<_>>(),
                request.layout_key,
            )
            .await
            .map_err(storage_error)?;
        let mut layout_cache_hits = 0usize;
        for record in &mut records {
            if let Some(cached) = cached_heights.get(&record.id) {
                layout_cache_hits += 1;
                record.layout_meta = BlockLayoutMeta {
                    block_id: record.id,
                    estimated_height: cached.height,
                    measured_height: (cached.source == CacheSource::ExactMatch)
                        .then_some(cached.height),
                    width_bucket: request.layout_key.width_bucket,
                    layout_version: request.layout_key.content_version,
                    dirty: cached.source != CacheSource::ExactMatch,
                };
            }
        }

        let visible_index_version = u64::try_from(request.visible_index_version).map_err(|_| {
            StorageError::CorruptData(format!(
                "negative visible index version {}",
                request.visible_index_version
            ))
        })?;
        let page_layout_snapshot = match self
            .layout_store
            .load_page_layout_rows(
                request.document_id,
                visible_index_version,
                structure_version,
                &request.layout_key.hash_key(),
                request.page_policy_version,
            )
            .await
        {
            Ok(rows) => page_layout_snapshot_from_rows(
                rows,
                request.visible_index_version,
                structure_version,
                request.layout_key.hash_key(),
                request.page_policy_version,
            ),
            Err(PostgresStorageError::CorruptData { .. }) => None,
            Err(error) => return Err(storage_error(error)),
        };

        let initial_payload_window_end = records.len().min(request.initial_payload_window_blocks);
        let loaded = self
            .payload_store
            .load_block_payloads(
                &records
                    .iter()
                    .take(initial_payload_window_end)
                    .map(|record| record.id)
                    .collect::<Vec<_>>(),
            )
            .await
            .map_err(storage_error)?;
        reject_missing_payloads(document_id, &loaded.missing_block_ids)?;

        Ok(LoadedDocument {
            metadata: StorageDocumentMetadata {
                document_id: runtime_document_id,
                workspace_id: runtime_workspace_id(metadata.workspace_id)?,
                title: metadata.title,
                structure_version,
                content_version: version(metadata.content_version)?,
                layout_version: version(metadata.layout_version)?,
                schema_version: version(metadata.schema_version)?,
            },
            records,
            block_attrs,
            initial_payloads: loaded.records,
            initial_payload_window_end,
            index_from_snapshot,
            layout_cache_hits,
            page_layout_snapshot,
        })
    }
}

#[async_trait]
impl DocumentStorage for PostgresDocumentStorage {
    fn backend_kind(&self) -> StorageBackendKind {
        StorageBackendKind::Postgres
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::POSTGRES
    }

    async fn load_document(&self, request: LoadDocumentRequest) -> StorageResult<LoadedDocument> {
        self.load_document_inner(request).await
    }

    async fn load_payloads(
        &self,
        _document_id: cditor_core::ids::DocumentId,
        block_ids: &[cditor_core::ids::BlockId],
    ) -> StorageResult<LoadedPayloadBatch> {
        let loaded = self
            .payload_store
            .load_block_payloads(block_ids)
            .await
            .map_err(storage_error)?;
        Ok(LoadedPayloadBatch {
            records: loaded.records,
            missing_block_ids: loaded.missing_block_ids,
        })
    }

    async fn commit(&self, batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
        let document_id = pg_document_id_from_runtime(batch.document_id);
        let saved_structure_version = batch.saved_structure_version();
        let saved_payload_versions = batch
            .payloads
            .iter()
            .map(|payload| (payload.block_id, payload.content_version))
            .collect();
        let structure_version = i64::try_from(batch.structure_version).map_err(|_| {
            StorageError::VersionOutOfRange {
                value: batch.structure_version,
            }
        })?;
        let content_version_after = batch
            .payloads
            .iter()
            .map(|payload| payload.content_version)
            .max()
            .and_then(|version| i64::try_from(version).ok());
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(PostgresStorageError::from)
            .map_err(storage_error)?;

        if let Some(snapshot) = &batch.page_layout_snapshot {
            validate_page_layout_batch(&batch, snapshot)?;
        }

        if !batch.index_records.is_empty() {
            self.document_store
                .save_block_index_records_tx(
                    &mut tx,
                    document_id,
                    &batch.index_records,
                    structure_version,
                )
                .await
                .map_err(storage_error)?;
            self.document_store
                .save_document_index_snapshot_tx(
                    &mut tx,
                    document_id,
                    cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION,
                    structure_version,
                    &batch.index_records,
                )
                .await
                .map_err(storage_error)?;
        }
        self.document_store
            .save_block_attrs_tx(&mut tx, document_id, &batch.block_attrs)
            .await
            .map_err(storage_error)?;
        if let Some(snapshot) = &batch.page_layout_snapshot {
            self.layout_store
                .save_page_layout_snapshot_tx(&mut tx, document_id, snapshot)
                .await
                .map_err(storage_error)?;
        }
        if !batch.payloads.is_empty() {
            self.payload_store
                .save_block_payloads_tx(&mut tx, document_id, &batch.payloads)
                .await
                .map_err(storage_error)?;
        }

        let transaction_store = PostgresTransactionStore::new(self.pool.clone());
        for transaction in &batch.transactions {
            transaction_store
                .save_edit_transaction_tx(
                    &mut tx,
                    document_id,
                    transaction,
                    &EditTransactionVersions {
                        structure_version_before: structure_version.checked_sub(1),
                        structure_version_after: Some(structure_version),
                        content_version_after,
                    },
                )
                .await
                .map_err(storage_error)?;
        }
        tx.commit()
            .await
            .map_err(PostgresStorageError::from)
            .map_err(storage_error)?;

        Ok(StorageSaveOutcome {
            saved_structure_version,
            saved_payload_versions,
        })
    }
}

fn page_layout_snapshot_from_rows(
    rows: Vec<cditor_storage::layout_cache::PageLayoutRow>,
    visible_index_version: i64,
    structure_version: u64,
    layout_key_hash: String,
    page_policy_version: u64,
) -> Option<StoragePageLayoutSnapshot> {
    if rows.is_empty() {
        return None;
    }
    let pages = rows
        .into_iter()
        .map(|row| {
            Some(StoragePageLayoutPage {
                layout: PageLayout {
                    page_index: row.page_index,
                    block_start: row.block_start_index,
                    block_count: row.block_count,
                    height: row.height,
                    measured_ratio: row.measured_ratio as f32,
                    confidence: row.confidence,
                    max_error_hint: row.max_error_hint,
                    dirty: row.dirty,
                },
                first_block_id: row.first_block_id?,
                last_block_id: row.last_block_id?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let covered_blocks = pages
        .last()
        .and_then(|page| page.layout.block_start.checked_add(page.layout.block_count))?;
    PageLayoutIndex::from_cached_pages(
        pages.iter().map(|page| page.layout).collect(),
        PagePolicy::default(),
        covered_blocks,
    )
    .ok()?;
    Some(StoragePageLayoutSnapshot {
        visible_index_version,
        structure_version,
        layout_key_hash,
        page_policy_version,
        pages,
    })
}

fn validate_page_layout_batch(
    batch: &StorageSaveBatch,
    snapshot: &StoragePageLayoutSnapshot,
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

fn version(value: i64) -> StorageResult<u64> {
    u64::try_from(value).map_err(|_| StorageError::CorruptData(format!("negative version {value}")))
}

fn runtime_workspace_id(value: Uuid) -> StorageResult<u64> {
    u64::try_from(value.as_u128()).map_err(|_| {
        StorageError::CorruptData(format!("workspace id {value} is outside runtime namespace"))
    })
}

fn reject_missing_payloads(
    document_id: PgDocumentId,
    missing: &[cditor_core::ids::BlockId],
) -> StorageResult<()> {
    if missing.is_empty() {
        return Ok(());
    }
    let sample = missing
        .iter()
        .take(5)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Err(StorageError::CorruptData(format!(
        "document {document_id} is missing {} payloads in its initial window (sample: {sample})",
        missing.len()
    )))
}

fn storage_error(error: PostgresStorageError) -> StorageError {
    match error {
        PostgresStorageError::Migration(message) => StorageError::Migration {
            backend: StorageBackendKind::Postgres,
            message,
        },
        PostgresStorageError::NotFound { entity, id } => StorageError::NotFound { entity, id },
        PostgresStorageError::CorruptData { message } => StorageError::CorruptData(message),
        PostgresStorageError::Serialization(error) => {
            StorageError::Serialization(error.to_string())
        }
        PostgresStorageError::Io(error) => StorageError::Io(error.to_string()),
        PostgresStorageError::Busy => StorageError::Busy {
            waited: std::time::Duration::ZERO,
        },
        PostgresStorageError::Conflict { message } => StorageError::Conflict(message),
        PostgresStorageError::Timeout { operation, timeout } => {
            StorageError::Timeout { operation, timeout }
        }
        other => StorageError::Backend {
            backend: StorageBackendKind::Postgres,
            message: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::layout::HeightConfidence;
    use cditor_storage::layout_cache::PageLayoutRow;

    use super::*;

    fn row(page_index: usize, block_start_index: usize) -> PageLayoutRow {
        PageLayoutRow {
            document_id: 10,
            visible_index_version: 2,
            structure_version: 7,
            layout_key_hash: "layout".to_owned(),
            page_policy_version: 1,
            page_index,
            block_start_index,
            block_count: 2,
            first_block_id: Some(block_start_index as u64 + 1),
            last_block_id: Some(block_start_index as u64 + 2),
            height: 80.0,
            measured_ratio: 1.0,
            confidence: HeightConfidence::Exact,
            max_error_hint: 0.0,
            dirty: false,
            updated_at: 1,
        }
    }

    #[test]
    fn postgres_rows_form_a_contiguous_backend_neutral_snapshot() {
        let snapshot = page_layout_snapshot_from_rows(
            vec![row(0, 0), row(1, 2)],
            2,
            7,
            "layout".to_owned(),
            1,
        )
        .unwrap();
        assert_eq!(snapshot.pages.len(), 2);
        assert_eq!(snapshot.pages[1].last_block_id, 4);
    }

    #[test]
    fn postgres_rows_with_a_gap_or_missing_boundary_are_a_cache_miss() {
        assert!(
            page_layout_snapshot_from_rows(
                vec![row(0, 0), row(1, 3)],
                2,
                7,
                "layout".to_owned(),
                1,
            )
            .is_none()
        );
        let mut missing_boundary = row(0, 0);
        missing_boundary.last_block_id = None;
        assert!(
            page_layout_snapshot_from_rows(vec![missing_boundary], 2, 7, "layout".to_owned(), 1,)
                .is_none()
        );
    }
}
