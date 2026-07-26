use std::fmt;
use std::sync::Arc;

use cditor_core::edit::{EditTransaction, ExternalUndoBlobRef};
use cditor_core::ids::{BlockId, DocumentId};
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{
    DocumentStorage, EmergencyLogAppendOutcome, EmergencyLogEntry, LoadedPayloadBatch,
    StorageBackendKind, StorageCapabilities, StorageResult, StorageSaveBatch, StorageSaveOutcome,
};

/// Document-scoped persistence capability owned by the application session.
///
/// Storage adapters remain document-agnostic ports. This object binds one
/// open document and its layout policy at the Session boundary.
#[derive(Clone)]
pub struct DocumentPersistence {
    storage: Arc<dyn DocumentStorage>,
    document_id: DocumentId,
    layout_key: Option<LayoutCacheKey>,
}

impl DocumentPersistence {
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

    pub async fn write_undo_blob(
        &self,
        snapshot_id: u64,
        block_count: usize,
        transaction: &EditTransaction,
    ) -> StorageResult<ExternalUndoBlobRef> {
        self.storage
            .write_undo_blob(self.document_id, snapshot_id, block_count, transaction)
            .await
    }

    pub async fn load_undo_blob(
        &self,
        reference: &ExternalUndoBlobRef,
    ) -> StorageResult<EditTransaction> {
        self.storage
            .load_undo_blob(self.document_id, reference)
            .await
    }

    pub async fn delete_undo_blob(&self, snapshot_id: u64) -> StorageResult<bool> {
        self.storage
            .delete_undo_blob(self.document_id, snapshot_id)
            .await
    }

    pub async fn prune_undo_blobs(&self, keep_recent: usize) -> StorageResult<u64> {
        self.storage
            .prune_undo_blobs(self.document_id, keep_recent)
            .await
    }

    pub async fn append_emergency_transactions(
        &self,
        transactions: &[EditTransaction],
    ) -> StorageResult<EmergencyLogAppendOutcome> {
        self.storage
            .append_emergency_transactions(self.document_id, transactions)
            .await
    }

    pub async fn load_emergency_transactions(&self) -> StorageResult<Vec<EmergencyLogEntry>> {
        self.storage
            .load_emergency_transactions(self.document_id)
            .await
    }

    pub async fn acknowledge_emergency_transactions(
        &self,
        through_sequence: u64,
    ) -> StorageResult<u64> {
        self.storage
            .acknowledge_emergency_transactions(self.document_id, through_sequence)
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

impl fmt::Debug for DocumentPersistence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DocumentPersistence")
            .field("backend", &self.backend_kind())
            .field("document_id", &self.document_id)
            .field("has_layout_key", &self.layout_key.is_some())
            .finish_non_exhaustive()
    }
}
