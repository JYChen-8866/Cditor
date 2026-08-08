use super::*;
use cditor_editor_protocol::projection::ProjectionRequest;

mod payload_readiness;
mod placeholder;
mod window_planning;

const SCROLLBAR_FOREGROUND_GUARD_BLOCKS: usize = 2;

impl DocumentRuntime {
    pub fn block_content_version(&self, block_id: BlockId) -> Option<u64> {
        self.document
            .payload_window
            .get(block_id)
            .map(|payload| payload.content_version)
    }

    pub fn block_kind(&self, block_id: BlockId) -> Option<RichBlockKind> {
        self.document
            .payload_window
            .get(block_id)
            .map(|payload| payload.kind.clone())
            .or_else(|| {
                self.document
                    .index
                    .index_of(block_id)
                    .map(|index| rich_block_kind_from_tag(self.document.index.kind_tags[index]))
            })
    }

    pub fn block_payload_record(&self, block_id: BlockId) -> Option<BlockPayloadRecord> {
        let payload = self.document.payload_window.get_shared(block_id)?.clone();
        let payload = if self.document.table_runtimes.contains_key(&block_id) {
            Arc::new(self.table_runtime_payload_record(block_id, payload.as_ref().clone()))
        } else {
            payload
        };
        Some(Arc::unwrap_or_clone(
            self.payload_with_composition_preview(block_id, payload),
        ))
    }

    pub fn projection_for_window(&self) -> EditorViewProjection {
        let page_range = self.current_page_window();
        let block_range = self.block_range_for_page_window(&page_range);
        self.projection_for_ranges(page_range, block_range)
    }

    pub fn projection_for_window_planned(&mut self) -> EditorViewProjection {
        let total_start = Instant::now();
        self.refresh_ai_session_validity();
        let mut layout_prefetch_page_range = self.current_page_window_planned();
        // The GUI always projects a viewport-sized block window. Page windows are
        // still maintained for layout and scroll geometry, but must not decide how
        // many block entities are created in a frame. This is the same bounded
        // path used by the synthetic 100k fixture and by resident/PG documents.
        let desired_ranges = self.viewport_window_ranges();
        self.preheat_page_local_cache(desired_ranges.page_range.clone());
        layout_prefetch_page_range.start = layout_prefetch_page_range
            .start
            .min(desired_ranges.page_range.start);
        layout_prefetch_page_range.end = layout_prefetch_page_range
            .end
            .max(desired_ranges.page_range.end);
        let payload_prefetch_block_range = self.payload_prefetch_range(&desired_ranges.block_range);
        self.ensure_demo_payload_window(&payload_prefetch_block_range);
        let payload_visible_block_range = if self.layout.scrollbar_drag.is_some() {
            // A scrollbar drag can jump thousands of blocks between frames. Treat
            // the complete render window as foreground data so an atomic window
            // commit never exposes placeholder overscan around a loaded core.
            desired_ranges.block_range.clone()
        } else {
            desired_ranges.visible_block_range.clone()
        };

        let desired = ProjectionWindowTarget {
            structure_version: self.document.visible_index.source_structure_version,
            page_range: desired_ranges.page_range.clone(),
            block_range: desired_ranges.block_range,
            visible_block_range: payload_visible_block_range.clone(),
            presented_scroll_top: self.layout.scroll.global_scroll_top,
        };
        let desired_ready = self.payloads_resident_for(&desired.visible_block_range);
        let payload_prefetch_resident = desired_ready
            && self.payloads_resident_for_prefetch_cached(&payload_prefetch_block_range);
        let desired_failed = self.payload_terminal_failure_for(&desired.visible_block_range);
        let stable = self.layout.projection.publication.stable.clone();
        let decision = self.layout.projection.reconcile(
            desired,
            desired_ready,
            stable.as_ref().is_some_and(|stable| {
                stable.target.structure_version
                    == self.document.visible_index.source_structure_version
                    && stable.target.block_range == stable.projection.render_window.block_range
            }),
            desired_failed,
        );
        let mut projection = match decision {
            ProjectionWindowDecision::Stable(stable_target) => {
                self.document.payload_window.block_range = stable_target.block_range.clone();
                if desired_ready {
                    let mut projection = self.projection_for_ranges(
                        stable_target.page_range.clone(),
                        stable_target.block_range.clone(),
                    );
                    projection.scroll.global_scroll_top = stable_target.presented_scroll_top;
                    let frame_id = self.layout.projection.publication.next_frame_id;
                    self.layout.projection.publication.next_frame_id += 1;
                    self.layout.projection.publication.stable = Some(StableProjectionSnapshot {
                        frame_id,
                        target: stable_target,
                        projection: projection.clone(),
                    });
                    projection
                } else if let Some(snapshot) = stable.as_ref() {
                    snapshot.projection.clone()
                } else {
                    self.placeholder_projection_for_ranges_with_visible_core(
                        stable_target.page_range,
                        stable_target.block_range,
                        stable_target.visible_block_range,
                    )
                }
            }
            ProjectionWindowDecision::ColdPlaceholder(desired)
            | ProjectionWindowDecision::FailedTarget {
                target: desired,
                stable: None,
            } => self.placeholder_projection_for_ranges_with_visible_core(
                desired.page_range,
                desired.block_range,
                desired.visible_block_range,
            ),
            ProjectionWindowDecision::FailedTarget {
                target,
                stable: Some(stable_target),
            } => {
                self.document.payload_window.block_range = stable_target.block_range.clone();
                let mut projection = stable
                    .as_ref()
                    .map(|snapshot| snapshot.projection.clone())
                    .unwrap_or_else(|| {
                        self.placeholder_projection_for_ranges_with_visible_core(
                            stable_target.page_range,
                            stable_target.block_range,
                            stable_target.visible_block_range,
                        )
                    });
                projection.scroll.global_scroll_top = stable_target.presented_scroll_top;
                projection.placeholder_window_failure =
                    self.payload_failure_view_for(&target.visible_block_range);
                projection.placeholder_window_error = projection
                    .placeholder_window_failure
                    .as_ref()
                    .map(|failure| failure.message.clone());
                projection
            }
        };
        projection.payload_visible_block_range = payload_visible_block_range;
        projection.payload_prefetch_block_range = payload_prefetch_block_range;
        projection.payload_prefetch_resident = payload_prefetch_resident;
        projection.layout_prefetch_page_range = layout_prefetch_page_range;
        log_runtime_timing(
            "runtime.projection_for_window_planned",
            total_start,
            Some(projection.blocks.len()),
        );
        projection
    }

    /// Foreground payload range for the viewport's current interaction target.
    ///
    /// Scrollbar dragging uses the complete render window because a large jump
    /// has no adjacent resident window to provide real overscan content. Other
    /// interactions retain the smaller physical viewport core.
    pub fn current_foreground_payload_range(&self) -> Range<usize> {
        let ranges = self.viewport_window_ranges();
        if self.layout.scrollbar_drag.is_some() {
            let total_visible = self.document.visible_index.total_visible_count();
            ranges
                .block_range
                .start
                .saturating_sub(SCROLLBAR_FOREGROUND_GUARD_BLOCKS)
                ..ranges
                    .block_range
                    .end
                    .saturating_add(SCROLLBAR_FOREGROUND_GUARD_BLOCKS)
                    .min(total_visible)
        } else {
            ranges.visible_block_range
        }
    }

    pub fn projection(&mut self, request: ProjectionRequest) -> EditorViewProjection {
        let mut projection = self.projection_for_window_planned();
        projection.viewport_revision = request.viewport_revision;
        if !request.include_diagnostics {
            projection.debug = DebugOverlaySnapshot::from_scroll_state(
                &projection.scroll,
                0,
                projection.render_window.page_range.clone(),
            );
        }
        projection
    }

    #[cfg(test)]
    pub(crate) fn full_projection_for_tests(&self) -> EditorViewProjection {
        self.projection_for_ranges(
            0..self.layout.page_layout.page_count(),
            0..self.document.visible_index.total_visible_count(),
        )
    }

    fn projection_for_ranges(
        &self,
        page_range: Range<usize>,
        block_range: Range<usize>,
    ) -> EditorViewProjection {
        let total_visible_blocks = self.document.visible_index.total_visible_count();
        let block_start = block_range.start.min(total_visible_blocks);
        let block_end = block_range.end.min(total_visible_blocks).max(block_start);
        let block_range = block_start..block_end;
        // Keep resident blocks on screen while PostgreSQL fills the newly
        // exposed edge of a scrolling window. Replacing the whole projection
        // because one overscan block is missing makes every wheel tick flash a
        // full-page skeleton. A full window placeholder is reserved for cold
        // jumps where none of the target blocks are resident yet.
        if !self.payload_window_covers(&block_range) && !self.payload_window_has_any(&block_range) {
            return self.placeholder_projection_for_ranges(page_range, block_range);
        }
        let block_ids = self.document.visible_index.visible_block_ids[block_range.clone()].to_vec();
        let local_height_index = BlockHeightIndex::new(block_range.clone().map(|visible_index| {
            self.cached_or_global_height_estimate(visible_index)
                .expect("projection block range is covered by the global height index")
        }))
        .expect("projection local heights are valid");
        let render_window = RenderWindow::loaded(
            page_range.clone(),
            block_range.clone(),
            &block_ids,
            local_height_index,
            1,
        )
        .expect("projection render window is valid");
        let selection_fragments =
            self.selection
                .document_selection
                .and_then(|selection| selection.normalize(&self.document.index).ok())
                .and_then(|selection| {
                    selection
                        .visible_selection_fragments(
                            block_range.clone(),
                            &self.document.index,
                            &self.document.visible_index,
                            |block_id| {
                                self.document
                                    .text_models
                                    .get(&block_id)
                                    .map(|model| model.len())
                                    .or_else(|| {
                                        self.document.payload_window.get(block_id).and_then(
                                            |record| editable_text_len_for_payload(&record.payload),
                                        )
                                    })
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
        let selection_overlay_blocks = whole_text_selection_blocks(
            &block_ids,
            &selection_ranges,
            &self.document.payload_window,
        );
        let blocks = block_ids
            .iter()
            .enumerate()
            .map(|(local_index, block_id)| {
                let visible_index = block_range.start + local_index;
                let source_index = self.document.index.index_of(*block_id).unwrap_or(visible_index);
                let marked_range = self
                    .active_composition()
                    .filter(|composition| composition.block_id == *block_id)
                    .and_then(|_| self.active_composition_marked_range());
                let payload = self.document
                    .payload_window
                    .get_shared(*block_id)
                    .cloned()
                    .map(|payload| {
                        if self.document.table_runtimes.contains_key(block_id) {
                            return Arc::new(self.table_runtime_payload_record(
                                *block_id,
                                payload.as_ref().clone(),
                            ));
                        }
                        if matches!(payload.kind, RichBlockKind::Table)
                            && !matches!(&payload.payload, BlockPayload::Table(table) if table::table_has_cells(table))
                        {
                            // Storage adapters may return a stale text payload
                            // for a block whose persisted kind is already Table.
                            // Repair only this exceptional path; normal tables
                            // keep their shared resident allocation.
                            return Arc::new(normalize_payload_record_for_kind(
                                payload.as_ref().clone(),
                            ));
                        }
                        payload
                    })
                    .map(|payload| self.payload_with_composition_preview(*block_id, payload))
                    .map(BlockPayloadView::Loaded)
                    .unwrap_or(BlockPayloadView::Placeholder {
                        estimated_height: 32.0,
                    });
                let placeholder = matches!(payload, BlockPayloadView::Placeholder { .. });
                let kind = match &payload {
                    BlockPayloadView::Loaded(payload) => payload.kind.clone(),
                    _ => rich_block_kind_from_tag(self.document.index.kind_tags[source_index]),
                };
                let selection_range = selection_ranges.get(block_id).cloned();
                let mut layout = self.document.index.layout_meta[source_index];
                if matches!(kind, RichBlockKind::Image)
                    && layout.effective_height() < IMAGE_BLOCK_ESTIMATED_HEIGHT_PX
                {
                    layout.estimated_height = IMAGE_BLOCK_ESTIMATED_HEIGHT_PX;
                    layout.measured_height = None;
                    layout.dirty = true;
                }
                if matches!(kind, RichBlockKind::Table)
                    && let BlockPayloadView::Loaded(record) = &payload
                    && let BlockPayload::Table(table) = &record.payload
                {
                    let table_height =
                        f64::from(table::table_payload_projected_height_px(table));
                    if layout.effective_height() < table_height
                        || layout.measured_height != Some(table_height)
                    {
                        layout.estimated_height = table_height;
                        layout.measured_height = Some(table_height);
                        layout.dirty = false;
                    }
                }
                let chrome = self.document
                    .list_projection_cache
                    .entry(source_index)
                    .map(|entry| {
                        let has_foldable_content = self.document
                            .visible_index
                            .has_foldable_content(&self.document.index, *block_id);
                        cditor_core::block::BlockChromeSnapshot::from_kind(
                            &kind,
                            entry.list_info,
                            has_foldable_content,
                            self.document.visible_index.is_folded(*block_id),
                        )
                    })
                    .unwrap_or_else(cditor_core::block::BlockChromeSnapshot::plain);
                let focused_table_cell = self.focused_table_cell_for_block(*block_id);
                let focused_table_cell_offset = self
                    .focused_table_cell_offset()
                    .filter(|(focused_block_id, _, _, _)| focused_block_id == block_id)
                    .map(|(_, _, _, offset)| offset);
                let focused_table_cell_affinity = self
                    .focused_table_cell_text_position()
                    .filter(|(focused_block_id, _, _, _, _)| focused_block_id == block_id)
                    .map(|(_, _, _, _, affinity)| affinity);
                let focused_table_cell_selection_range = self
                    .focused_table_cell_selection_state()
                    .filter(|(focused_block_id, _, _, _, _, _)| focused_block_id == block_id)
                    .map(|(_, _, _, range, _, _)| range);
                let table_payload = self
                    .table_runtime(*block_id)
                    .map(|runtime| {
                        TablePayloadSnapshot::from_shared_table(runtime.shared_table().clone())
                    })
                    .or_else(|| match &payload {
                        BlockPayloadView::Loaded(record) => {
                            TablePayloadSnapshot::from_record(record.clone())
                        }
                        _ => None,
                    })
                    .map(|table| self.table_payload_with_composition_preview(*block_id, table));
                let table_view = table_payload.map(|table| {
                    table::table_view_state_from_payload(
                        table,
                        focused_table_cell,
                        focused_table_cell_offset,
                        focused_table_cell_affinity,
                        focused_table_cell_selection_range,
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
                let mut attrs = self.document.block_attrs.get(block_id).cloned().unwrap_or_default();
                attrs.folded = self.document.visible_index.is_folded(*block_id);
                ViewBlockSnapshot {
                    block_id: *block_id,
                    visible_index,
                    depth: self.document.index.depths[source_index],
                    chrome,
                    kind,
                    attrs,
                    payload,
                    layout,
                    selected: self.selection.selected_block_ids.contains(block_id),
                    selection_range,
                    selection_overlay: selection_overlay_blocks.contains(block_id),
                    focused: self.focused_block_id() == Some(*block_id),
                    caret_offset: self.editing
                        .session
                        .as_ref()
                        .filter(|editing| editing.block_id == *block_id)
                        .map(EditingSession::focus_offset),
                    caret_affinity: self
                        .caret_position_for_block(*block_id)
                        .map(|position| position.affinity),
                    marked_range,
                    table_view,
                    focused_table_cell,
                    focused_table_cell_offset,
                    pinned: self.editing
                        .session
                        .as_ref()
                        .is_some_and(|editing| editing.is_pinned(*block_id)),
                    placeholder,
                }
            })
            .collect::<Vec<_>>();
        let before_window_height = self
            .layout
            .height_index
            .offset_of_block(render_window.block_range.start)
            .unwrap_or(0.0);
        let window_height = render_window.height();
        let down_placer_height = self.down_placer_height();
        let after_window_height = (self
            .scroll_extent_height(self.layout.page_layout.total_height())
            - before_window_height
            - window_height)
            .max(0.0);
        let debug = DebugOverlaySnapshot::from_scroll_state(
            &self.layout.scroll,
            0,
            render_window.page_range.clone(),
        )
        .with_entity_stats(
            blocks.len(),
            blocks.iter().filter(|block| block.pinned).count(),
        );
        EditorViewProjection {
            document_id: self.document_id,
            viewport_revision: 0,
            window_generation: self.layout.projection.generation(),
            scroll: self.layout.scroll,
            render_window,
            payload_visible_block_range: block_range.clone(),
            payload_prefetch_block_range: block_range.clone(),
            payload_prefetch_resident: self.payloads_resident_for(&block_range),
            layout_prefetch_page_range: page_range,
            blocks,
            ai_preview: self.ai_preview_for_block_range(&block_range),
            before_window_height,
            placeholder_window_height: None,
            placeholder_window_error: None,
            placeholder_window_failure: None,
            after_window_height,
            down_placer_height,
            total_visible_blocks,
            debug,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_facade_preserves_request_identity_and_bounds_the_window() {
        let mut runtime = DocumentRuntime::empty();
        let projection = runtime.projection(ProjectionRequest {
            viewport_revision: 42,
            include_diagnostics: false,
        });

        assert_eq!(projection.viewport_revision, 42);
        assert_eq!(projection.document_id, runtime.document_id);
        assert!(projection.blocks.len() <= 320);
        assert!(projection.debug.page_boundaries.is_empty());
    }
}
