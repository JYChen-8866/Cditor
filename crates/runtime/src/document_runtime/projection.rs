use super::*;

impl DocumentRuntime {
    pub fn block_content_version(&self, block_id: BlockId) -> Option<u64> {
        self.payload_window
            .get(block_id)
            .map(|payload| payload.content_version)
    }

    pub fn block_payload_record(&self, block_id: BlockId) -> Option<BlockPayloadRecord> {
        self.payload_window
            .get(block_id)
            .cloned()
            .map(|payload| self.table_runtime_payload_record(block_id, payload))
            .map(|payload| self.payload_with_composition_preview(block_id, payload))
    }

    pub fn projection_for_window(&self) -> EditorViewProjection {
        let page_range = self.current_page_window();
        let block_range = self.block_range_for_page_window(&page_range);
        self.projection_for_ranges(page_range, block_range)
    }

    pub fn projection_for_window_planned(&mut self) -> EditorViewProjection {
        let total_start = Instant::now();
        self.refresh_ai_session_validity();
        let (page_range, block_range) = if self.demo_payload_count.is_some() {
            self.demo_viewport_window_ranges()
        } else {
            let page_range = self.current_page_window_planned();
            let block_range = self.block_range_for_page_window(&page_range);
            (page_range, block_range)
        };
        self.ensure_demo_payload_window(&block_range);
        let projection = self.projection_for_ranges(page_range, block_range);
        let projection =
            if self.scrollbar_drag.is_some() && projection.render_window.is_placeholder() {
                self.last_successful_projection
                    .clone()
                    .unwrap_or(projection)
            } else {
                if !projection.render_window.is_placeholder() {
                    self.last_successful_projection = Some(projection.clone());
                }
                projection
            };
        log_runtime_timing(
            "runtime.projection_for_window_planned",
            total_start,
            Some(projection.blocks.len()),
        );
        projection
    }

    pub fn projection(&self) -> EditorViewProjection {
        self.projection_for_ranges(
            0..self.page_layout.page_count(),
            0..self.visible_index.total_visible_count(),
        )
    }

    fn demo_viewport_window_ranges(&self) -> (Range<usize>, Range<usize>) {
        let total_visible = self.visible_index.total_visible_count();
        if total_visible == 0 {
            return (0..0, 0..0);
        }
        let current = self
            .target_for_global_offset(self.scroll.global_scroll_top)
            .map(|target| target.block_index)
            .unwrap_or(0)
            .min(total_visible - 1);
        let viewport_end = self
            .height_index
            .block_at_offset(self.scroll.global_scroll_top + self.scroll.viewport_height)
            .map(|hit| hit.index)
            .unwrap_or(current)
            .min(total_visible - 1);
        let overscan = 48usize;
        let max_blocks = 320usize;
        let start = current.saturating_sub(overscan);
        let natural_end = viewport_end.saturating_add(overscan + 1).min(total_visible);
        let end = natural_end
            .min(start.saturating_add(max_blocks))
            .max(start + 1);
        let page = self
            .page_layout
            .page_for_block_index(current)
            .unwrap_or(0)
            .min(self.page_layout.page_count().saturating_sub(1));
        (
            page..page.saturating_add(1).min(self.page_layout.page_count()),
            start..end,
        )
    }

    fn ensure_demo_payload_window(&mut self, block_range: &Range<usize>) {
        let Some(count) = self.demo_payload_count else {
            return;
        };
        if block_range.is_empty() || self.payload_window_covers(block_range) {
            return;
        }

        let total_visible = self.visible_index.total_visible_count();
        let preload = 256usize;
        let start = block_range.start.saturating_sub(preload);
        let end = block_range.end.saturating_add(preload).min(total_visible);
        let payload_range = start..end;
        let start_time = Instant::now();
        let payloads = cditor_core::demo_fixtures::large_mixed_demo_payload_records(
            payload_range.clone(),
            count,
        );
        let payload_count = payloads.len();

        self.payload_window = PayloadWindow::new(payload_range.clone());
        self.text_models.clear();
        self.table_runtimes.clear();
        for payload in payloads {
            let mut payload = normalize_payload_record_for_kind(payload);
            self.sync_table_runtime_from_loaded_record(&mut payload);
            self.payload_window.insert(payload);
        }
        eprintln!(
            "[cditor][timing] demo_payload_window range={:?} payloads={} elapsed_ms={:.2}",
            payload_range,
            payload_count,
            start_time.elapsed().as_secs_f64() * 1000.0
        );
    }

    fn block_range_for_page_window(&self, page_range: &Range<usize>) -> Range<usize> {
        let total_visible = self.visible_index.total_visible_count();
        let page_count = self.page_layout.page_count();
        if page_range.is_empty() || page_count == 0 || total_visible == 0 {
            return 0..0;
        }

        let start_page = page_range.start.min(page_count);
        let end_page = page_range.end.min(page_count);
        if start_page >= end_page {
            return 0..0;
        }

        let start = self.page_layout.pages[start_page]
            .block_start
            .min(total_visible);
        let end = self.page_layout.pages[end_page - 1]
            .block_end()
            .min(total_visible);
        start..end.max(start)
    }

    fn projection_for_ranges(
        &self,
        page_range: Range<usize>,
        block_range: Range<usize>,
    ) -> EditorViewProjection {
        let total_visible_blocks = self.visible_index.total_visible_count();
        let block_start = block_range.start.min(total_visible_blocks);
        let block_end = block_range.end.min(total_visible_blocks).max(block_start);
        let block_range = block_start..block_end;
        if !self.payload_window_covers(&block_range) {
            return self.placeholder_projection_for_ranges(page_range, block_range);
        }
        let block_ids = self.visible_index.visible_block_ids[block_range.clone()].to_vec();
        let local_height_index =
            BlockHeightIndex::new(block_ids.iter().enumerate().map(|(local_index, block_id)| {
                let source_index = self
                    .index
                    .index_of(*block_id)
                    .unwrap_or(block_range.start + local_index);
                HeightEstimate::new(
                    self.index.layout_meta[source_index].effective_height(),
                    HeightConfidence::Historical,
                    4.0,
                )
            }))
            .expect("projection local heights are valid");
        let render_window = RenderWindow::loaded(
            page_range,
            block_range.clone(),
            &block_ids,
            local_height_index,
            1,
        )
        .expect("projection render window is valid");
        let selection_fragments = self
            .document_selection
            .and_then(|selection| selection.normalize(&self.index).ok())
            .and_then(|selection| {
                selection
                    .visible_selection_fragments(
                        block_range.clone(),
                        &self.index,
                        &self.visible_index,
                        |block_id| {
                            self.text_models
                                .get(&block_id)
                                .map(|model| model.len())
                                .unwrap_or(0)
                        },
                    )
                    .ok()
            })
            .unwrap_or_default();
        let selection_ranges = selection_fragments
            .into_iter()
            .map(|fragment| (fragment.block_id, fragment.range))
            .collect::<HashMap<_, _>>();
        let selection_overlay_blocks =
            whole_text_selection_blocks(&block_ids, &selection_ranges, &self.payload_window);
        let blocks = block_ids
            .iter()
            .enumerate()
            .map(|(local_index, block_id)| {
                let visible_index = block_range.start + local_index;
                let source_index = self.index.index_of(*block_id).unwrap_or(visible_index);
                let marked_range = self
                    .active_composition()
                    .filter(|composition| composition.block_id == *block_id)
                    .and_then(|_| self.active_composition_marked_range());
                let payload = self
                    .payload_window
                    .get(*block_id)
                    .cloned()
                    .map(|payload| self.table_runtime_payload_record(*block_id, payload))
                    .map(|payload| self.payload_with_composition_preview(*block_id, payload))
                    .map(BlockPayloadView::Loaded)
                    .unwrap_or(BlockPayloadView::Placeholder {
                        estimated_height: 32.0,
                    });
                let kind = match &payload {
                    BlockPayloadView::Loaded(payload) => payload.kind.clone(),
                    _ => rich_block_kind_from_tag(self.index.kind_tags[source_index]),
                };
                let selection_range = selection_ranges.get(block_id).cloned();
                let mut layout = self.index.layout_meta[source_index];
                if matches!(kind, RichBlockKind::Image)
                    && layout.effective_height() < IMAGE_BLOCK_ESTIMATED_HEIGHT_PX
                {
                    layout.estimated_height = IMAGE_BLOCK_ESTIMATED_HEIGHT_PX;
                    layout.measured_height = None;
                    layout.dirty = true;
                }
                let chrome = self
                    .list_projection_cache
                    .entry(source_index)
                    .map(|entry| {
                        cditor_core::block::BlockChromeSnapshot::from_kind(
                            &kind,
                            entry.list_info,
                            entry.chrome.has_children,
                            entry.chrome.collapsed,
                        )
                    })
                    .unwrap_or_else(cditor_core::block::BlockChromeSnapshot::plain);
                let focused_table_cell = self.focused_table_cell_for_block(*block_id);
                let focused_table_cell_offset = self
                    .focused_table_cell_offset()
                    .filter(|(focused_block_id, _, _, _)| focused_block_id == block_id)
                    .map(|(_, _, _, offset)| offset);
                let table_view = self.table_runtime(*block_id).map(|runtime| {
                    table::table_view_state_from_payload(
                        runtime.table(),
                        focused_table_cell,
                        focused_table_cell_offset,
                        self.table_horizontal_scroll_offset_px(*block_id),
                    )
                });
                if matches!(kind, RichBlockKind::Table) {
                    let (rows, cols) = table_view
                        .as_ref()
                        .map(|view| {
                            (
                                view.table.rows.len(),
                                view.table
                                    .rows
                                    .first()
                                    .map(|row| row.cells.len())
                                    .unwrap_or(0),
                            )
                        })
                        .unwrap_or((0, 0));
                    trace_table(
                        "projection.table",
                        format_args!(
                            "block={} visible_index={visible_index} rows={rows} cols={cols} height={} focused={} focused_cell={:?} focused_cell_offset={:?} payload_loaded={}",
                            block_id,
                            layout.effective_height(),
                            self.focused_block_id() == Some(*block_id),
                            focused_table_cell,
                            focused_table_cell_offset,
                            matches!(payload, BlockPayloadView::Loaded(_))
                        ),
                    );
                }
                ViewBlockSnapshot {
                    block_id: *block_id,
                    visible_index,
                    depth: self.index.depths[source_index],
                    chrome,
                    kind,
                    attrs: BlockAttrs::default(),
                    payload,
                    layout,
                    selected: self.selected_block_ids.contains(block_id),
                    selection_range,
                    selection_overlay: selection_overlay_blocks.contains(block_id),
                    focused: self.focused_block_id() == Some(*block_id),
                    caret_offset: self
                        .editing
                        .as_ref()
                        .filter(|editing| editing.block_id == *block_id)
                        .map(|editing| editing.caret_anchor.text_offset as usize),
                    marked_range,
                    table_view,
                    focused_table_cell,
                    focused_table_cell_offset,
                    pinned: self
                        .editing
                        .as_ref()
                        .is_some_and(|editing| editing.is_pinned(*block_id)),
                    placeholder: false,
                }
            })
            .collect::<Vec<_>>();
        let before_window_height = self
            .height_index
            .offset_of_block(render_window.block_range.start)
            .unwrap_or(0.0);
        let window_height = render_window.height();
        let down_placer_height = self.down_placer_height();
        let after_window_height = (self.scroll_extent_height(self.page_layout.total_height())
            - before_window_height
            - window_height)
            .max(0.0);
        let debug = DebugOverlaySnapshot::from_scroll_state(
            &self.scroll,
            0,
            render_window.page_range.clone(),
        )
        .with_entity_stats(
            blocks.len(),
            blocks.iter().filter(|block| block.pinned).count(),
        );
        EditorViewProjection {
            document_id: self.document_id,
            scroll: self.scroll,
            render_window,
            blocks,
            ai_preview: self.ai_preview_for_block_range(&block_range),
            before_window_height,
            placeholder_window_height: None,
            after_window_height,
            down_placer_height,
            total_visible_blocks,
            debug,
        }
    }

    fn payload_window_covers(&self, block_range: &Range<usize>) -> bool {
        if block_range.is_empty() {
            return true;
        }
        if self.payload_window.block_range.start > block_range.start
            || block_range.end > self.payload_window.block_range.end
        {
            return false;
        }
        block_range.clone().all(|visible_index| {
            self.visible_index
                .id_at_visible_index(visible_index)
                .is_some_and(|block_id| self.payload_window.payloads.contains_key(&block_id))
        })
    }

    fn placeholder_projection_for_ranges(
        &self,
        page_range: Range<usize>,
        block_range: Range<usize>,
    ) -> EditorViewProjection {
        let total_visible_blocks = self.visible_index.total_visible_count();
        let before_window_height = self
            .height_index
            .offset_of_block(block_range.start)
            .unwrap_or(0.0);
        let placeholder_height = self.height_for_page_range(&page_range);
        let render_window = RenderWindow::placeholder(PlaceholderWindow {
            page_range: page_range.clone(),
            block_range,
            height: placeholder_height,
            target_anchor: self
                .target_for_global_offset(self.scroll.global_scroll_top)
                .map(|target| cditor_editor::scroll::ScrollAnchor {
                    block_id: target.block_id,
                    offset_in_block: target.offset_in_block,
                    viewport_y: 0.0,
                }),
        });
        let down_placer_height = self.down_placer_height();
        let after_window_height = (self.scroll_extent_height(self.page_layout.total_height())
            - before_window_height
            - placeholder_height)
            .max(0.0);
        let debug = DebugOverlaySnapshot::from_scroll_state(
            &self.scroll,
            0,
            render_window.page_range.clone(),
        )
        .with_entity_stats(0, 0);
        EditorViewProjection {
            document_id: self.document_id,
            scroll: self.scroll,
            render_window,
            blocks: Vec::new(),
            ai_preview: None,
            before_window_height,
            placeholder_window_height: Some(placeholder_height),
            after_window_height,
            down_placer_height,
            total_visible_blocks,
            debug,
        }
    }

    fn height_for_page_range(&self, page_range: &Range<usize>) -> f64 {
        let page_count = self.page_layout.page_count();
        if page_range.is_empty() || page_count == 0 {
            return 0.0;
        }
        let start = page_range.start.min(page_count);
        let end = page_range.end.min(page_count).max(start);
        self.page_layout.pages[start..end]
            .iter()
            .map(|page| page.height)
            .sum()
    }
}

fn whole_text_selection_blocks(
    block_ids: &[BlockId],
    selection_ranges: &HashMap<BlockId, SelectionRange>,
    payload_window: &PayloadWindow,
) -> HashSet<BlockId> {
    let mut selected = HashSet::new();
    let mut run_start = 0;
    while run_start < block_ids.len() {
        if !selection_ranges.contains_key(&block_ids[run_start]) {
            run_start += 1;
            continue;
        }
        let mut run_end = run_start + 1;
        while run_end < block_ids.len() && selection_ranges.contains_key(&block_ids[run_end]) {
            run_end += 1;
        }
        let run = &block_ids[run_start..run_end];
        if run.len() >= 2
            && run.iter().all(|block_id| {
                let Some(range) = selection_ranges.get(block_id) else {
                    return false;
                };
                match range {
                    SelectionRange::Full => true,
                    SelectionRange::Partial(range) => {
                        payload_window.get(*block_id).is_some_and(|payload| {
                            range.start == 0 && range.end == payload.plain_text().len()
                        })
                    }
                }
            })
        {
            selected.extend(run.iter().copied());
        }
        run_start = run_end;
    }
    selected
}
