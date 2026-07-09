use std::collections::HashMap;

use crate::scroll::anchor::{AnchorFrame, AnchorGlobalOffsetResolver, AnchorKind};
use crate::scroll::virtual_scroll::{LayoutPx, ScrollOrigin, VirtualScrollState};
use cditor_core::document::VisibleDocumentIndex;
use cditor_core::ids::BlockId;
use cditor_core::layout::{BlockHeightIndex, PageLayoutIndex};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightChange {
    pub block_id: BlockId,
    pub old_height: LayoutPx,
    pub new_height: LayoutPx,
    pub delta: LayoutPx,
}

impl HeightChange {
    pub fn new(block_id: BlockId, old_height: LayoutPx, new_height: LayoutPx) -> Self {
        Self {
            block_id,
            old_height,
            new_height,
            delta: new_height - old_height,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeightChangeQueue {
    order: Vec<BlockId>,
    changes: HashMap<BlockId, HeightChange>,
}

impl HeightChangeQueue {
    pub fn new() -> Self {
        Self {
            order: Vec::new(),
            changes: HashMap::new(),
        }
    }

    pub fn push(&mut self, change: HeightChange) {
        if let Some(existing) = self.changes.get_mut(&change.block_id) {
            existing.new_height = change.new_height;
            existing.delta = existing.new_height - existing.old_height;
            return;
        }
        self.order.push(change.block_id);
        self.changes.insert(change.block_id, change);
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn drain_coalesced(&mut self) -> Vec<HeightChange> {
        let mut drained = Vec::with_capacity(self.changes.len());
        for block_id in self.order.drain(..) {
            if let Some(change) = self.changes.remove(&block_id) {
                drained.push(change);
            }
        }
        drained
    }
}

impl Default for HeightChangeQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightErrorBudget {
    pub viewport_max_error_px: LayoutPx,
    pub page_max_error_px: LayoutPx,
    pub total_height_max_error_ratio: f64,
    pub correction_apply_threshold_px: LayoutPx,
    pub displayed_total_converge_px_per_frame: LayoutPx,
}

impl Default for HeightErrorBudget {
    fn default() -> Self {
        Self {
            viewport_max_error_px: 1.0,
            page_max_error_px: 4.0,
            total_height_max_error_ratio: 0.001,
            correction_apply_threshold_px: 1.0,
            displayed_total_converge_px_per_frame: 512.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightCorrectionConfig {
    pub error_budget: HeightErrorBudget,
}

impl Default for HeightCorrectionConfig {
    fn default() -> Self {
        Self {
            error_budget: HeightErrorBudget::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightCorrectionDebugOverlay {
    pub last_correction_delta: LayoutPx,
    pub coalesced_subpixel_delta: LayoutPx,
    pub suppressed_round_trip_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeightErrorAccumulator {
    pending: HashMap<BlockId, HeightChange>,
    last_committed_height: HashMap<BlockId, LayoutPx>,
    debug: HeightCorrectionDebugOverlay,
}

impl HeightErrorAccumulator {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            last_committed_height: HashMap::new(),
            debug: HeightCorrectionDebugOverlay {
                last_correction_delta: 0.0,
                coalesced_subpixel_delta: 0.0,
                suppressed_round_trip_count: 0,
            },
        }
    }

    pub fn push_with_budget(
        &mut self,
        change: HeightChange,
        budget: HeightErrorBudget,
    ) -> Option<HeightChange> {
        if let Some(last) = self.last_committed_height.get(&change.block_id).copied() {
            if (change.new_height - last).abs() < budget.correction_apply_threshold_px {
                self.debug.suppressed_round_trip_count += 1;
                self.debug.coalesced_subpixel_delta += change.delta;
                return None;
            }
        }

        let pending = self.pending.entry(change.block_id).or_insert(change);
        pending.new_height = change.new_height;
        pending.delta = pending.new_height - pending.old_height;
        self.debug.coalesced_subpixel_delta = pending.delta;

        if pending.delta.abs() >= budget.correction_apply_threshold_px {
            let emitted = self
                .pending
                .remove(&change.block_id)
                .expect("pending change exists");
            self.last_committed_height
                .insert(emitted.block_id, emitted.new_height);
            self.debug.last_correction_delta = emitted.delta;
            self.debug.coalesced_subpixel_delta = 0.0;
            Some(emitted)
        } else {
            None
        }
    }

    pub fn debug_overlay(&self) -> HeightCorrectionDebugOverlay {
        self.debug
    }
}

impl Default for HeightErrorAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameScrollContext {
    pub frame_id: u64,
    pub primary_origin: ScrollOrigin,
    pub primary_anchor_kind: Option<AnchorKind>,
    pub pending_height_changes: Vec<HeightChange>,
    pub applied_scroll_correction: bool,
}

impl FrameScrollContext {
    pub fn new(frame_id: u64, primary_origin: ScrollOrigin) -> Self {
        Self {
            frame_id,
            primary_origin,
            primary_anchor_kind: None,
            pending_height_changes: Vec::new(),
            applied_scroll_correction: false,
        }
    }
}

pub trait LoadedPageLayoutUpdater {
    fn update_loaded_block_height(&mut self, block_id: BlockId, new_height: LayoutPx);
}

pub struct HeightCorrectionPipeline<'a> {
    pub visible_index: &'a VisibleDocumentIndex,
    pub block_height_index: &'a mut BlockHeightIndex,
    pub page_layout_index: &'a mut PageLayoutIndex,
    pub virtual_scroll: &'a mut VirtualScrollState,
    pub loaded_page_layout: Option<&'a mut dyn LoadedPageLayoutUpdater>,
    pub config: HeightCorrectionConfig,
}

impl HeightCorrectionPipeline<'_> {
    pub fn apply_frame_end(
        &mut self,
        queue: &mut HeightChangeQueue,
        anchor_frame: &mut AnchorFrame,
        context: &mut FrameScrollContext,
    ) -> HeightCorrectionFrameResult {
        let changes = queue.drain_coalesced();
        if changes.is_empty() {
            converge_displayed_total_height(self.virtual_scroll, self.config);
            context.primary_anchor_kind = anchor_frame.primary_kind();
            return HeightCorrectionFrameResult {
                applied_changes: 0,
                anchor_restore_applied: false,
                scroll_top_before: self.virtual_scroll.global_scroll_top,
                scroll_top_after: self.virtual_scroll.global_scroll_top,
                model_total_height: self.virtual_scroll.model_total_height,
                displayed_total_height: self.virtual_scroll.displayed_total_height,
            };
        }

        let scroll_top_before = self.virtual_scroll.global_scroll_top;
        context.pending_height_changes = changes.clone();
        context.primary_anchor_kind = anchor_frame.primary_kind();
        let anchor_block_index = anchor_frame.primary().and_then(|candidate| {
            self.visible_index
                .visible_index_of(candidate.anchor.block_id)
        });
        let mut should_restore_anchor = false;
        let mut page_delta: HashMap<usize, LayoutPx> = HashMap::new();

        let mut applied_changes = 0usize;
        for change in &changes {
            if change.delta.abs() < self.config.error_budget.correction_apply_threshold_px {
                continue;
            }
            let Some(block_index) = self.visible_index.visible_index_of(change.block_id) else {
                continue;
            };
            if let Some(anchor_index) = anchor_block_index {
                if block_index <= anchor_index {
                    should_restore_anchor = true;
                }
            }
            let _ = self
                .block_height_index
                .update_height(block_index, change.new_height);
            if let Some(page_index) = self.page_layout_index.page_for_block_index(block_index) {
                *page_delta.entry(page_index).or_insert(0.0) += change.delta;
            }
            if let Some(loaded) = self.loaded_page_layout.as_deref_mut() {
                loaded.update_loaded_block_height(change.block_id, change.new_height);
            }
            applied_changes += 1;
        }

        for (page_index, delta) in page_delta {
            if let Some(page) = self.page_layout_index.pages.get(page_index).copied() {
                let _ = self
                    .page_layout_index
                    .update_page_height(page_index, page.height + delta);
            }
        }

        let model_total_height = self.block_height_index.total_height();
        let _ = self
            .virtual_scroll
            .set_model_total_height(model_total_height);
        converge_displayed_total_height(self.virtual_scroll, self.config);

        if should_restore_anchor {
            let resolver = VisibleHeightAnchorResolver {
                visible_index: self.visible_index,
                block_height_index: self.block_height_index,
            };
            if let Some(restored) = anchor_frame.restore_once(&resolver) {
                let _ = self.virtual_scroll.scroll_to_global_offset(
                    restored.restored_global_scroll_top,
                    ScrollOrigin::ProgrammaticVirtualScroll,
                );
                context.applied_scroll_correction = true;
            }
        }

        HeightCorrectionFrameResult {
            applied_changes,
            anchor_restore_applied: context.applied_scroll_correction,
            scroll_top_before,
            scroll_top_after: self.virtual_scroll.global_scroll_top,
            model_total_height: self.virtual_scroll.model_total_height,
            displayed_total_height: self.virtual_scroll.displayed_total_height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightCorrectionFrameResult {
    pub applied_changes: usize,
    pub anchor_restore_applied: bool,
    pub scroll_top_before: LayoutPx,
    pub scroll_top_after: LayoutPx,
    pub model_total_height: LayoutPx,
    pub displayed_total_height: LayoutPx,
}

struct VisibleHeightAnchorResolver<'a> {
    visible_index: &'a VisibleDocumentIndex,
    block_height_index: &'a BlockHeightIndex,
}

impl AnchorGlobalOffsetResolver for VisibleHeightAnchorResolver<'_> {
    fn global_offset_of_block(&self, block_id: BlockId) -> Option<LayoutPx> {
        let block_index = self.visible_index.visible_index_of(block_id)?;
        self.block_height_index.offset_of_block(block_index)
    }
}

fn converge_displayed_total_height(
    virtual_scroll: &mut VirtualScrollState,
    config: HeightCorrectionConfig,
) {
    let current = virtual_scroll.displayed_total_height;
    let target = virtual_scroll.model_total_height;
    let delta = target - current;
    let max_step = config
        .error_budget
        .displayed_total_converge_px_per_frame
        .max(0.0);
    if delta.abs() <= max_step || max_step == 0.0 {
        virtual_scroll.displayed_total_height = target;
    } else {
        virtual_scroll.displayed_total_height = current + delta.signum() * max_step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scroll::{AnchorCandidate, AnchorKind, ScrollAnchor};
    use cditor_core::document::{BlockIndexRecord, DocumentIndex, VisibleDocumentIndex};
    use cditor_core::layout::{HeightConfidence, HeightEstimate, PagePolicy};
    use std::collections::HashMap;

    const PARAGRAPH: u16 = 1;

    #[test]
    fn coalesces_multiple_changes_for_same_block() {
        let mut queue = HeightChangeQueue::new();
        queue.push(HeightChange::new(1, 10.0, 12.0));
        queue.push(HeightChange::new(1, 12.0, 15.0));

        let changes = queue.drain_coalesced();

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0], HeightChange::new(1, 10.0, 15.0));
    }

    #[test]
    fn frame_end_updates_block_page_loaded_and_model_height_once() {
        let (visible, mut block_heights, mut pages) = sample_indexes();
        let mut virtual_scroll =
            VirtualScrollState::new(100.0, block_heights.total_height()).unwrap();
        let mut loaded = MockLoadedPageLayout::default();
        let mut queue = HeightChangeQueue::new();
        queue.push(HeightChange::new(2, 20.0, 25.0));
        let mut anchor = AnchorFrame::new(1);
        anchor.offer(candidate(AnchorKind::ViewportTop, 1, 0.0, 0.0));
        let mut context = FrameScrollContext::new(1, ScrollOrigin::UserWheel);

        let result = HeightCorrectionPipeline {
            visible_index: &visible,
            block_height_index: &mut block_heights,
            page_layout_index: &mut pages,
            virtual_scroll: &mut virtual_scroll,
            loaded_page_layout: Some(&mut loaded),
            config: HeightCorrectionConfig::default(),
        }
        .apply_frame_end(&mut queue, &mut anchor, &mut context);

        assert_eq!(result.applied_changes, 1);
        assert_eq!(block_heights.heights[1], 25.0);
        assert_eq!(pages.total_height(), 155.0);
        assert_eq!(virtual_scroll.model_total_height, 155.0);
        assert_eq!(loaded.updated.get(&2), Some(&25.0));
    }

    #[test]
    fn viewport_below_height_change_does_not_modify_scroll_top() {
        let (visible, mut block_heights, mut pages) = sample_indexes();
        let mut virtual_scroll =
            VirtualScrollState::new(100.0, block_heights.total_height()).unwrap();
        virtual_scroll
            .scroll_to_global_offset(30.0, ScrollOrigin::UserWheel)
            .unwrap();
        let mut queue = HeightChangeQueue::new();
        queue.push(HeightChange::new(5, 50.0, 80.0));
        let mut anchor = AnchorFrame::new(1);
        anchor.offer(candidate(AnchorKind::ViewportTop, 2, 0.0, 0.0));
        let mut context = FrameScrollContext::new(1, ScrollOrigin::UserWheel);

        let result = HeightCorrectionPipeline {
            visible_index: &visible,
            block_height_index: &mut block_heights,
            page_layout_index: &mut pages,
            virtual_scroll: &mut virtual_scroll,
            loaded_page_layout: None,
            config: HeightCorrectionConfig::default(),
        }
        .apply_frame_end(&mut queue, &mut anchor, &mut context);

        assert!(!result.anchor_restore_applied);
        assert_eq!(result.scroll_top_before, result.scroll_top_after);
        assert_eq!(virtual_scroll.global_scroll_top, 30.0);
    }

    #[test]
    fn change_above_anchor_restores_anchor_once_at_frame_end() {
        let (visible, mut block_heights, mut pages) = sample_indexes();
        let mut virtual_scroll =
            VirtualScrollState::new(100.0, block_heights.total_height()).unwrap();
        virtual_scroll
            .scroll_to_global_offset(60.0, ScrollOrigin::UserWheel)
            .unwrap();
        let mut queue = HeightChangeQueue::new();
        queue.push(HeightChange::new(1, 10.0, 20.0));
        queue.push(HeightChange::new(2, 20.0, 30.0));
        let mut anchor = AnchorFrame::new(2);
        anchor.offer(candidate(AnchorKind::Caret, 4, 5.0, 20.0));
        let mut context = FrameScrollContext::new(2, ScrollOrigin::UserWheel);

        let result = HeightCorrectionPipeline {
            visible_index: &visible,
            block_height_index: &mut block_heights,
            page_layout_index: &mut pages,
            virtual_scroll: &mut virtual_scroll,
            loaded_page_layout: None,
            config: HeightCorrectionConfig::default(),
        }
        .apply_frame_end(&mut queue, &mut anchor, &mut context);

        assert!(result.anchor_restore_applied);
        assert_eq!(anchor.restore_count(), 1);
        assert_eq!(context.primary_anchor_kind, Some(AnchorKind::Caret));
        assert_eq!(virtual_scroll.global_scroll_top, 65.0);
    }

    #[test]
    fn displayed_total_height_converges_per_frame_instead_of_jumping() {
        let (visible, mut block_heights, mut pages) = sample_indexes();
        let mut virtual_scroll =
            VirtualScrollState::new(100.0, block_heights.total_height()).unwrap();
        virtual_scroll.displayed_total_height = 150.0;
        let mut queue = HeightChangeQueue::new();
        queue.push(HeightChange::new(5, 50.0, 1050.0));
        let mut anchor = AnchorFrame::new(1);
        anchor.offer(candidate(AnchorKind::ViewportTop, 1, 0.0, 0.0));
        let mut context = FrameScrollContext::new(1, ScrollOrigin::UserWheel);

        let result = HeightCorrectionPipeline {
            visible_index: &visible,
            block_height_index: &mut block_heights,
            page_layout_index: &mut pages,
            virtual_scroll: &mut virtual_scroll,
            loaded_page_layout: None,
            config: HeightCorrectionConfig {
                error_budget: HeightErrorBudget {
                    displayed_total_converge_px_per_frame: 100.0,
                    ..HeightErrorBudget::default()
                },
            },
        }
        .apply_frame_end(&mut queue, &mut anchor, &mut context);

        assert_eq!(result.model_total_height, 1150.0);
        assert_eq!(result.displayed_total_height, 250.0);
    }

    #[test]
    fn subpixel_corrections_are_coalesced_until_threshold() {
        let budget = HeightErrorBudget::default();
        let mut accumulator = HeightErrorAccumulator::new();

        assert_eq!(
            accumulator.push_with_budget(HeightChange::new(1, 10.0, 10.2), budget),
            None
        );
        assert_eq!(
            accumulator.push_with_budget(HeightChange::new(1, 10.2, 10.7), budget),
            None
        );
        let emitted = accumulator
            .push_with_budget(HeightChange::new(1, 10.7, 11.6), budget)
            .unwrap();

        assert_eq!(emitted.old_height, 10.0);
        assert_eq!(emitted.new_height, 11.6);
        assert!((emitted.delta - 1.6).abs() < 1e-9);
        assert!((accumulator.debug_overlay().last_correction_delta - 1.6).abs() < 1e-9);
    }

    #[test]
    fn rounded_unrounded_height_round_trip_is_suppressed() {
        let budget = HeightErrorBudget::default();
        let mut accumulator = HeightErrorAccumulator::new();
        assert!(
            accumulator
                .push_with_budget(HeightChange::new(1, 100.0, 101.2), budget)
                .is_some()
        );

        assert_eq!(
            accumulator.push_with_budget(HeightChange::new(1, 101.2, 101.6), budget),
            None
        );

        assert_eq!(accumulator.debug_overlay().suppressed_round_trip_count, 1);
    }

    #[test]
    fn offscreen_measure_chaos_updates_model_but_displayed_total_converges() {
        let (visible, mut block_heights, mut pages) = sample_indexes();
        let mut virtual_scroll =
            VirtualScrollState::new(100.0, block_heights.total_height()).unwrap();
        virtual_scroll.displayed_total_height = 150.0;
        let mut queue = HeightChangeQueue::new();
        queue.push(HeightChange::new(1, 10.0, 11.0));
        queue.push(HeightChange::new(2, 20.0, 24.0));
        queue.push(HeightChange::new(3, 30.0, 32.0));
        let mut anchor = AnchorFrame::new(11);
        anchor.offer(candidate(AnchorKind::ViewportTop, 5, 0.0, 0.0));
        let mut context = FrameScrollContext::new(11, ScrollOrigin::UserWheel);

        let result = HeightCorrectionPipeline {
            visible_index: &visible,
            block_height_index: &mut block_heights,
            page_layout_index: &mut pages,
            virtual_scroll: &mut virtual_scroll,
            loaded_page_layout: None,
            config: HeightCorrectionConfig {
                error_budget: HeightErrorBudget {
                    displayed_total_converge_px_per_frame: 2.0,
                    ..HeightErrorBudget::default()
                },
            },
        }
        .apply_frame_end(&mut queue, &mut anchor, &mut context);

        assert_eq!(result.model_total_height, 157.0);
        assert_eq!(result.displayed_total_height, 152.0);
        assert!(result.anchor_restore_applied);
        assert_eq!(anchor.restore_count(), 1);
    }

    #[test]
    fn below_threshold_frame_changes_do_not_trigger_anchor_restore() {
        let (visible, mut block_heights, mut pages) = sample_indexes();
        let mut virtual_scroll =
            VirtualScrollState::new(100.0, block_heights.total_height()).unwrap();
        let mut queue = HeightChangeQueue::new();
        queue.push(HeightChange::new(1, 10.0, 10.9));
        let mut anchor = AnchorFrame::new(12);
        anchor.offer(candidate(AnchorKind::Caret, 3, 0.0, 10.0));
        let mut context = FrameScrollContext::new(12, ScrollOrigin::UserWheel);

        let result = HeightCorrectionPipeline {
            visible_index: &visible,
            block_height_index: &mut block_heights,
            page_layout_index: &mut pages,
            virtual_scroll: &mut virtual_scroll,
            loaded_page_layout: None,
            config: HeightCorrectionConfig::default(),
        }
        .apply_frame_end(&mut queue, &mut anchor, &mut context);

        assert_eq!(result.applied_changes, 0);
        assert!(!result.anchor_restore_applied);
        assert_eq!(anchor.restore_count(), 0);
    }

    #[test]
    fn height_correction_chaos_test_limits_anchor_restore_count_per_frame() {
        let (visible, mut block_heights, mut pages) = sample_indexes();
        let mut virtual_scroll =
            VirtualScrollState::new(100.0, block_heights.total_height()).unwrap();
        let mut queue = HeightChangeQueue::new();
        for i in 0..100 {
            let block_id = (i % 5) + 1;
            queue.push(HeightChange::new(
                block_id,
                block_id as f64 * 10.0,
                20.0 + i as f64,
            ));
        }
        let mut anchor = AnchorFrame::new(9);
        anchor.offer(candidate(AnchorKind::Composition, 3, 10.0, 30.0));
        let mut context = FrameScrollContext::new(9, ScrollOrigin::UserWheel);

        let result = HeightCorrectionPipeline {
            visible_index: &visible,
            block_height_index: &mut block_heights,
            page_layout_index: &mut pages,
            virtual_scroll: &mut virtual_scroll,
            loaded_page_layout: None,
            config: HeightCorrectionConfig::default(),
        }
        .apply_frame_end(&mut queue, &mut anchor, &mut context);

        assert_eq!(result.applied_changes, 5);
        assert_eq!(anchor.restore_count(), 1);
        assert!(context.applied_scroll_correction);
    }

    fn sample_indexes() -> (VisibleDocumentIndex, BlockHeightIndex, PageLayoutIndex) {
        let records: Vec<_> = (1..=5)
            .map(|id| BlockIndexRecord::new(id, None, 0, PARAGRAPH, 0))
            .collect();
        let document = DocumentIndex::new(1, records, 1).unwrap();
        let visible = VisibleDocumentIndex::from_document_index(&document);
        let block_heights = BlockHeightIndex::new([
            HeightEstimate::new(10.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(20.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(30.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(40.0, HeightConfidence::Exact, 0.0),
            HeightEstimate::new(50.0, HeightConfidence::Exact, 0.0),
        ])
        .unwrap();
        let pages = PageLayoutIndex::from_block_height_index(
            &block_heights,
            cditor_core::layout::PagePolicy {
                max_blocks: 5,
                target_height: 1_000.0,
                ..PagePolicy::default()
            },
        )
        .unwrap();
        (visible, block_heights, pages)
    }

    fn candidate(
        kind: AnchorKind,
        block_id: BlockId,
        offset_in_block: LayoutPx,
        viewport_y: LayoutPx,
    ) -> AnchorCandidate {
        AnchorCandidate {
            kind,
            anchor: ScrollAnchor {
                block_id,
                offset_in_block,
                viewport_y,
            },
        }
    }

    #[derive(Default)]
    struct MockLoadedPageLayout {
        updated: HashMap<BlockId, LayoutPx>,
    }

    impl LoadedPageLayoutUpdater for MockLoadedPageLayout {
        fn update_loaded_block_height(&mut self, block_id: BlockId, new_height: LayoutPx) {
            self.updated.insert(block_id, new_height);
        }
    }
}
