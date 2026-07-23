use super::*;

/// Virtual document geometry, measurement convergence, and viewport state.
#[derive(Debug)]
pub(super) struct LayoutState {
    pub(super) height_index: BlockHeightIndex,
    pub(super) page_layout: PageLayoutIndex,
    pub(super) scroll: VirtualScrollState,
    pub(super) table_horizontal_scroll_offsets: HashMap<BlockId, f32>,
    pub(super) payload_window_generation: u64,
    pub(super) window_planner: WindowPlanner,
    pub(super) last_planned_scroll_top: f64,
    pub(super) window_plan_clock_ms: u64,
    pub(super) window_memory_pressure: WindowMemoryPressure,
    pub(super) pending_measured_heights: HashMap<BlockId, PendingMeasuredHeight>,
    pub(super) dirty: bool,
    pub(super) scrollbar_drag: Option<ScrollbarDragSession>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PendingMeasuredHeight {
    pub(super) content_version: u64,
    pub(super) height: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_preserves_height_page_and_scroll_convergence() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            (1..=10)
                .map(|block_id| {
                    BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "")
                })
                .collect(),
            720.0,
        );
        runtime.sync_viewport_height(320.0).unwrap();
        let version = runtime.block_payload_record(1).unwrap().content_version;

        assert!(runtime.queue_measured_height(1, version, 96.0).unwrap());
        assert!(runtime.flush_pending_height_corrections().unwrap());

        assert_eq!(
            runtime.layout.height_index.total_height(),
            runtime.layout.page_layout.total_height()
        );
        assert_eq!(
            runtime.layout.scroll.model_total_height,
            runtime.scroll_extent_height(runtime.layout.page_layout.total_height())
        );
        assert!(runtime.layout.dirty);
        assert!(runtime.layout.pending_measured_heights.is_empty());

        let mut accumulator = ScrollAccumulator::default();
        accumulator.push_input(
            cditor_viewport::scroll::ScrollInput {
                delta_y: 48.0,
                mode: cditor_viewport::scroll::ScrollDeltaMode::Pixel,
                phase: cditor_viewport::scroll::ScrollPhase::Changed,
                device: cditor_viewport::scroll::ScrollDevice::Trackpad,
                timestamp: Instant::now(),
            },
            runtime.viewport_height(),
        );
        assert!(
            runtime
                .apply_scroll_accumulator_frame(&mut accumulator)
                .unwrap()
        );
        assert_eq!(runtime.global_scroll_top(), 48.0);
    }
}
