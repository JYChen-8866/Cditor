use std::collections::VecDeque;
use std::ops::Range;

use crate::scroll::{AnchorKind, LayoutPx, ScrollPrecision};
use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TraceEventKind {
    PageHeightCorrected,
    AnchorRestored,
    WindowChanged,
    EntityEvicted,
    PinAdded,
    PinRemoved,
    OldRequestDiscarded,
    ScrollbarDragFrozenTotalHeight,
    LayoutTaskDeferred,
    AsyncResultDiscarded,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceEvent {
    pub frame_id: u64,
    pub timestamp_ms: u64,
    pub kind: TraceEventKind,
    pub payload: TraceEventPayload,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraceEventPayload {
    PageHeightCorrected(PageHeightCorrectedEvent),
    AnchorRestored(AnchorRestoredEvent),
    WindowChanged(WindowChangedEvent),
    EntityEvicted(EntityEvictedEvent),
    PinChanged(PinChangedEvent),
    OldRequestDiscarded(OldRequestDiscardedEvent),
    ScrollbarDragFrozenTotalHeight(ScrollbarDragFrozenTotalHeightEvent),
    LayoutTaskDeferred(LayoutTaskDeferredEvent),
    AsyncResultDiscarded(AsyncResultDiscardedEvent),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageHeightCorrectedEvent {
    pub page_index: usize,
    pub old_height: LayoutPx,
    pub new_height: LayoutPx,
    pub correction_delta: LayoutPx,
    pub scroll_jitter_px: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorRestoredEvent {
    pub anchor_kind: AnchorKind,
    pub block_id: BlockId,
    pub before_viewport_y: LayoutPx,
    pub after_viewport_y: LayoutPx,
    pub jitter_px: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowChangedEvent {
    pub previous_page_range: Range<usize>,
    pub next_page_range: Range<usize>,
    pub loaded_pages: Vec<usize>,
    pub placeholder_pages: Vec<usize>,
    pub commit_count_in_frame: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityEvictedEvent {
    pub block_id: BlockId,
    pub pinned: bool,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinChangedEvent {
    pub block_id: BlockId,
    pub reason: PinTraceReason,
    pub pinned_count_after: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinTraceReason {
    Focus,
    Composition,
    SelectionEndpoint,
    DragSource,
    SlashMenu,
    Dirty,
    AsyncTask,
    RecentEdit,
    Resource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OldRequestDiscardedEvent {
    pub request_generation: u64,
    pub current_generation: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarDragFrozenTotalHeightEvent {
    pub frozen_total_height: LayoutPx,
    pub displayed_total_height: LayoutPx,
    pub pending_corrections: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutTaskDeferredEvent {
    pub task_kind: String,
    pub queue_len: usize,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncResultDiscardedEvent {
    pub block_id: Option<BlockId>,
    pub request_generation: u64,
    pub current_generation: u64,
    pub request_content_version: Option<u64>,
    pub current_content_version: Option<u64>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceEventLog {
    capacity: usize,
    events: VecDeque<TraceEvent>,
    dropped_events: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceEventSnapshot {
    pub events: Vec<TraceEvent>,
    pub dropped_events: usize,
    pub page_height_correction_count: usize,
    pub anchor_restore_count: usize,
    pub window_commit_count: usize,
    pub old_request_discard_count: usize,
    pub async_result_discard_count: usize,
    pub max_scroll_jitter_px: f64,
    pub max_anchor_jitter_px: f64,
}

impl TraceEventLog {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            events: VecDeque::new(),
            dropped_events: 0,
        }
    }

    pub fn push(&mut self, event: TraceEvent) {
        if self.events.len() == self.capacity {
            self.events.pop_front();
            self.dropped_events = self.dropped_events.saturating_add(1);
        }
        self.events.push_back(event);
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn snapshot(&self) -> TraceEventSnapshot {
        let events = self.events.iter().cloned().collect::<Vec<_>>();
        TraceEventSnapshot::from_events(events, self.dropped_events)
    }

    pub fn events_for_frame(&self, frame_id: u64) -> Vec<TraceEvent> {
        self.events
            .iter()
            .filter(|event| event.frame_id == frame_id)
            .cloned()
            .collect()
    }

    pub fn jitter_causes_for_frame(&self, frame_id: u64) -> Vec<TraceEvent> {
        self.events
            .iter()
            .filter(|event| event.frame_id == frame_id)
            .filter(|event| {
                matches!(
                    event.kind,
                    TraceEventKind::PageHeightCorrected
                        | TraceEventKind::AnchorRestored
                        | TraceEventKind::WindowChanged
                        | TraceEventKind::OldRequestDiscarded
                        | TraceEventKind::AsyncResultDiscarded
                )
            })
            .cloned()
            .collect()
    }
}

impl Default for TraceEventLog {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl TraceEventSnapshot {
    fn from_events(events: Vec<TraceEvent>, dropped_events: usize) -> Self {
        let mut page_height_correction_count = 0;
        let mut anchor_restore_count = 0;
        let mut window_commit_count = 0;
        let mut old_request_discard_count = 0;
        let mut async_result_discard_count = 0;
        let mut max_scroll_jitter_px = 0.0_f64;
        let mut max_anchor_jitter_px = 0.0_f64;

        for event in &events {
            match &event.payload {
                TraceEventPayload::PageHeightCorrected(payload) => {
                    page_height_correction_count += 1;
                    max_scroll_jitter_px = max_scroll_jitter_px.max(payload.scroll_jitter_px.abs());
                }
                TraceEventPayload::AnchorRestored(payload) => {
                    anchor_restore_count += 1;
                    max_anchor_jitter_px = max_anchor_jitter_px.max(payload.jitter_px.abs());
                }
                TraceEventPayload::WindowChanged(payload) => {
                    window_commit_count += payload.commit_count_in_frame;
                }
                TraceEventPayload::OldRequestDiscarded(_) => old_request_discard_count += 1,
                TraceEventPayload::AsyncResultDiscarded(_) => async_result_discard_count += 1,
                _ => {}
            }
        }

        Self {
            events,
            dropped_events,
            page_height_correction_count,
            anchor_restore_count,
            window_commit_count,
            old_request_discard_count,
            async_result_discard_count,
            max_scroll_jitter_px,
            max_anchor_jitter_px,
        }
    }
}

impl TraceEvent {
    pub fn page_height_corrected(
        frame_id: u64,
        timestamp_ms: u64,
        page_index: usize,
        old_height: LayoutPx,
        new_height: LayoutPx,
        scroll_jitter_px: f64,
    ) -> Self {
        Self {
            frame_id,
            timestamp_ms,
            kind: TraceEventKind::PageHeightCorrected,
            payload: TraceEventPayload::PageHeightCorrected(PageHeightCorrectedEvent {
                page_index,
                old_height,
                new_height,
                correction_delta: new_height - old_height,
                scroll_jitter_px: scroll_jitter_px.abs(),
            }),
        }
    }

    pub fn anchor_restored(
        frame_id: u64,
        timestamp_ms: u64,
        anchor_kind: AnchorKind,
        block_id: BlockId,
        before_viewport_y: LayoutPx,
        after_viewport_y: LayoutPx,
    ) -> Self {
        Self {
            frame_id,
            timestamp_ms,
            kind: TraceEventKind::AnchorRestored,
            payload: TraceEventPayload::AnchorRestored(AnchorRestoredEvent {
                anchor_kind,
                block_id,
                before_viewport_y,
                after_viewport_y,
                jitter_px: (after_viewport_y - before_viewport_y).abs(),
            }),
        }
    }

    pub fn window_changed(
        frame_id: u64,
        timestamp_ms: u64,
        previous_page_range: Range<usize>,
        next_page_range: Range<usize>,
        loaded_pages: Vec<usize>,
        placeholder_pages: Vec<usize>,
        commit_count_in_frame: usize,
    ) -> Self {
        Self {
            frame_id,
            timestamp_ms,
            kind: TraceEventKind::WindowChanged,
            payload: TraceEventPayload::WindowChanged(WindowChangedEvent {
                previous_page_range,
                next_page_range,
                loaded_pages,
                placeholder_pages,
                commit_count_in_frame,
            }),
        }
    }

    pub fn entity_evicted(
        frame_id: u64,
        timestamp_ms: u64,
        block_id: BlockId,
        pinned: bool,
        dirty: bool,
    ) -> Self {
        Self {
            frame_id,
            timestamp_ms,
            kind: TraceEventKind::EntityEvicted,
            payload: TraceEventPayload::EntityEvicted(EntityEvictedEvent {
                block_id,
                pinned,
                dirty,
            }),
        }
    }

    pub fn pin_added(
        frame_id: u64,
        timestamp_ms: u64,
        block_id: BlockId,
        reason: PinTraceReason,
        pinned_count_after: usize,
    ) -> Self {
        pin_changed(
            frame_id,
            timestamp_ms,
            TraceEventKind::PinAdded,
            block_id,
            reason,
            pinned_count_after,
        )
    }

    pub fn pin_removed(
        frame_id: u64,
        timestamp_ms: u64,
        block_id: BlockId,
        reason: PinTraceReason,
        pinned_count_after: usize,
    ) -> Self {
        pin_changed(
            frame_id,
            timestamp_ms,
            TraceEventKind::PinRemoved,
            block_id,
            reason,
            pinned_count_after,
        )
    }

    pub fn old_request_discarded(
        frame_id: u64,
        timestamp_ms: u64,
        request_generation: u64,
        current_generation: u64,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            frame_id,
            timestamp_ms,
            kind: TraceEventKind::OldRequestDiscarded,
            payload: TraceEventPayload::OldRequestDiscarded(OldRequestDiscardedEvent {
                request_generation,
                current_generation,
                reason: reason.into(),
            }),
        }
    }

    pub fn scrollbar_drag_frozen_total_height(
        frame_id: u64,
        timestamp_ms: u64,
        frozen_total_height: LayoutPx,
        displayed_total_height: LayoutPx,
        pending_corrections: usize,
    ) -> Self {
        Self {
            frame_id,
            timestamp_ms,
            kind: TraceEventKind::ScrollbarDragFrozenTotalHeight,
            payload: TraceEventPayload::ScrollbarDragFrozenTotalHeight(
                ScrollbarDragFrozenTotalHeightEvent {
                    frozen_total_height,
                    displayed_total_height,
                    pending_corrections,
                },
            ),
        }
    }

    pub fn layout_task_deferred(
        frame_id: u64,
        timestamp_ms: u64,
        task_kind: impl Into<String>,
        queue_len: usize,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            frame_id,
            timestamp_ms,
            kind: TraceEventKind::LayoutTaskDeferred,
            payload: TraceEventPayload::LayoutTaskDeferred(LayoutTaskDeferredEvent {
                task_kind: task_kind.into(),
                queue_len,
                reason: reason.into(),
            }),
        }
    }

    pub fn async_result_discarded(
        frame_id: u64,
        timestamp_ms: u64,
        block_id: Option<BlockId>,
        request_generation: u64,
        current_generation: u64,
        request_content_version: Option<u64>,
        current_content_version: Option<u64>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            frame_id,
            timestamp_ms,
            kind: TraceEventKind::AsyncResultDiscarded,
            payload: TraceEventPayload::AsyncResultDiscarded(AsyncResultDiscardedEvent {
                block_id,
                request_generation,
                current_generation,
                request_content_version,
                current_content_version,
                reason: reason.into(),
            }),
        }
    }
}

fn pin_changed(
    frame_id: u64,
    timestamp_ms: u64,
    kind: TraceEventKind,
    block_id: BlockId,
    reason: PinTraceReason,
    pinned_count_after: usize,
) -> TraceEvent {
    TraceEvent {
        frame_id,
        timestamp_ms,
        kind,
        payload: TraceEventPayload::PinChanged(PinChangedEvent {
            block_id,
            reason,
            pinned_count_after,
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceScrollStateSample {
    pub frame_id: u64,
    pub global_scroll_top: LayoutPx,
    pub precision: ScrollPrecision,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_log_snapshot_counts_jitter_causes_and_window_commits() {
        let mut log = TraceEventLog::new(32);
        log.push(TraceEvent::page_height_corrected(
            1, 10, 3, 100.0, 120.0, 2.5,
        ));
        log.push(TraceEvent::anchor_restored(
            1,
            11,
            AnchorKind::ViewportTop,
            42,
            100.0,
            101.0,
        ));
        log.push(TraceEvent::window_changed(
            1,
            12,
            0..2,
            1..4,
            vec![1, 2],
            vec![3],
            1,
        ));
        log.push(TraceEvent::entity_evicted(1, 13, 9, false, false));
        log.push(TraceEvent::pin_added(1, 14, 42, PinTraceReason::Focus, 1));
        log.push(TraceEvent::pin_removed(1, 15, 42, PinTraceReason::Focus, 0));

        let snapshot = log.snapshot();

        assert_eq!(snapshot.events.len(), 6);
        assert_eq!(snapshot.page_height_correction_count, 1);
        assert_eq!(snapshot.anchor_restore_count, 1);
        assert_eq!(snapshot.window_commit_count, 1);
        assert_eq!(snapshot.max_scroll_jitter_px, 2.5);
        assert_eq!(snapshot.max_anchor_jitter_px, 1.0);
        assert_eq!(log.jitter_causes_for_frame(1).len(), 3);
    }

    #[test]
    fn old_request_discard_simulation_is_observable() {
        let mut log = TraceEventLog::new(8);
        log.push(TraceEvent::old_request_discarded(
            2,
            20,
            3,
            5,
            "generation mismatch",
        ));
        log.push(TraceEvent::async_result_discarded(
            2,
            21,
            Some(88),
            3,
            5,
            Some(10),
            Some(12),
            "content version mismatch",
        ));

        let snapshot = log.snapshot();

        assert_eq!(snapshot.old_request_discard_count, 1);
        assert_eq!(snapshot.async_result_discard_count, 1);
        assert_eq!(log.events_for_frame(2).len(), 2);
        assert!(
            log.jitter_causes_for_frame(2)
                .iter()
                .any(|event| event.kind == TraceEventKind::AsyncResultDiscarded)
        );
    }

    #[test]
    fn log_is_bounded_and_reports_dropped_events() {
        let mut log = TraceEventLog::new(2);
        log.push(TraceEvent::layout_task_deferred(
            1,
            1,
            "Prefetch",
            10,
            "budget exhausted",
        ));
        log.push(TraceEvent::scrollbar_drag_frozen_total_height(
            1, 2, 10_000.0, 9_900.0, 3,
        ));
        log.push(TraceEvent::entity_evicted(1, 3, 5, false, false));

        let snapshot = log.snapshot();

        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.dropped_events, 1);
        assert_eq!(
            snapshot.events[0].kind,
            TraceEventKind::ScrollbarDragFrozenTotalHeight
        );
        assert_eq!(snapshot.events[1].kind, TraceEventKind::EntityEvicted);
    }
}
