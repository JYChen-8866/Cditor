use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cditor_session::{DocumentPersistence, PersistencePipeline};
use cditor_storage::{
    DOCUMENT_INDEX_VISIBLE_VERSION, DocumentStorage, LoadDocumentRequest, LoadedDocument,
    LoadedPayloadBatch, StorageBackendKind, StorageCapabilities, StorageError, StorageResult,
    StorageSaveBatch, StorageSaveOutcome,
};

use cditor_core::document::BlockIndexRecord;
use cditor_core::rich_text::{
    BlockAttrs, BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind,
};
use cditor_storage::layout_cache::LayoutCacheKey;

#[derive(Debug, Clone)]
pub struct FailFirstPersistenceProbe {
    storage: Arc<FailFirstStorage>,
}

impl FailFirstPersistenceProbe {
    pub fn transaction_counts(&self) -> Vec<usize> {
        self.storage.transaction_counts.lock().unwrap().clone()
    }
}

pub fn fail_first_persistence_fixture(
    document_id: cditor_core::ids::DocumentId,
    autosave_interval: Option<Duration>,
) -> (PersistencePipeline, FailFirstPersistenceProbe) {
    let storage = Arc::new(FailFirstStorage::default());
    let pipeline = PersistencePipeline::for_session(
        DocumentPersistence::new(storage.clone(), document_id),
        autosave_interval,
    );
    (pipeline, FailFirstPersistenceProbe { storage })
}

#[derive(Debug, Clone, Copy)]
pub struct StorageContractConfig {
    pub backend: StorageBackendKind,
    pub document_id: cditor_core::ids::DocumentId,
    pub isolated_document_id: cditor_core::ids::DocumentId,
}

/// Runs the behavior shared by every durable `DocumentStorage` adapter.
pub async fn run_document_storage_contract(
    storage: &dyn DocumentStorage,
    config: StorageContractConfig,
) {
    assert_eq!(storage.backend_kind(), config.backend);
    assert!(storage.capabilities().payload_window);

    let request = storage_contract_request(config.document_id);
    let loaded = storage.load_document(request.clone()).await.unwrap();
    assert_eq!(loaded.metadata.document_id, config.document_id);
    assert!(!loaded.records.is_empty());
    assert!(!loaded.initial_payloads.is_empty());

    let block_id = loaded.records[0].id;
    let next_version = loaded.initial_payloads[0].content_version.saturating_add(1);
    let mut payload = BlockPayloadRecord::rich_text(
        block_id,
        RichBlockKind::Paragraph,
        "shared storage contract",
    );
    payload.content_version = next_version;
    let outcome = storage
        .commit(StorageSaveBatch {
            document_id: config.document_id,
            layout_key: None,
            payloads: vec![payload],
            index_records: Vec::new(),
            structure_version: loaded.metadata.structure_version,
            transactions: Vec::new(),
            block_attrs: vec![(block_id, BlockAttrs::default())],
            page_layout_snapshot: None,
        })
        .await
        .unwrap();
    assert_eq!(
        outcome.saved_payload_versions,
        vec![(block_id, next_version)]
    );

    let payloads = storage
        .load_payloads(config.document_id, &[block_id])
        .await
        .unwrap();
    assert!(payloads.missing_block_ids.is_empty());
    assert_eq!(payloads.records[0].plain_text(), "shared storage contract");

    let isolated = storage
        .load_document(storage_contract_request(config.isolated_document_id))
        .await
        .unwrap();
    assert_eq!(isolated.metadata.document_id, config.isolated_document_id);
    assert_ne!(isolated.metadata.document_id, loaded.metadata.document_id);
    assert_ne!(
        isolated.initial_payloads[0].plain_text(),
        "shared storage contract"
    );
    storage.flush().await.unwrap();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixedStorageSeedReport {
    pub document_id: cditor_core::ids::DocumentId,
    pub block_count: usize,
}

/// Seeds development and acceptance data through the public storage port.
pub async fn seed_mixed_storage_document(
    storage: &dyn DocumentStorage,
    document_id: cditor_core::ids::DocumentId,
    block_count: usize,
) -> StorageResult<MixedStorageSeedReport> {
    let block_count = block_count.max(1);
    let loaded = storage
        .load_document(storage_contract_request(document_id))
        .await?;
    let first_id = loaded.records[0].id;
    let base = document_id
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_add(10_000))
        .ok_or_else(|| StorageError::InvalidConfiguration("seed id range overflow".to_owned()))?;
    let paragraph_tag = kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph);
    let mut records = Vec::with_capacity(block_count);
    let mut payloads = Vec::with_capacity(block_count);
    records.push(BlockIndexRecord::new(first_id, None, 0, paragraph_tag, 0));
    payloads.push(BlockPayloadRecord::rich_text(
        first_id,
        RichBlockKind::Paragraph,
        "Mixed storage fixture 0",
    ));
    for index in 1..block_count {
        let block_id = base
            .checked_add(index as u64)
            .ok_or_else(|| StorageError::InvalidConfiguration("seed block overflow".to_owned()))?;
        records.push(BlockIndexRecord::new(block_id, None, 0, paragraph_tag, 0));
        let kind = match index % 4 {
            0 => RichBlockKind::Heading { level: 2 },
            1 => RichBlockKind::Paragraph,
            2 => RichBlockKind::BulletedList,
            _ => RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
        };
        payloads.push(BlockPayloadRecord::rich_text(
            block_id,
            kind,
            format!("Mixed storage fixture {index}"),
        ));
    }
    storage
        .commit(StorageSaveBatch {
            document_id,
            layout_key: None,
            payloads,
            index_records: records,
            structure_version: loaded.metadata.structure_version.saturating_add(1),
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: None,
        })
        .await?;
    storage.flush().await?;
    Ok(MixedStorageSeedReport {
        document_id,
        block_count,
    })
}

fn storage_contract_request(document_id: cditor_core::ids::DocumentId) -> LoadDocumentRequest {
    LoadDocumentRequest {
        document_id,
        workspace_id: 1,
        initial_payload_window_blocks: 32,
        visible_index_version: DOCUMENT_INDEX_VISIBLE_VERSION,
        layout_key: LayoutCacheKey {
            width_bucket: 10,
            exact_width_px: 800,
            content_version: 1,
            attrs_version: 0,
            style_version: 0,
            font_version: 0,
            theme_version: 0,
            scale_factor_milli: 1_000,
        },
        page_policy_version: cditor_core::layout::PAGE_POLICY_VERSION,
    }
}

#[derive(Debug, Default)]
struct FailFirstStorage {
    attempts: AtomicUsize,
    transaction_counts: Mutex<Vec<usize>>,
}

#[async_trait]
impl DocumentStorage for FailFirstStorage {
    fn backend_kind(&self) -> StorageBackendKind {
        StorageBackendKind::Custom
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities {
            emergency_log: false,
            ..StorageCapabilities::SQLITE
        }
    }

    async fn load_document(&self, _request: LoadDocumentRequest) -> StorageResult<LoadedDocument> {
        unreachable!("fail-first persistence fixture does not load documents")
    }

    async fn load_payloads(
        &self,
        _document_id: cditor_core::ids::DocumentId,
        _block_ids: &[cditor_core::ids::BlockId],
    ) -> StorageResult<LoadedPayloadBatch> {
        unreachable!("fail-first persistence fixture does not load payload windows")
    }

    async fn commit(&self, batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
        self.transaction_counts
            .lock()
            .unwrap()
            .push(batch.transactions.len());
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(StorageError::Backend {
                backend: StorageBackendKind::Custom,
                message: "injected first-save failure".to_owned(),
            });
        }
        Ok(StorageSaveOutcome {
            saved_structure_version: batch.saved_structure_version(),
            saved_payload_versions: batch
                .payloads
                .iter()
                .map(|payload| (payload.block_id, payload.content_version))
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_exposes_an_enabled_pipeline_and_stable_probe() {
        let (pipeline, probe) = fail_first_persistence_fixture(7, None);

        assert!(pipeline.is_enabled());
        assert!(probe.transaction_counts().is_empty());
    }
}
