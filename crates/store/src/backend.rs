use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::EditTransaction;
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::rich_text::{BlockAttrs, BlockPayloadRecord};

use crate::error::StorageResult;
use crate::layout_cache::LayoutCacheKey;
use crate::page_layout_snapshot::StoragePageLayoutSnapshot;

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
}

impl StorageCapabilities {
    pub const SQLITE: Self = Self {
        payload_window: true,
        full_text_search: false,
        cloud_sync: false,
        server_authoritative: false,
    };

    pub const POSTGRES: Self = Self {
        payload_window: true,
        full_text_search: true,
        cloud_sync: false,
        server_authoritative: true,
    };
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

    async fn load_payloads(
        &self,
        document_id: DocumentId,
        block_ids: &[BlockId],
    ) -> StorageResult<LoadedPayloadBatch>;

    async fn commit(&self, batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome>;

    async fn flush(&self) -> StorageResult<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct StorageSession {
    storage: Arc<dyn DocumentStorage>,
    document_id: DocumentId,
    layout_key: Option<LayoutCacheKey>,
}

impl StorageSession {
    pub fn new(storage: Arc<dyn DocumentStorage>, document_id: DocumentId) -> Self {
        Self {
            storage,
            document_id,
            layout_key: None,
        }
    }

    pub fn with_layout_key(mut self, layout_key: LayoutCacheKey) -> Self {
        self.layout_key = Some(layout_key);
        self
    }

    pub fn document_id(&self) -> DocumentId {
        self.document_id
    }

    pub fn backend_kind(&self) -> StorageBackendKind {
        self.storage.backend_kind()
    }

    pub fn capabilities(&self) -> StorageCapabilities {
        self.storage.capabilities()
    }

    pub fn layout_key(&self) -> Option<LayoutCacheKey> {
        self.layout_key
    }

    pub async fn load_payloads(&self, block_ids: &[BlockId]) -> StorageResult<LoadedPayloadBatch> {
        self.storage
            .load_payloads(self.document_id, block_ids)
            .await
    }

    pub async fn commit(&self, mut batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
        batch.document_id = self.document_id;
        if batch.layout_key.is_none() {
            batch.layout_key = self.layout_key;
        }
        self.storage.commit(batch).await
    }

    pub async fn flush(&self) -> StorageResult<()> {
        self.storage.flush().await
    }
}

impl fmt::Debug for StorageSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StorageSession")
            .field("backend", &self.backend_kind())
            .field("document_id", &self.document_id)
            .field("has_layout_key", &self.layout_key.is_some())
            .finish_non_exhaustive()
    }
}
