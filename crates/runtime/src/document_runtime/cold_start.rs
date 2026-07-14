use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentRuntimeIndexSource {
    Snapshot,
    Blocks,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentRuntimeColdStartData {
    pub document_id: DocumentId,
    pub document_title: String,
    pub structure_version: u64,
    pub records: Vec<BlockIndexRecord>,
    pub block_attrs: Vec<(BlockId, BlockAttrs)>,
    pub initial_payloads: Vec<BlockPayloadRecord>,
    pub initial_payload_window_end: usize,
    pub index_source: DocumentRuntimeIndexSource,
    pub layout_cache_hits: usize,
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
    /// Builds the live runtime from storage-neutral cold-start data.
    ///
    /// All database I/O and database identifier conversion must happen in the
    /// application integration layer before this boundary.
    pub fn from_cold_start_data(
        data: DocumentRuntimeColdStartData,
        viewport_height: f64,
    ) -> Result<(Self, DocumentRuntimeColdStartReport), String> {
        if !viewport_height.is_finite() || viewport_height <= 0.0 {
            return Err(format!(
                "cold-start viewport height must be positive and finite, got {viewport_height}"
            ));
        }
        if data.initial_payload_window_end > data.records.len() {
            return Err(format!(
                "cold-start payload window end {} exceeds document length {}",
                data.initial_payload_window_end,
                data.records.len()
            ));
        }

        let mut document_block_ids = HashSet::with_capacity(data.records.len());
        for record in &data.records {
            if !document_block_ids.insert(record.id) {
                return Err(format!(
                    "cold-start document index contains duplicate block id {}",
                    record.id
                ));
            }
            if record.layout_meta.block_id != record.id {
                return Err(format!(
                    "cold-start block {} has layout metadata for block {}",
                    record.id, record.layout_meta.block_id
                ));
            }
            let height = record.layout_meta.effective_height();
            if !height.is_finite() || height <= 0.0 {
                return Err(format!(
                    "cold-start block {} has invalid effective height {height}",
                    record.id
                ));
            }
        }

        let window_end = data.initial_payload_window_end;
        let expected_payload_ids = data
            .records
            .iter()
            .take(window_end)
            .map(|record| record.id)
            .collect::<HashSet<_>>();
        let loaded_payload_ids = data
            .initial_payloads
            .iter()
            .map(|record| record.block_id)
            .collect::<HashSet<_>>();
        if loaded_payload_ids.len() != data.initial_payloads.len() {
            return Err("cold-start payload window contains duplicate block ids".to_owned());
        }
        let unexpected_payload_ids = loaded_payload_ids
            .difference(&expected_payload_ids)
            .copied()
            .collect::<Vec<_>>();
        if !unexpected_payload_ids.is_empty() {
            return Err(format!(
                "cold-start payload window contains {} unexpected blocks",
                unexpected_payload_ids.len()
            ));
        }
        let missing_payload_ids = expected_payload_ids
            .difference(&loaded_payload_ids)
            .copied()
            .collect::<Vec<_>>();
        if !missing_payload_ids.is_empty() {
            let sample = missing_payload_ids
                .iter()
                .take(5)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "initial payload window is missing {} blocks (sample block ids: {sample})",
                missing_payload_ids.len()
            ));
        }

        let mut attrs_block_ids = HashSet::with_capacity(data.block_attrs.len());
        for (block_id, _) in &data.block_attrs {
            if !attrs_block_ids.insert(*block_id) {
                return Err(format!(
                    "cold-start block attributes contain duplicate block id {block_id}"
                ));
            }
            if !document_block_ids.contains(block_id) {
                return Err(format!(
                    "cold-start block attributes reference unknown block id {block_id}"
                ));
            }
        }

        let payloads_loaded = data.initial_payloads.len();
        let document_title = data.document_title;
        let index_source = data.index_source;
        let layout_cache_hits = data.layout_cache_hits;
        let mut runtime = Self::from_index_records_with_window(
            data.document_id,
            data.records,
            data.initial_payloads,
            data.structure_version,
            viewport_height,
            0..window_end,
        );
        runtime.block_attrs = data.block_attrs.into_iter().collect();
        let total_blocks = runtime.index.total_count();

        Ok((
            runtime,
            DocumentRuntimeColdStartReport {
                document_title,
                index_source,
                total_blocks,
                payloads_loaded,
                payloads_missing: 0,
                layout_cache_hits,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records(count: u64) -> Vec<BlockIndexRecord> {
        (1..=count)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
            })
            .collect()
    }

    #[test]
    fn cold_start_builds_runtime_without_a_concrete_store_dependency() {
        let (runtime, report) = DocumentRuntime::from_cold_start_data(
            DocumentRuntimeColdStartData {
                document_id: 9,
                document_title: "Loaded".to_owned(),
                structure_version: 3,
                records: records(3),
                block_attrs: vec![(1, BlockAttrs::default())],
                initial_payloads: vec![
                    BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "one"),
                    BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "two"),
                ],
                initial_payload_window_end: 2,
                index_source: DocumentRuntimeIndexSource::Snapshot,
                layout_cache_hits: 1,
            },
            720.0,
        )
        .unwrap();

        assert_eq!(runtime.document_id, 9);
        assert_eq!(runtime.index.structure_version, 3);
        assert_eq!(runtime.payload_window.block_range, 0..2);
        assert_eq!(report.document_title, "Loaded");
        assert_eq!(report.index_source, DocumentRuntimeIndexSource::Snapshot);
        assert_eq!(report.total_blocks, 3);
        assert_eq!(report.payloads_loaded, 2);
        assert_eq!(report.layout_cache_hits, 1);
    }

    #[test]
    fn cold_start_rejects_a_missing_initial_payload() {
        let error = DocumentRuntime::from_cold_start_data(
            DocumentRuntimeColdStartData {
                document_id: 9,
                document_title: "Broken".to_owned(),
                structure_version: 1,
                records: records(2),
                block_attrs: Vec::new(),
                initial_payloads: vec![BlockPayloadRecord::rich_text(
                    1,
                    RichBlockKind::Paragraph,
                    "one",
                )],
                initial_payload_window_end: 2,
                index_source: DocumentRuntimeIndexSource::Blocks,
                layout_cache_hits: 0,
            },
            720.0,
        )
        .unwrap_err();

        assert!(error.contains("missing 1 blocks"));
        assert!(error.contains('2'));
    }

    #[test]
    fn cold_start_rejects_invalid_storage_boundary_data_before_runtime_construction() {
        let mut duplicate_records = records(2);
        duplicate_records[1].id = 1;
        duplicate_records[1].layout_meta.block_id = 1;
        let error = DocumentRuntime::from_cold_start_data(
            DocumentRuntimeColdStartData {
                document_id: 9,
                document_title: "Broken".to_owned(),
                structure_version: 1,
                records: duplicate_records,
                block_attrs: Vec::new(),
                initial_payloads: Vec::new(),
                initial_payload_window_end: 0,
                index_source: DocumentRuntimeIndexSource::Blocks,
                layout_cache_hits: 0,
            },
            720.0,
        )
        .unwrap_err();
        assert!(error.contains("duplicate block id 1"));

        let error = DocumentRuntime::from_cold_start_data(
            DocumentRuntimeColdStartData {
                document_id: 9,
                document_title: "Broken".to_owned(),
                structure_version: 1,
                records: records(1),
                block_attrs: vec![(2, BlockAttrs::default())],
                initial_payloads: Vec::new(),
                initial_payload_window_end: 0,
                index_source: DocumentRuntimeIndexSource::Blocks,
                layout_cache_hits: 0,
            },
            720.0,
        )
        .unwrap_err();
        assert!(error.contains("unknown block id 2"));
    }
}
