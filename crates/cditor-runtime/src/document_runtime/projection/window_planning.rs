use super::*;

const MAX_RENDER_WINDOW_BLOCKS: usize = 320;
const NORMAL_RENDER_OVERSCAN_VIEWPORTS: f64 = 1.0;
const WARNING_RENDER_OVERSCAN_VIEWPORTS: f64 = 0.5;
// The GPUI surface can place document chrome before the first block (a cover,
// icon, and status notice currently need up to 332px). Keep enough leading
// geometry resident even under critical pressure so the render window can be
// positioned above the viewport instead of exposing that offset as blank UI.
const MIN_RENDER_LEADING_GUARD_PX: f64 = 384.0;
// Maximum per-edge drift (in blocks) that height-correction feedback may move
// the desired window before planning accepts the new ranges. Windows text
// metrics re-measure the edited block with small deltas on every keystroke;
// without hysteresis each delta shifts the block boundaries, changes the
// desired window identity, bumps the generation, and discards in-flight
// payload loads. Scrolls and structure edits bypass this entirely.
const WINDOW_PLAN_HYSTERESIS_BLOCKS: usize = 8;
const WINDOW_PLAN_SCROLL_EPSILON_PX: f64 = 0.5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ViewportWindowRanges {
    pub(super) page_range: Range<usize>,
    pub(super) block_range: Range<usize>,
    pub(super) visible_block_range: Range<usize>,
    /// Blocks physically intersecting the viewport, without overscan. Used by
    /// planning hysteresis to prove a previous window still covers the screen.
    pub(super) viewport_core_range: Range<usize>,
}

impl DocumentRuntime {
    pub(super) fn payload_prefetch_range(&self, render_range: &Range<usize>) -> Range<usize> {
        if render_range.is_empty() {
            return render_range.clone();
        }
        let velocity = self
            .layout
            .window_planner
            .debug_overlay()
            .last_velocity_viewports_per_second;
        let velocity_steps = ((velocity.abs() / 3.0).floor() as usize).min(4);
        let (base, velocity_step) = match self.layout.window_memory_pressure {
            WindowMemoryPressure::Normal => (128usize, 64usize),
            WindowMemoryPressure::Warning => (48usize, 32usize),
            WindowMemoryPressure::Critical => (0usize, 0usize),
        };
        let directional = velocity_steps * velocity_step;
        let before = base + usize::from(velocity < 0.0) * directional;
        let after = base + usize::from(velocity > 0.0) * directional;
        render_range.start.saturating_sub(before)
            ..render_range
                .end
                .saturating_add(after)
                .min(self.document.visible_index.total_visible_count())
    }

    pub(super) fn viewport_window_ranges(&self) -> ViewportWindowRanges {
        let total_visible = self.document.visible_index.total_visible_count();
        if total_visible == 0 {
            return ViewportWindowRanges {
                page_range: 0..0,
                block_range: 0..0,
                visible_block_range: 0..0,
                viewport_core_range: 0..0,
            };
        }
        let current = self
            .target_for_global_offset(self.layout.scroll.global_scroll_top)
            .map(|target| target.block_index)
            .unwrap_or(0)
            .min(total_visible - 1);
        let viewport_end = self
            .layout
            .height_index
            .block_at_offset(
                self.layout.scroll.global_scroll_top + self.layout.scroll.viewport_height,
            )
            .map(|hit| hit.index)
            .unwrap_or(current)
            .min(total_visible - 1);
        let viewport_height = self.layout.scroll.viewport_height.max(1.0);
        let overscan_viewports = match self.layout.window_memory_pressure {
            WindowMemoryPressure::Normal => NORMAL_RENDER_OVERSCAN_VIEWPORTS,
            WindowMemoryPressure::Warning => WARNING_RENDER_OVERSCAN_VIEWPORTS,
            WindowMemoryPressure::Critical => 0.0,
        };
        let overscan_px = viewport_height * overscan_viewports;
        let leading_overscan_px = overscan_px.max(MIN_RENDER_LEADING_GUARD_PX);
        let overscan_start = self
            .layout
            .height_index
            .block_at_offset((self.layout.scroll.global_scroll_top - leading_overscan_px).max(0.0))
            .map(|hit| hit.index)
            .unwrap_or(current)
            .min(current);
        let visible_end = viewport_end
            .saturating_add(1)
            .min(current.saturating_add(MAX_RENDER_WINDOW_BLOCKS))
            .min(total_visible);
        // Preserve the bounded visible core even if corrupt legacy height data
        // maps a viewport across hundreds of zero-height blocks.
        let start = overscan_start.max(visible_end.saturating_sub(MAX_RENDER_WINDOW_BLOCKS));
        let overscan_end = self
            .layout
            .height_index
            .block_at_offset(self.layout.scroll.global_scroll_top + viewport_height + overscan_px)
            .map(|hit| hit.index.saturating_add(1))
            .unwrap_or(visible_end)
            .min(total_visible);
        let end = overscan_end
            .max(visible_end)
            .min(start.saturating_add(MAX_RENDER_WINDOW_BLOCKS))
            .max(start + 1);
        let start_page = self
            .layout
            .page_layout
            .page_for_block_index(start)
            .unwrap_or(0)
            .min(self.layout.page_layout.page_count().saturating_sub(1));
        let end_page = self
            .layout
            .page_layout
            .page_for_block_index(end.saturating_sub(1))
            .unwrap_or(start_page)
            .saturating_add(1)
            .min(self.layout.page_layout.page_count());
        let block_range = start..end;
        let viewport_core_range = current..visible_end.max(current + 1);
        ViewportWindowRanges {
            page_range: start_page..end_page.max(start_page.saturating_add(1)),
            // Readiness is judged on the physical viewport core only. Overscan
            // rows belong to the render window for geometry, but a missing
            // overscan payload must not veto presenting a fully loaded screen
            // — the GUI reserves missing overscan rows silently and the
            // prefetch lane fills them. Scrollbar drags override this with the
            // complete render window (see projection.rs), because a long jump
            // has no adjacent resident content to reserve space with.
            visible_block_range: viewport_core_range.clone(),
            viewport_core_range,
            block_range,
        }
    }

    /// Viewport window ranges with height-correction hysteresis applied.
    ///
    /// Text re-measurement moves block boundaries by fractions of a block on
    /// every keystroke (Windows metrics especially). Re-planning the window
    /// for those micro-shifts changes the desired window identity, bumps the
    /// projection generation, and discards in-flight payload loads while the
    /// user types. When nothing but heights changed — same structure, same
    /// folding, same scroll offset — and the previously planned window still
    /// covers the physical viewport within a small per-edge drift, planning
    /// keeps the previous ranges. Scrolling, scrollbar drags, folding, and
    /// structure edits all bypass the reuse and re-plan immediately.
    pub(super) fn viewport_window_ranges_planned(&mut self) -> ViewportWindowRanges {
        let current = self.viewport_window_ranges();
        let visibility_version = self.document.visible_index.visibility_version;
        let reused = self.layout.scrollbar_drag.is_none() && {
            let window = &self.layout.projection.window;
            window.desired.as_ref().is_some_and(|previous| {
                previous.structure_version == self.document.visible_index.source_structure_version
                    && window.desired_visibility_version == Some(visibility_version)
                    && (previous.presented_scroll_top - self.layout.scroll.global_scroll_top).abs()
                        < WINDOW_PLAN_SCROLL_EPSILON_PX
                    && previous.block_range != current.block_range
                    && range_edge_drift(&previous.block_range, &current.block_range)
                        <= WINDOW_PLAN_HYSTERESIS_BLOCKS
                    && previous.block_range.start <= current.viewport_core_range.start
                    && current.viewport_core_range.end <= previous.block_range.end
            })
        };
        let ranges = if reused {
            let previous = self
                .layout
                .projection
                .window
                .desired
                .as_ref()
                .expect("hysteresis reuse requires a previous desired window");
            ViewportWindowRanges {
                page_range: previous.page_range.clone(),
                block_range: previous.block_range.clone(),
                visible_block_range: previous.visible_block_range.clone(),
                viewport_core_range: current.viewport_core_range,
            }
        } else {
            current
        };
        self.layout.projection.window.desired_visibility_version = Some(visibility_version);
        ranges
    }

    pub(super) fn ensure_demo_payload_window(&mut self, block_range: &Range<usize>) {
        let Some(count) = self.document.demo_payload_count else {
            return;
        };
        if block_range.is_empty() || self.payload_window_covers(block_range) {
            return;
        }

        let total_visible = self.document.visible_index.total_visible_count();
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

        self.document.payload_window = PayloadWindow::new(payload_range.clone());
        self.document.text_models.clear();
        self.document.table_runtimes.clear();
        for payload in payloads {
            let mut payload = normalize_payload_record_for_kind(payload);
            self.sync_table_runtime_from_loaded_record(&mut payload);
            self.document.payload_window.insert_loaded(payload);
        }
        crate::diagnostics::write_stderr(format_args!(
            "[cditor][timing] demo_payload_window range={:?} payloads={} elapsed_ms={:.2}",
            payload_range,
            payload_count,
            start_time.elapsed().as_secs_f64() * 1000.0
        ));
    }

    pub(super) fn block_range_for_page_window(&self, page_range: &Range<usize>) -> Range<usize> {
        let total_visible = self.document.visible_index.total_visible_count();
        let page_count = self.layout.page_layout.page_count();
        if page_range.is_empty() || page_count == 0 || total_visible == 0 {
            return 0..0;
        }

        let start_page = page_range.start.min(page_count);
        let end_page = page_range.end.min(page_count);
        if start_page >= end_page {
            return 0..0;
        }

        let start = self.layout.page_layout.pages[start_page]
            .block_start
            .min(total_visible);
        let end = self.layout.page_layout.pages[end_page - 1]
            .block_end()
            .min(total_visible);
        start..end.max(start)
    }
}

fn range_edge_drift(previous: &Range<usize>, current: &Range<usize>) -> usize {
    previous
        .start
        .abs_diff(current.start)
        .max(previous.end.abs_diff(current.end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_edge_drift_is_the_larger_per_edge_shift() {
        assert_eq!(range_edge_drift(&(10..50), &(10..50)), 0);
        assert_eq!(range_edge_drift(&(10..50), &(12..49)), 2);
        assert_eq!(range_edge_drift(&(10..50), &(4..70)), 20);
    }
}
