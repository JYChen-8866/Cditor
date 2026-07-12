use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRuntimeFromStoreOptions {
    pub viewport_height: u32,
    pub visible_index_version: i64,
    pub initial_payload_window_blocks: usize,
    pub layout_key: LayoutCacheKey,
}

impl Default for DocumentRuntimeFromStoreOptions {
    fn default() -> Self {
        Self {
            viewport_height: 720,
            visible_index_version: 0,
            initial_payload_window_blocks: 64,
            layout_key: LayoutCacheKey {
                width_bucket: 10,
                exact_width_px: 800,
                content_version: 1,
                attrs_version: 0,
                style_version: 0,
                font_version: 0,
                theme_version: 0,
                scale_factor_milli: 1000,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentRuntimeIndexSource {
    Snapshot,
    Blocks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRuntimeColdStartReport {
    pub document_title: String,
    pub index_source: DocumentRuntimeIndexSource,
    pub total_blocks: usize,
    pub payloads_loaded: usize,
    pub payloads_missing: usize,
    pub layout_cache_hits: usize,
}

impl DocumentRuntime {
    pub async fn from_store(
        document_id: PgDocumentId,
        document_store: &PostgresDocumentStore,
        payload_store: &PostgresPayloadStore,
        layout_store: &PostgresLayoutCacheStore,
        options: DocumentRuntimeFromStoreOptions,
    ) -> PostgresStorageResult<(Self, DocumentRuntimeColdStartReport)> {
        let metadata = document_store.load_document_metadata(document_id).await?;
        let runtime_document_id = runtime_document_id_from_pg(metadata.id).ok_or_else(|| {
            PostgresStorageError::CorruptData {
                message: format!("document id {} is outside runtime namespace", metadata.id),
            }
        })?;
        let structure_version = u64::try_from(metadata.structure_version).map_err(|_| {
            PostgresStorageError::CorruptData {
                message: format!(
                    "document {} has negative structure_version {}",
                    metadata.id, metadata.structure_version
                ),
            }
        })?;

        let snapshot_records = document_store
            .load_document_index_snapshot(
                document_id,
                options.visible_index_version,
                metadata.structure_version,
            )
            .await?;
        let (mut records, index_source) = match snapshot_records {
            Some(records) => (records, DocumentRuntimeIndexSource::Snapshot),
            None => (
                document_store.load_block_index_records(document_id).await?,
                DocumentRuntimeIndexSource::Blocks,
            ),
        };

        let mut layout_cache_hits = 0usize;
        for record in &mut records {
            let cached = layout_store
                .load_block_height(record.id, options.layout_key)
                .await?;
            if cached.source != CacheSource::Missing {
                layout_cache_hits += 1;
                record.layout_meta = BlockLayoutMeta {
                    block_id: record.id,
                    estimated_height: cached.height,
                    measured_height: (cached.source == CacheSource::ExactMatch)
                        .then_some(cached.height),
                    width_bucket: options.layout_key.width_bucket,
                    layout_version: options.layout_key.content_version,
                    dirty: cached.source != CacheSource::ExactMatch,
                };
            }
        }

        let initial_window_end = records.len().min(options.initial_payload_window_blocks);
        let initial_block_ids = records
            .iter()
            .take(initial_window_end)
            .map(|record| record.id)
            .collect::<Vec<_>>();
        let loaded_payloads = payload_store
            .load_block_payloads(&initial_block_ids)
            .await?;
        let payloads_missing = loaded_payloads.missing_block_ids.len();
        let payloads_loaded = loaded_payloads.records.len();

        let runtime = Self::from_index_records_with_window(
            runtime_document_id,
            records,
            loaded_payloads.records,
            structure_version,
            f64::from(options.viewport_height),
            0..initial_window_end,
        );
        let total_blocks = runtime.index.total_count();

        Ok((
            runtime,
            DocumentRuntimeColdStartReport {
                document_title: metadata.title,
                index_source,
                total_blocks,
                payloads_loaded,
                payloads_missing,
                layout_cache_hits,
            },
        ))
    }
}
