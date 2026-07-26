use cditor_storage::{
    DocumentLoadProgress, DocumentLoadStage, LoadDocumentRequest, LoadedDocument, StorageError,
    StorageResult,
};

use super::SqliteDocumentStorage;

const DOCUMENT_LOAD_UNITS: usize = 7;

impl SqliteDocumentStorage {
    pub(super) async fn load_document_inner(
        &self,
        request: LoadDocumentRequest,
        progress: &mut (dyn FnMut(DocumentLoadProgress) + Send),
    ) -> StorageResult<LoadedDocument> {
        let mut completed_units = 0;
        let mut advance = |stage| {
            completed_units += 1;
            progress(DocumentLoadProgress {
                stage,
                completed_units,
                total_units: DOCUMENT_LOAD_UNITS,
            });
        };

        self.ensure_minimal_document(&request).await?;
        advance(DocumentLoadStage::EnsureDocument);
        let metadata = self.load_metadata(request.document_id).await?;
        advance(DocumentLoadStage::Metadata);
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
        advance(DocumentLoadStage::Structure);
        let layout_cache_hits = self
            .apply_block_layout_cache(request.document_id, &mut records, request.layout_key)
            .await?;
        advance(DocumentLoadStage::BlockLayout);
        let page_layout_snapshot = self
            .load_page_layout_snapshot(
                request.document_id,
                request.visible_index_version,
                metadata.structure_version,
                request.layout_key,
                request.page_policy_version,
            )
            .await?;
        advance(DocumentLoadStage::PageLayout);
        let block_attrs = self.load_attrs(request.document_id).await?;
        advance(DocumentLoadStage::Attributes);
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
        advance(DocumentLoadStage::InitialPayloads);
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
}
