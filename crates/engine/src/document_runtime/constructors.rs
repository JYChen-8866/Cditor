use super::*;

impl DocumentRuntime {
    pub fn empty() -> Self {
        let mut document = RichTextDocument::empty(1);
        document.push_root_block(RichBlockRecord::paragraph(1, ""));
        Self::from_rich_text_document(document, 720.0)
    }

    pub fn demo() -> Self {
        let mut document = RichTextDocument::empty(1);
        document.push_root_block(RichBlockRecord::heading(1, 1, "CDitor V2"));
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

        runtime.demo_payload_count = Some(count);
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

    pub fn from_rich_text_document(document: RichTextDocument, viewport_height: f64) -> Self {
        Self::from_index_records(
            document.id,
            document.index_records(),
            document.payload_records(),
            document.structure_version,
            viewport_height,
        )
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
        records: Vec<BlockIndexRecord>,
        payloads: Vec<BlockPayloadRecord>,
        structure_version: u64,
        viewport_height: f64,
        payload_window_range: Range<usize>,
        page_policy: PagePolicy,
    ) -> Self {
        let record_count = records.len();
        let start = Instant::now();
        let height_estimates = records
            .iter()
            .map(|record| {
                HeightEstimate::new(
                    record.layout_meta.estimated_height,
                    HeightConfidence::Historical,
                    4.0,
                )
            })
            .collect::<Vec<_>>();
        log_runtime_timing("runtime.height_estimates", start, Some(record_count));

        let start = Instant::now();
        let index = DocumentIndex::new(document_id, records, structure_version)
            .expect("document index is valid");
        log_runtime_timing("runtime.document_index", start, Some(record_count));

        let start = Instant::now();
        let list_projection_cache = ListProjectionCache::build(&index);
        log_runtime_timing("runtime.list_projection_cache", start, Some(record_count));

        let start = Instant::now();
        let visible_index = VisibleDocumentIndex::from_document_index(&index);
        log_runtime_timing("runtime.visible_index", start, Some(record_count));

        let start = Instant::now();
        let height_index = BlockHeightIndex::new(height_estimates).expect("demo heights are valid");
        log_runtime_timing("runtime.height_index", start, Some(record_count));

        let start = Instant::now();
        let page_layout = PageLayoutIndex::from_block_height_index(&height_index, page_policy)
            .expect("demo pages are valid");
        log_runtime_timing("runtime.page_layout", start, Some(page_layout.page_count()));
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
            payload_window.insert(payload);
        }

        Self {
            document_id,
            index,
            visible_index,
            height_index,
            page_layout,
            scroll,
            editing: None,
            payload_window,
            table_runtimes,
            text_models,
            selected_block_ids: HashSet::new(),
            list_projection_cache,
            document_selection: None,
            focused_text_selection: None,
            focused_table_cell: None,
            undo_stacks: HashMap::new(),
            redo_stacks: HashMap::new(),
            structure_undo_stack: Vec::new(),
            structure_redo_stack: Vec::new(),
            paste_undo_stack: Vec::new(),
            paste_redo_stack: Vec::new(),
            undo_events: Vec::new(),
            redo_events: Vec::new(),
            pending_structure_transactions: Vec::new(),
            next_transaction_id: 1,
            hot_path: SingleCharInputHotPath::default(),
            payload_window_generation: 0,
            window_planner: WindowPlanner::new(1, 2, WindowPlannerPolicy::default()),
            last_planned_scroll_top: 0.0,
            window_plan_clock_ms: 0,
            pending_measured_heights: HashMap::new(),
            layout_dirty: false,
            scrollbar_drag: None,
            last_successful_projection: None,
            demo_payload_count: None,
        }
    }
}
