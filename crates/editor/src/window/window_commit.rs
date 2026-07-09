use std::collections::BTreeSet;
use std::ops::Range;

use crate::scroll::VirtualScrollTarget;
use crate::window::{RenderWindow, RenderWindowError};
use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowLoadState {
    CurrentStable,
    PreparingNext,
    PlaceholderShown,
    ReadyToSwap,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageWindowRequest {
    pub generation: u64,
    pub page_range: Range<usize>,
    pub target: VirtualScrollTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProtectedWindowPins {
    pub focus_block: Option<BlockId>,
    pub composition_block: Option<BlockId>,
    pub selection_endpoint_blocks: BTreeSet<BlockId>,
}

impl ProtectedWindowPins {
    pub fn protected_blocks(&self) -> BTreeSet<BlockId> {
        let mut blocks = self.selection_endpoint_blocks.clone();
        if let Some(block_id) = self.focus_block {
            blocks.insert(block_id);
        }
        if let Some(block_id) = self.composition_block {
            blocks.insert(block_id);
        }
        blocks
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowCommitCoordinator {
    pub state: WindowLoadState,
    pub current_generation: u64,
    pub stable_window: Option<RenderWindow>,
    pub displayed_placeholder: Option<RenderWindow>,
    pending_request: Option<PageWindowRequest>,
    prepared_next: Option<RenderWindow>,
    anchor_restore_used_for_generation: Option<u64>,
}

impl WindowCommitCoordinator {
    pub fn new(initial_window: Option<RenderWindow>) -> Self {
        Self {
            state: WindowLoadState::CurrentStable,
            current_generation: 0,
            stable_window: initial_window,
            displayed_placeholder: None,
            pending_request: None,
            prepared_next: None,
            anchor_restore_used_for_generation: None,
        }
    }

    pub fn request_window(
        &mut self,
        page_range: Range<usize>,
        target: VirtualScrollTarget,
    ) -> PageWindowRequest {
        self.current_generation = self.current_generation.saturating_add(1);
        let request = PageWindowRequest {
            generation: self.current_generation,
            page_range,
            target,
        };
        self.pending_request = Some(request.clone());
        self.prepared_next = None;
        self.displayed_placeholder = None;
        self.state = WindowLoadState::PreparingNext;
        request
    }

    pub fn show_placeholder(
        &mut self,
        generation: u64,
        placeholder: RenderWindow,
    ) -> Result<WindowCommitEvent, WindowCommitError> {
        self.ensure_current_generation(generation)?;
        if !placeholder.is_placeholder() {
            return Err(WindowCommitError::PlaceholderExpected);
        }
        self.displayed_placeholder = Some(placeholder);
        self.state = WindowLoadState::PlaceholderShown;
        Ok(WindowCommitEvent::PlaceholderShown { generation })
    }

    pub fn prepare_loaded_window(
        &mut self,
        generation: u64,
        window: RenderWindow,
    ) -> Result<WindowCommitEvent, WindowCommitError> {
        self.ensure_current_generation(generation)?;
        if window.is_placeholder() {
            return Err(WindowCommitError::LoadedWindowExpected);
        }
        self.prepared_next = Some(window);
        self.state = WindowLoadState::ReadyToSwap;
        Ok(WindowCommitEvent::ReadyToSwap { generation })
    }

    pub fn fail_request(
        &mut self,
        generation: u64,
    ) -> Result<WindowCommitEvent, WindowCommitError> {
        self.ensure_current_generation(generation)?;
        self.prepared_next = None;
        self.displayed_placeholder = None;
        self.pending_request = None;
        self.state = WindowLoadState::CurrentStable;
        Ok(WindowCommitEvent::LoadFailedKeptStable { generation })
    }

    pub fn atomic_swap(
        &mut self,
        generation: u64,
        protected_pins: &ProtectedWindowPins,
    ) -> Result<SwapOutcome, WindowCommitError> {
        self.ensure_current_generation(generation)?;
        if self.state != WindowLoadState::ReadyToSwap {
            return Err(WindowCommitError::NotReadyToSwap(self.state));
        }

        let next = self
            .prepared_next
            .take()
            .ok_or(WindowCommitError::MissingPreparedWindow)?;
        let old = self.stable_window.replace(next);
        self.displayed_placeholder = None;
        self.pending_request = None;
        self.state = WindowLoadState::CurrentStable;

        let retained_protected_blocks =
            retained_protected_blocks(old.as_ref(), self.stable_window.as_ref(), protected_pins);
        let should_restore_anchor = self.anchor_restore_used_for_generation != Some(generation);
        self.anchor_restore_used_for_generation = Some(generation);

        Ok(SwapOutcome {
            generation,
            should_restore_anchor,
            retained_protected_blocks,
        })
    }

    pub fn visible_window(&self) -> Option<&RenderWindow> {
        self.displayed_placeholder
            .as_ref()
            .or(self.stable_window.as_ref())
    }

    pub fn trace_frame(&self) -> WindowCommitTraceFrame {
        WindowCommitTraceFrame {
            state: self.state,
            generation: self.current_generation,
            has_stable_window: self.stable_window.is_some(),
            has_placeholder: self.displayed_placeholder.is_some(),
            has_prepared_next: self.prepared_next.is_some(),
        }
    }

    fn ensure_current_generation(&self, generation: u64) -> Result<(), WindowCommitError> {
        if generation == self.current_generation {
            Ok(())
        } else {
            Err(WindowCommitError::StaleGeneration {
                current: self.current_generation,
                received: generation,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapOutcome {
    pub generation: u64,
    pub should_restore_anchor: bool,
    pub retained_protected_blocks: BTreeSet<BlockId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowCommitEvent {
    PlaceholderShown { generation: u64 },
    ReadyToSwap { generation: u64 },
    LoadFailedKeptStable { generation: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowCommitTraceFrame {
    pub state: WindowLoadState,
    pub generation: u64,
    pub has_stable_window: bool,
    pub has_placeholder: bool,
    pub has_prepared_next: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowCommitError {
    StaleGeneration { current: u64, received: u64 },
    PlaceholderExpected,
    LoadedWindowExpected,
    NotReadyToSwap(WindowLoadState),
    MissingPreparedWindow,
    RenderWindow(RenderWindowError),
}

impl From<RenderWindowError> for WindowCommitError {
    fn from(error: RenderWindowError) -> Self {
        Self::RenderWindow(error)
    }
}

fn retained_protected_blocks(
    old: Option<&RenderWindow>,
    next: Option<&RenderWindow>,
    pins: &ProtectedWindowPins,
) -> BTreeSet<BlockId> {
    let mut retained = BTreeSet::new();
    for block_id in pins.protected_blocks() {
        let in_next = next.is_some_and(|window| window.entities.contains_key(&block_id));
        let in_old = old.is_some_and(|window| window.entities.contains_key(&block_id));
        if in_next || in_old {
            retained.insert(block_id);
        }
    }
    retained
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scroll::{ScrollPrecision, VirtualScrollTarget};
    use cditor_core::layout::{BlockHeightIndex, HeightConfidence, HeightEstimate};

    #[test]
    fn fast_remote_scroll_discards_stale_generations() {
        let initial = loaded_window(0..1, 0..2, &[1, 2], 1).unwrap();
        let mut coordinator = WindowCommitCoordinator::new(Some(initial));
        let request_10 = coordinator.request_window(10..11, target(10_000.0));
        let request_40 = coordinator.request_window(40..41, target(40_000.0));
        let request_80 = coordinator.request_window(80..81, target(80_000.0));

        assert!(matches!(
            coordinator.prepare_loaded_window(
                request_10.generation,
                loaded_window(10..11, 20..22, &[20, 21], 2).unwrap()
            ),
            Err(WindowCommitError::StaleGeneration { .. })
        ));
        assert!(matches!(
            coordinator.prepare_loaded_window(
                request_40.generation,
                loaded_window(40..41, 80..82, &[80, 81], 3).unwrap()
            ),
            Err(WindowCommitError::StaleGeneration { .. })
        ));
        assert!(
            coordinator
                .prepare_loaded_window(
                    request_80.generation,
                    loaded_window(80..81, 160..162, &[160, 161], 4).unwrap()
                )
                .is_ok()
        );
    }

    #[test]
    fn delayed_page_load_keeps_stable_window_and_can_show_placeholder() {
        let initial = loaded_window(0..1, 0..2, &[1, 2], 1).unwrap();
        let mut coordinator = WindowCommitCoordinator::new(Some(initial));
        let request = coordinator.request_window(50..51, target(50_000.0));
        let placeholder = RenderWindow::placeholder(crate::window::PlaceholderWindow {
            page_range: 50..51,
            block_range: 100..110,
            height: 1_000.0,
            target_anchor: None,
        });

        coordinator
            .show_placeholder(request.generation, placeholder)
            .unwrap();

        let trace = coordinator.trace_frame();
        assert_eq!(trace.state, WindowLoadState::PlaceholderShown);
        assert!(trace.has_stable_window);
        assert!(trace.has_placeholder);
        assert!(coordinator.visible_window().unwrap().is_placeholder());
    }

    #[test]
    fn load_failure_removes_placeholder_and_keeps_current_stable() {
        let initial = loaded_window(0..1, 0..2, &[1, 2], 1).unwrap();
        let mut coordinator = WindowCommitCoordinator::new(Some(initial.clone()));
        let request = coordinator.request_window(5..6, target(5_000.0));
        coordinator
            .show_placeholder(
                request.generation,
                RenderWindow::placeholder(crate::window::PlaceholderWindow {
                    page_range: 5..6,
                    block_range: 10..12,
                    height: 200.0,
                    target_anchor: None,
                }),
            )
            .unwrap();

        coordinator.fail_request(request.generation).unwrap();

        assert_eq!(coordinator.state, WindowLoadState::CurrentStable);
        assert!(coordinator.displayed_placeholder.is_none());
        assert_eq!(coordinator.stable_window, Some(initial));
    }

    #[test]
    fn ready_to_swap_atomic_swap_restores_anchor_once() {
        let initial = loaded_window(0..1, 0..2, &[1, 2], 1).unwrap();
        let mut coordinator = WindowCommitCoordinator::new(Some(initial));
        let request = coordinator.request_window(1..2, target(100.0));
        coordinator
            .prepare_loaded_window(
                request.generation,
                loaded_window(1..2, 2..4, &[3, 4], 2).unwrap(),
            )
            .unwrap();

        let first = coordinator
            .atomic_swap(request.generation, &ProtectedWindowPins::default())
            .unwrap();

        assert_eq!(coordinator.state, WindowLoadState::CurrentStable);
        assert!(first.should_restore_anchor);
        assert_eq!(coordinator.stable_window.as_ref().unwrap().page_range, 1..2);
    }

    #[test]
    fn swap_frame_trace_never_reports_half_loaded_window_as_visible() {
        let initial = loaded_window(0..1, 0..2, &[1, 2], 1).unwrap();
        let mut coordinator = WindowCommitCoordinator::new(Some(initial));
        let request = coordinator.request_window(9..10, target(9_000.0));

        let preparing_trace = coordinator.trace_frame();
        assert_eq!(preparing_trace.state, WindowLoadState::PreparingNext);
        assert!(preparing_trace.has_stable_window);
        assert!(!preparing_trace.has_prepared_next);

        coordinator
            .prepare_loaded_window(
                request.generation,
                loaded_window(9..10, 18..20, &[18, 19], 2).unwrap(),
            )
            .unwrap();
        let ready_trace = coordinator.trace_frame();
        assert_eq!(ready_trace.state, WindowLoadState::ReadyToSwap);
        assert!(ready_trace.has_stable_window);
        assert!(ready_trace.has_prepared_next);
    }

    #[test]
    fn protected_focus_composition_selection_blocks_are_retained_across_swap() {
        let initial = loaded_window(0..1, 0..3, &[1, 2, 3], 1).unwrap();
        let mut coordinator = WindowCommitCoordinator::new(Some(initial));
        let request = coordinator.request_window(1..2, target(100.0));
        coordinator
            .prepare_loaded_window(
                request.generation,
                loaded_window(1..2, 3..6, &[3, 4, 5], 2).unwrap(),
            )
            .unwrap();
        let pins = ProtectedWindowPins {
            focus_block: Some(2),
            composition_block: Some(3),
            selection_endpoint_blocks: BTreeSet::from([5]),
        };

        let outcome = coordinator.atomic_swap(request.generation, &pins).unwrap();

        assert_eq!(outcome.retained_protected_blocks, BTreeSet::from([2, 3, 5]));
    }

    fn loaded_window(
        page_range: Range<usize>,
        block_range: Range<usize>,
        block_ids: &[BlockId],
        generation: u64,
    ) -> Result<RenderWindow, RenderWindowError> {
        let heights = BlockHeightIndex::new(
            block_ids
                .iter()
                .map(|_| HeightEstimate::new(24.0, HeightConfidence::Exact, 0.0)),
        )
        .unwrap();
        RenderWindow::loaded(page_range, block_range, block_ids, heights, generation)
    }

    fn target(global_scroll_top: f64) -> VirtualScrollTarget {
        VirtualScrollTarget {
            block_id: None,
            block_index: None,
            offset_in_block: 0.0,
            global_scroll_top,
            precision: ScrollPrecision::Estimated,
        }
    }
}
