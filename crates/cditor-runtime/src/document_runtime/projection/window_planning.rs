use super::*;

const MAX_RENDER_WINDOW_BLOCKS: usize = 320;
const NORMAL_RENDER_OVERSCAN_VIEWPORTS: f64 = 1.0;
const WARNING_RENDER_OVERSCAN_VIEWPORTS: f64 = 0.5;
// The GPUI surface can place document chrome before the first block (a cover,
// icon, and status notice currently need up to 332px). Keep enough leading
// geometry resident even under critical pressure so the render window can be
// positioned above the viewport instead of exposing that offset as blank UI.
const MIN_RENDER_LEADING_GUARD_PX: f64 = 384.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ViewportWindowRanges {
    pub(super) page_range: Range<usize>,
    pub(super) block_range: Range<usize>,
    pub(super) visible_block_range: Range<usize>,
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
        ViewportWindowRanges {
            page_range: start_page..end_page.max(start_page.saturating_add(1)),
            // The complete bounded render window is the atomic readiness unit.
            // This prevents a committed frame from mixing loaded viewport rows
            // with placeholder overscan rows, without depending on total document
            // size: the range is always capped by MAX_RENDER_WINDOW_BLOCKS.
            visible_block_range: block_range.clone(),
            block_range,
        }
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
