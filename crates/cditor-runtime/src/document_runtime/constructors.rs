use super::document_state::DocumentState;
use super::editing_state::EditingState;
use super::selection_state::SelectionState;
use super::*;

impl DocumentRuntime {
    pub fn empty() -> Self {
        let mut document = RichTextDocument::empty(1);
        document.push_root_block(RichBlockRecord::rich_text(
            2,
            RichBlockKind::DocumentTitle,
            "",
        ));
        document.push_root_block(RichBlockRecord::paragraph(1, ""));
        Self::from_rich_text_document(document, 720.0)
    }

    pub fn empty_composer() -> Self {
        Self::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "",
            )],
            720.0,
        )
    }

    pub fn demo() -> Self {
        let mut document = RichTextDocument::empty(1);
        document.push_root_block(RichBlockRecord::rich_text(
            5,
            RichBlockKind::DocumentTitle,
            "Cditor",
        ));
        document.push_root_block(RichBlockRecord::paragraph(1, "正文内容"));
        document.push_root_block(RichBlockRecord::paragraph(
            2,
            "这是接入当前 V2 runtime 的最小 GPUI 富文本编辑器。",
        ));
        document.push_root_block(RichBlockRecord::paragraph(3, "点击窗口后直接输入文本。"));
        document.push_root_block(RichBlockRecord::quote(4, "UI 只是投影，runtime 才是真相。"));
        Self::from_rich_text_document(document, 720.0)
    }

    pub fn large_mixed_demo() -> Self {
        let total_start = Instant::now();
        let count = cditor_core::demo_fixtures::LARGE_MIXED_DEMO_BLOCKS;

        let start = Instant::now();
        let records = cditor_core::demo_fixtures::large_mixed_demo_index_records(count);
        log_runtime_timing("large_demo.index_records", start, Some(count));

        let initial_payload_window = 0..512.min(count);
        let start = Instant::now();
        let payloads = cditor_core::demo_fixtures::large_mixed_demo_payload_records(
            initial_payload_window.clone(),
            count,
        );
        log_runtime_timing(
            "large_demo.initial_payloads",
            start,
            Some(initial_payload_window.len()),
        );

        let start = Instant::now();
        let mut runtime = Self::from_index_records_with_window_and_page_policy(
            cditor_core::demo_fixtures::LARGE_MIXED_DEMO_DOCUMENT_ID,
            records,
            payloads,
            1,
            720.0,
            initial_payload_window,
            large_demo_page_policy(),
        );
        log_runtime_timing("large_demo.runtime_from_index", start, Some(count));

        runtime.document.demo_payload_count = Some(count);
        log_runtime_timing("large_demo.total", total_start, Some(count));
        runtime
    }

    pub fn from_payloads(
        document_id: DocumentId,
        payloads: Vec<BlockPayloadRecord>,
        viewport_height: f64,
    ) -> Self {
        let records = payloads
            .iter()
            .enumerate()
            .map(|(index, payload)| {
                BlockIndexRecord::new(
                    payload.block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&payload.kind),
                    0,
                )
                .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(
                    payload.block_id,
                    estimate_payload_height(payload, index),
                ))
            })
            .collect::<Vec<_>>();
        Self::from_index_records(document_id, records, payloads, 1, viewport_height)
    }

    pub fn from_rich_text_document(mut document: RichTextDocument, viewport_height: f64) -> Self {
        let mut metadata = document.metadata.clone();
        if metadata.name.is_none() {
            metadata.name = metadata.title.take();
        }
        if !document
            .blocks
            .iter()
            .any(|block| matches!(block.kind, RichBlockKind::DocumentTitle))
        {
            let title_id = document
                .blocks
                .iter()
                .map(|block| block.id)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            let mut title = RichBlockRecord::rich_text(
                title_id,
                RichBlockKind::DocumentTitle,
                metadata.name.clone().unwrap_or_default(),
            );
            title.document_id = document.id;
            title.structure_version = document.structure_version;
            title.next_id = document.root_blocks.first().copied();
            if let Some(first_root) = document.root_blocks.first().copied()
                && let Some(first) = document
                    .blocks
                    .iter_mut()
                    .find(|block| block.id == first_root)
            {
                first.prev_id = Some(title_id);
            }
            document.root_blocks.insert(0, title_id);
            document.blocks.insert(0, title);
        }
        let block_attrs = document
            .blocks
            .iter()
            .filter(|block| block.attrs != BlockAttrs::default())
            .map(|block| (block.id, block.attrs.clone()))
            .collect();
        let mut runtime = Self::from_index_records(
            document.id,
            document.index_records(),
            document.payload_records(),
            document.structure_version,
            viewport_height,
        );
        runtime.document.block_attrs = block_attrs;
        runtime.document.metadata = metadata;
        runtime
    }

    pub(super) fn from_index_records(
        document_id: DocumentId,
        records: Vec<BlockIndexRecord>,
        payloads: Vec<BlockPayloadRecord>,
        structure_version: u64,
        viewport_height: f64,
    ) -> Self {
        let payload_window_range = 0..records.len();
        Self::from_index_records_with_window(
            document_id,
            records,
            payloads,
            structure_version,
            viewport_height,
            payload_window_range,
        )
    }

    pub(super) fn from_index_records_with_window(
        document_id: DocumentId,
        records: Vec<BlockIndexRecord>,
        payloads: Vec<BlockPayloadRecord>,
        structure_version: u64,
        viewport_height: f64,
        payload_window_range: Range<usize>,
    ) -> Self {
        Self::from_index_records_with_window_and_page_policy(
            document_id,
            records,
            payloads,
            structure_version,
            viewport_height,
            payload_window_range,
            PagePolicy::default(),
        )
    }

    pub(super) fn from_index_records_with_window_and_page_policy(
        document_id: DocumentId,
        mut records: Vec<BlockIndexRecord>,
        payloads: Vec<BlockPayloadRecord>,
        structure_version: u64,
        viewport_height: f64,
        payload_window_range: Range<usize>,
        page_policy: PagePolicy,
    ) -> Self {
        let record_count = records.len();
        let loaded_kinds = payloads
            .iter()
            .map(|payload| (payload.block_id, &payload.kind))
            .collect::<HashMap<_, _>>();
        for record in &mut records {
            if let Some(kind) = loaded_kinds.get(&record.id) {
                record.kind_tag = kind_tag_for_rich_block_kind(kind);
            }
        }
        let loaded_table_heights = payloads
            .iter()
            .filter_map(|payload| match (&payload.kind, &payload.payload) {
                (RichBlockKind::Table, BlockPayload::Table(table)) => Some((
                    payload.block_id,
                    f64::from(table::table_payload_projected_height_px(table)),
                )),
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        let loaded_video_heights = payloads
            .iter()
            .filter_map(|payload| {
                matches!(payload.kind, RichBlockKind::Video)
                    .then(|| (payload.block_id, estimate_payload_height(payload, 0)))
            })
            .collect::<HashMap<_, _>>();
        for record in &mut records {
            normalize_whiteboard_layout(record);
            let loaded_video_height = loaded_video_heights.get(&record.id).copied();
            if let Some(height) = loaded_table_heights.get(&record.id).copied() {
                record.layout_meta.estimated_height = height;
                record.layout_meta.measured_height = Some(height);
                record.layout_meta.dirty = false;
            }
            normalize_video_layout(record, loaded_video_height);
        }
        let start = Instant::now();
        let index = DocumentIndex::new(document_id, records, structure_version)
            .expect("document index is valid");
        log_runtime_timing("runtime.document_index", start, Some(record_count));

        let start = Instant::now();
        let list_projection_cache = ListProjectionCache::build(&index);
        log_runtime_timing(
            "runtime.document.list_projection_cache",
            start,
            Some(record_count),
        );

        let start = Instant::now();
        let visible_index = VisibleDocumentIndex::from_document_index(&index);
        log_runtime_timing("runtime.document.visible_index", start, Some(record_count));

        let start = Instant::now();
        let height_index = BlockHeightIndex::from_visible_document(&index, &visible_index)
            .expect("demo heights are valid");
        log_runtime_timing("runtime.layout.height_index", start, Some(record_count));

        let start = Instant::now();
        let page_layout = PageLayoutIndex::from_block_height_index(&height_index, page_policy)
            .expect("demo pages are valid")
            .with_identity(cditor_core::layout::PageLayoutIdentity::for_page(
                document_id,
                visible_index.source_structure_version,
                visible_index.visibility_version,
                0,
                cditor_core::layout::PAGE_POLICY_VERSION,
                0,
            ));
        log_runtime_timing(
            "runtime.layout.page_layout",
            start,
            Some(page_layout.page_count()),
        );
        let scroll = VirtualScrollState::new(viewport_height, height_index.total_height())
            .expect("demo scroll state is valid");
        let payload_window_range = payload_window_range
            .start
            .min(visible_index.total_visible_count())
            ..payload_window_range
                .end
                .min(visible_index.total_visible_count());
        let mut payload_window = PayloadWindow::new(payload_window_range);
        let mut table_runtimes = HashMap::new();
        let mut text_models = HashMap::new();
        for payload in payloads {
            let payload = normalize_payload_record_for_kind(payload);
            if matches!(payload.kind, RichBlockKind::Table) {
                table_runtimes.insert(
                    payload.block_id,
                    TableRuntime::from_payload(payload.payload.clone()),
                );
            }
            sync_text_model_for_payload(&mut text_models, &payload);
            payload_window.insert_loaded(payload);
        }

        let runtime = Self {
            document_id,
            document: DocumentState {
                metadata: DocumentMetadata::default(),
                revision: structure_version,
                index,
                visible_index,
                payload_window,
                block_attrs: HashMap::new(),
                collection_records: HashMap::new(),
                comment_threads: HashMap::new(),
                assets: HashMap::new(),
                block_asset_ids: HashMap::new(),
                table_runtimes,
                text_models,
                list_projection_cache,
                demo_payload_count: None,
            },
            layout: LayoutState {
                height_index,
                page_layout,
                page_local_cache: HashMap::new(),
                scroll,
                table_horizontal_scroll_offsets: HashMap::new(),
                payload_window_generation: 0,
                payload_prefetch_residency_probe: None,
                window_planner: WindowPlanner::new(1, 2, WindowPlannerPolicy::default()),
                last_planned_scroll_top: 0.0,
                window_plan_clock_ms: 0,
                window_memory_pressure: WindowMemoryPressure::Normal,
                projection: ProjectionState::default(),
                pending_measured_heights: HashMap::new(),
                animating_heights: HashSet::new(),
                dirty: false,
                scrollbar_drag: None,
            },
            editing: EditingState {
                next_input_session_id: 1,
                ..EditingState::default()
            },
            selection: SelectionState::default(),
            ai_session: None,
            next_ai_request_id: 1,
            history: HistoryState::default(),
            transactions: TransactionState::default(),
        };
        runtime
    }
}

const WHITEBOARD_STABLE_BLOCK_HEIGHT_PX: f64 = 480.0;

fn normalize_whiteboard_layout(record: &mut BlockIndexRecord) {
    if !matches!(
        rich_block_kind_from_tag(record.kind_tag),
        RichBlockKind::Whiteboard
    ) || (record.layout_meta.effective_height() - WHITEBOARD_STABLE_BLOCK_HEIGHT_PX).abs() < 0.5
    {
        return;
    }

    // Whiteboards have a deterministic stable box. Do not carry an exact height
    // written by older document renderers into a reopened runtime.
    record.layout_meta.estimated_height = WHITEBOARD_STABLE_BLOCK_HEIGHT_PX;
    record.layout_meta.measured_height = None;
    record.layout_meta.dirty = true;
}

fn normalize_video_layout(record: &mut BlockIndexRecord, loaded_height: Option<f64>) {
    if !matches!(
        rich_block_kind_from_tag(record.kind_tag),
        RichBlockKind::Video
    ) {
        return;
    }
    let height = loaded_height.unwrap_or(cditor_core::layout::VIDEO_BLOCK_ESTIMATED_HEIGHT_PX);
    if (record.layout_meta.effective_height() - height).abs() < 0.5
        && record.layout_meta.measured_height == Some(height)
    {
        return;
    }

    // Older builds persisted a fixed 360px box, then replaced it only after
    // the video entered the viewport. Normalize before constructing the height
    // index so scroll planning never observes that stale geometry.
    record.layout_meta.estimated_height = height;
    record.layout_meta.measured_height = Some(height);
    record.layout_meta.dirty = false;
}

#[cfg(test)]
mod whiteboard_layout_tests {
    use super::*;

    #[test]
    fn reopening_discards_a_stale_whiteboard_height() {
        let mut record = BlockIndexRecord::new(
            7,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Whiteboard),
            0,
        )
        .with_layout_meta(BlockLayoutMeta {
            block_id: 7,
            estimated_height: 200.0,
            measured_height: Some(200.0),
            width_bucket: 860,
            layout_version: 1,
            dirty: false,
        });

        normalize_whiteboard_layout(&mut record);

        assert_eq!(record.layout_meta.effective_height(), 480.0);
        assert_eq!(record.layout_meta.measured_height, None);
        assert!(record.layout_meta.dirty);
    }
}

#[cfg(test)]
mod video_layout_tests {
    use super::*;

    #[test]
    fn reopening_discards_the_legacy_fixed_video_height() {
        let mut record = BlockIndexRecord::new(
            7,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Video),
            0,
        )
        .with_layout_meta(BlockLayoutMeta {
            block_id: 7,
            estimated_height: 360.0,
            measured_height: Some(360.0),
            width_bucket: 800,
            layout_version: 1,
            dirty: false,
        });

        normalize_video_layout(&mut record, None);

        assert_eq!(
            record.layout_meta.effective_height(),
            cditor_core::layout::VIDEO_BLOCK_ESTIMATED_HEIGHT_PX
        );
        assert_eq!(
            record.layout_meta.measured_height,
            Some(cditor_core::layout::VIDEO_BLOCK_ESTIMATED_HEIGHT_PX)
        );
        assert!(!record.layout_meta.dirty);
    }
}

#[cfg(test)]
mod empty_document_tests {
    use super::*;

    #[test]
    fn empty_document_starts_with_a_reserved_document_title_and_body() {
        let runtime = DocumentRuntime::empty();

        assert_eq!(runtime.block_kind(2), Some(RichBlockKind::DocumentTitle));
        assert_eq!(runtime.block_kind(1), Some(RichBlockKind::Paragraph));
        assert_eq!(runtime.document_title_block_id(), Some(2));
        assert!(
            !runtime
                .loaded_payload_records_snapshot()
                .iter()
                .any(|block| matches!(block.kind, RichBlockKind::Heading { level: 1 }))
        );
        assert!(
            runtime
                .block_payload_record(2)
                .unwrap()
                .plain_text()
                .is_empty()
        );
    }

    #[test]
    fn empty_composer_starts_with_a_paragraph_instead_of_a_page_title() {
        let runtime = DocumentRuntime::empty_composer();

        assert_eq!(runtime.block_kind(1), Some(RichBlockKind::Paragraph));
        assert!(
            runtime
                .block_payload_record(1)
                .unwrap()
                .plain_text()
                .is_empty()
        );
    }

    #[test]
    fn demo_does_not_prepopulate_a_body_h1() {
        let runtime = DocumentRuntime::demo();

        assert!(
            !runtime
                .loaded_payload_records_snapshot()
                .iter()
                .any(|block| matches!(block.kind, RichBlockKind::Heading { level: 1 }))
        );
    }
}
