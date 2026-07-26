use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::{EditTransaction, ExternalUndoBlobRef};
use cditor_core::ids::AssetId;
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::rich_text::{BlockAttrs, BlockPayloadRecord};
use cditor_core::schema::VersionedEnvelope;

use crate::error::StorageResult;
use crate::layout_cache::LayoutCacheKey;
use crate::page_layout_snapshot::StoragePageLayoutSnapshot;
use crate::query_index::{
    BacklinkRecord, FtsApplyResult, LocalIndexRebuildRequest, LocalSearchHit, LocalSearchRequest,
};
use crate::{AssetManifestRecord, AssetReference, AssetUploadMutation, ProvisionalAssetRequest};
use crate::{MaterializedCheckpoint, MaterializedRebuildPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageBackendKind {
    Sqlite,
    Postgres,
    Custom,
}

impl fmt::Display for StorageBackendKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Sqlite => "SQLite",
            Self::Postgres => "PostgreSQL",
            Self::Custom => "custom",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageCapabilities {
    pub payload_window: bool,
    pub full_text_search: bool,
    pub cloud_sync: bool,
    pub server_authoritative: bool,
    pub emergency_log: bool,
}

impl StorageCapabilities {
    pub const SQLITE: Self = Self {
        payload_window: true,
        full_text_search: true,
        cloud_sync: false,
        server_authoritative: false,
        emergency_log: true,
    };

    pub const POSTGRES: Self = Self {
        payload_window: true,
        full_text_search: true,
        cloud_sync: false,
        server_authoritative: true,
        emergency_log: false,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencyLogEntry {
    pub sequence: u64,
    pub transaction_id: u64,
    pub envelope: VersionedEnvelope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmergencyLogAppendOutcome {
    pub appended: usize,
    pub through_sequence: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageDocumentMetadata {
    pub document_id: DocumentId,
    pub workspace_id: u64,
    pub title: String,
    pub structure_version: u64,
    pub content_version: u64,
    pub layout_version: u64,
    pub schema_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadDocumentRequest {
    pub document_id: DocumentId,
    pub workspace_id: u64,
    pub initial_payload_window_blocks: usize,
    pub visible_index_version: i64,
    pub layout_key: LayoutCacheKey,
    pub page_policy_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentLoadStage {
    EnsureDocument,
    Metadata,
    Structure,
    BlockLayout,
    PageLayout,
    Attributes,
    InitialPayloads,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentLoadProgress {
    pub stage: DocumentLoadStage,
    pub completed_units: usize,
    pub total_units: usize,
}

impl DocumentLoadProgress {
    pub fn percentage(self) -> u8 {
        if self.total_units == 0 {
            return 0;
        }
        ((self.completed_units.min(self.total_units) * 100) / self.total_units) as u8
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedDocument {
    pub metadata: StorageDocumentMetadata,
    pub records: Vec<BlockIndexRecord>,
    pub block_attrs: Vec<(BlockId, BlockAttrs)>,
    pub initial_payloads: Vec<BlockPayloadRecord>,
    pub initial_payload_window_end: usize,
    pub index_from_snapshot: bool,
    pub layout_cache_hits: usize,
    pub page_layout_snapshot: Option<StoragePageLayoutSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedPayloadBatch {
    pub records: Vec<BlockPayloadRecord>,
    pub missing_block_ids: Vec<BlockId>,
}

#[derive(Debug, Clone)]
pub struct StorageSaveBatch {
    pub document_id: DocumentId,
    pub layout_key: Option<LayoutCacheKey>,
    pub payloads: Vec<BlockPayloadRecord>,
    pub index_records: Vec<BlockIndexRecord>,
    pub structure_version: u64,
    pub transactions: Vec<EditTransaction>,
    pub block_attrs: Vec<(BlockId, BlockAttrs)>,
    pub page_layout_snapshot: Option<StoragePageLayoutSnapshot>,
}

impl StorageSaveBatch {
    pub fn saved_structure_version(&self) -> Option<u64> {
        (!self.index_records.is_empty()).then_some(self.structure_version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageSaveOutcome {
    pub saved_structure_version: Option<u64>,
    pub saved_payload_versions: Vec<(BlockId, u64)>,
}

#[async_trait]
pub trait DocumentStorage: Send + Sync {
    fn backend_kind(&self) -> StorageBackendKind;

    fn capabilities(&self) -> StorageCapabilities;

    async fn load_document(&self, request: LoadDocumentRequest) -> StorageResult<LoadedDocument>;

    async fn load_document_with_progress(
        &self,
        request: LoadDocumentRequest,
        progress: &mut (dyn FnMut(DocumentLoadProgress) + Send),
    ) -> StorageResult<LoadedDocument> {
        progress(DocumentLoadProgress {
            stage: DocumentLoadStage::EnsureDocument,
            completed_units: 0,
            total_units: 1,
        });
        let loaded = self.load_document(request).await?;
        progress(DocumentLoadProgress {
            stage: DocumentLoadStage::InitialPayloads,
            completed_units: 1,
            total_units: 1,
        });
        Ok(loaded)
    }

    async fn load_payloads(
        &self,
        document_id: DocumentId,
        block_ids: &[BlockId],
    ) -> StorageResult<LoadedPayloadBatch>;

    async fn write_undo_blob(
        &self,
        _document_id: DocumentId,
        _snapshot_id: u64,
        _block_count: usize,
        _transaction: &EditTransaction,
    ) -> StorageResult<ExternalUndoBlobRef> {
        Err(crate::error::StorageError::Backend {
            backend: self.backend_kind(),
            message: "external undo blobs are not supported by this backend".to_owned(),
        })
    }

    async fn load_undo_blob(
        &self,
        _document_id: DocumentId,
        _reference: &ExternalUndoBlobRef,
    ) -> StorageResult<EditTransaction> {
        Err(crate::error::StorageError::Backend {
            backend: self.backend_kind(),
            message: "external undo blobs are not supported by this backend".to_owned(),
        })
    }

    async fn delete_undo_blob(
        &self,
        _document_id: DocumentId,
        _snapshot_id: u64,
    ) -> StorageResult<bool> {
        Ok(false)
    }

    async fn prune_undo_blobs(
        &self,
        _document_id: DocumentId,
        _keep_recent: usize,
    ) -> StorageResult<u64> {
        Ok(0)
    }

    async fn append_emergency_transactions(
        &self,
        _document_id: DocumentId,
        _transactions: &[EditTransaction],
    ) -> StorageResult<EmergencyLogAppendOutcome> {
        Err(crate::error::StorageError::Backend {
            backend: self.backend_kind(),
            message: "durable emergency log is not supported by this backend".to_owned(),
        })
    }

    async fn load_emergency_transactions(
        &self,
        _document_id: DocumentId,
    ) -> StorageResult<Vec<EmergencyLogEntry>> {
        Ok(Vec::new())
    }

    async fn acknowledge_emergency_transactions(
        &self,
        _document_id: DocumentId,
        _through_sequence: u64,
    ) -> StorageResult<u64> {
        Ok(0)
    }

    async fn create_materialized_checkpoint(
        &self,
        _document_id: DocumentId,
    ) -> StorageResult<MaterializedCheckpoint> {
        Err(crate::error::StorageError::Backend {
            backend: self.backend_kind(),
            message: "materialized checkpoints are not supported by this backend".to_owned(),
        })
    }

    async fn load_materialized_rebuild_plan(
        &self,
        _document_id: DocumentId,
    ) -> StorageResult<Option<MaterializedRebuildPlan>> {
        Ok(None)
    }

    async fn search_local(
        &self,
        _request: LocalSearchRequest,
    ) -> StorageResult<Vec<LocalSearchHit>> {
        Err(crate::error::StorageError::Backend {
            backend: self.backend_kind(),
            message: "local full-text search is not supported by this backend".to_owned(),
        })
    }

    async fn backlinks(
        &self,
        _target_document_id: DocumentId,
        _target_block_id: Option<BlockId>,
        _limit: usize,
    ) -> StorageResult<Vec<BacklinkRecord>> {
        Ok(Vec::new())
    }

    async fn rebuild_local_query_index(
        &self,
        _request: LocalIndexRebuildRequest,
    ) -> StorageResult<FtsApplyResult> {
        Err(crate::error::StorageError::Backend {
            backend: self.backend_kind(),
            message: "local query-index rebuild is not supported by this backend".to_owned(),
        })
    }

    async fn create_provisional_asset(
        &self,
        _request: ProvisionalAssetRequest,
    ) -> StorageResult<AssetManifestRecord> {
        Err(crate::error::StorageError::Backend {
            backend: self.backend_kind(),
            message: "asset manifests are not supported by this backend".to_owned(),
        })
    }

    async fn asset_manifest(
        &self,
        _asset_id: AssetId,
    ) -> StorageResult<Option<AssetManifestRecord>> {
        Ok(None)
    }

    async fn update_asset_upload(
        &self,
        _asset_id: AssetId,
        _mutation: AssetUploadMutation,
    ) -> StorageResult<AssetManifestRecord> {
        Err(crate::error::StorageError::Backend {
            backend: self.backend_kind(),
            message: "asset upload state is not supported by this backend".to_owned(),
        })
    }

    async fn pending_asset_uploads(
        &self,
        _workspace_id: u64,
        _limit: usize,
    ) -> StorageResult<Vec<AssetManifestRecord>> {
        Ok(Vec::new())
    }

    async fn asset_references(&self, _asset_id: AssetId) -> StorageResult<Vec<AssetReference>> {
        Ok(Vec::new())
    }

    async fn commit(&self, batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome>;

    async fn flush(&self) -> StorageResult<()> {
        Ok(())
    }
}

#[async_trait]
pub trait StorageProvider: Send + Sync {
    fn label(&self) -> &str;

    fn open_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(90)
    }

    async fn open(&self) -> StorageResult<Arc<dyn DocumentStorage>>;
}

#[derive(Clone)]
pub struct StaticStorageProvider {
    label: String,
    storage: Arc<dyn DocumentStorage>,
}

impl StaticStorageProvider {
    pub fn new(label: impl Into<String>, storage: Arc<dyn DocumentStorage>) -> Self {
        Self {
            label: label.into(),
            storage,
        }
    }
}

#[async_trait]
impl StorageProvider for StaticStorageProvider {
    fn label(&self) -> &str {
        &self.label
    }

    async fn open(&self) -> StorageResult<Arc<dyn DocumentStorage>> {
        Ok(self.storage.clone())
    }
}
