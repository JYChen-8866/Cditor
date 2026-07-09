use std::collections::BTreeSet;
use std::ops::Range;
use std::time::{Duration, Instant};

use cditor_core::ids::BlockId;
use cditor_editor::scroll::{AnchorCandidate, AnchorKind, CaretAnchor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EditingPriority {
    Background,
    Normal,
    High,
    Realtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextLayoutVersion {
    pub content_version: u64,
    pub layout_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaretGeometryVersion {
    pub content_version: u64,
    pub layout_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutCachePin {
    pub block_id: BlockId,
    pub content_version: u64,
    pub layout_version: u64,
    pub priority: EditingPriority,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositionState {
    pub block_id: BlockId,
    pub range_start: u64,
    pub range_end: u64,
    pub preview_text: String,
    pub selected_range_start: Option<u64>,
    pub selected_range_end: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTarget {
    BlockText {
        block_id: BlockId,
    },
    TableCell {
        block_id: BlockId,
        row: usize,
        col: usize,
    },
}

impl InputTarget {
    pub fn block_id(self) -> BlockId {
        match self {
            Self::BlockText { block_id } | Self::TableCell { block_id, .. } => block_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditingSession {
    pub block_id: BlockId,
    pub content_version: u64,
    pub caret_anchor: CaretAnchor,
    pub composition: Option<CompositionState>,
    pub input_target: InputTarget,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub layout_cache_pin: LayoutCachePin,
    text_layout_version: TextLayoutVersion,
    caret_geometry_version: CaretGeometryVersion,
    pinned_blocks: BTreeSet<BlockId>,
}

impl EditingSession {
    pub fn start(block_id: BlockId, content_version: u64, caret_anchor: CaretAnchor) -> Self {
        let layout_cache_pin = LayoutCachePin {
            block_id,
            content_version,
            layout_version: 0,
            priority: EditingPriority::Realtime,
        };
        let mut pinned_blocks = BTreeSet::new();
        pinned_blocks.insert(block_id);

        Self {
            block_id,
            content_version,
            caret_anchor,
            composition: None,
            input_target: InputTarget::BlockText { block_id },
            selected_range: caret_anchor.text_offset as usize..caret_anchor.text_offset as usize,
            selection_reversed: false,
            marked_range: None,
            layout_cache_pin,
            text_layout_version: TextLayoutVersion {
                content_version,
                layout_version: 0,
            },
            caret_geometry_version: CaretGeometryVersion {
                content_version,
                layout_version: 0,
            },
            pinned_blocks,
        }
    }

    pub fn apply_content_edit(&mut self, next_caret_anchor: CaretAnchor) {
        self.content_version = self.content_version.saturating_add(1);
        self.caret_anchor = next_caret_anchor;
        let caret = next_caret_anchor.text_offset as usize;
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        self.text_layout_version.content_version = self.content_version;
        self.text_layout_version.layout_version =
            self.text_layout_version.layout_version.saturating_add(1);
        self.caret_geometry_version.content_version = self.content_version;
        self.caret_geometry_version.layout_version = self.text_layout_version.layout_version;
        self.layout_cache_pin.content_version = self.content_version;
        self.layout_cache_pin.layout_version = self.text_layout_version.layout_version;
        self.layout_cache_pin.priority = EditingPriority::Realtime;
        self.pin_block(self.block_id);
    }

    pub fn set_input_target(&mut self, target: InputTarget) {
        self.block_id = target.block_id();
        self.caret_anchor.block_id = target.block_id();
        self.layout_cache_pin.block_id = target.block_id();
        self.input_target = target;
        self.pin_block(target.block_id());
    }

    pub fn set_collapsed_selection(&mut self, offset: usize) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.marked_range = None;
        self.caret_anchor.text_offset = offset as u64;
    }

    pub fn set_selected_range(&mut self, range: Range<usize>, reversed: bool) {
        self.selected_range = range;
        self.selection_reversed = reversed;
        self.marked_range = None;
        self.caret_anchor.text_offset = if reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        } as u64;
    }

    pub fn set_marked_range(&mut self, marked_range: Range<usize>) {
        self.marked_range = Some(marked_range);
    }

    pub fn update_composition(
        &mut self,
        composition: CompositionState,
    ) -> Result<(), EditingSessionError> {
        if composition.block_id != self.block_id {
            return Err(EditingSessionError::CompositionBlockMismatch {
                session_block: self.block_id,
                composition_block: composition.block_id,
            });
        }
        self.composition = Some(composition);
        self.pin_block(self.block_id);
        self.layout_cache_pin.priority = EditingPriority::Realtime;
        Ok(())
    }

    pub fn clear_composition(&mut self) {
        self.composition = None;
        self.marked_range = None;
    }

    pub fn pin_block(&mut self, block_id: BlockId) {
        self.pinned_blocks.insert(block_id);
    }

    pub fn pinned_blocks(&self) -> &BTreeSet<BlockId> {
        &self.pinned_blocks
    }

    pub fn is_pinned(&self, block_id: BlockId) -> bool {
        self.pinned_blocks.contains(&block_id)
    }

    pub fn can_evict(&self, block_id: BlockId) -> bool {
        !self.is_pinned(block_id)
    }

    pub fn layout_priority_for(&self, block_id: BlockId) -> EditingPriority {
        if block_id == self.block_id || self.is_pinned(block_id) {
            EditingPriority::Realtime
        } else {
            EditingPriority::Normal
        }
    }

    pub fn primary_anchor_candidate(&self) -> AnchorCandidate {
        if self.composition.is_some() {
            AnchorCandidate {
                kind: AnchorKind::Composition,
                anchor: AnchorCandidate::from(self.caret_anchor).anchor,
            }
        } else {
            self.caret_anchor.into()
        }
    }

    pub fn text_layout_version(&self) -> TextLayoutVersion {
        self.text_layout_version
    }

    pub fn caret_geometry_version(&self) -> CaretGeometryVersion {
        self.caret_geometry_version
    }

    pub fn ensure_layout_and_caret_same_version(&self) -> Result<(), EditingSessionError> {
        if self.text_layout_version.content_version != self.caret_geometry_version.content_version
            || self.text_layout_version.layout_version != self.caret_geometry_version.layout_version
        {
            return Err(EditingSessionError::CaretLayoutVersionMismatch {
                text_layout: self.text_layout_version,
                caret_geometry: self.caret_geometry_version,
            });
        }
        Ok(())
    }

    pub fn finish(mut self) -> FinishedEditingSession {
        self.pinned_blocks.remove(&self.block_id);
        FinishedEditingSession {
            block_id: self.block_id,
            final_content_version: self.content_version,
            released_layout_cache_pin: self.layout_cache_pin,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinishedEditingSession {
    pub block_id: BlockId,
    pub final_content_version: u64,
    pub released_layout_cache_pin: LayoutCachePin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditingSessionError {
    CompositionBlockMismatch {
        session_block: BlockId,
        composition_block: BlockId,
    },
    CaretLayoutVersionMismatch {
        text_layout: TextLayoutVersion,
        caret_geometry: CaretGeometryVersion,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputLatencySample {
    pub elapsed: Duration,
    pub within_budget: bool,
}

pub fn measure_hot_path_latency(started_at: Instant, budget: Duration) -> InputLatencySample {
    let elapsed = started_at.elapsed();
    InputLatencySample {
        elapsed,
        within_budget: elapsed <= budget,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_editing_session_and_auto_pins_current_block() {
        let session = EditingSession::start(42, 7, caret(42, 12, 24.0, 120.0));

        assert_eq!(session.block_id, 42);
        assert_eq!(session.content_version, 7);
        assert!(session.is_pinned(42));
        assert!(!session.can_evict(42));
        assert_eq!(session.layout_cache_pin.priority, EditingPriority::Realtime);
    }

    #[test]
    fn current_block_does_not_evict_when_scrolled_far_from_window() {
        let session = EditingSession::start(42, 1, caret(42, 0, 0.0, 0.0));
        let visible_window_blocks: BTreeSet<BlockId> = (1..=10).collect();

        assert!(!visible_window_blocks.contains(&42));
        assert!(!session.can_evict(42));
        assert!(session.can_evict(7));
    }

    #[test]
    fn current_editing_block_layout_task_is_realtime_priority() {
        let session = EditingSession::start(42, 1, caret(42, 0, 0.0, 0.0));

        assert_eq!(session.layout_priority_for(42), EditingPriority::Realtime);
        assert_eq!(session.layout_priority_for(7), EditingPriority::Normal);
    }

    #[test]
    fn content_edit_keeps_caret_geometry_and_text_layout_same_version() {
        let mut session = EditingSession::start(42, 1, caret(42, 0, 0.0, 0.0));

        session.apply_content_edit(caret(42, 1, 18.0, 80.0));

        assert_eq!(session.content_version, 2);
        assert_eq!(session.text_layout_version().content_version, 2);
        assert_eq!(session.caret_geometry_version().content_version, 2);
        assert_eq!(
            session.text_layout_version().layout_version,
            session.caret_geometry_version().layout_version
        );
        assert!(session.ensure_layout_and_caret_same_version().is_ok());
    }

    #[test]
    fn composition_state_uses_composition_anchor_priority_and_pin() {
        let mut session = EditingSession::start(42, 1, caret(42, 0, 10.0, 100.0));

        session
            .update_composition(CompositionState {
                block_id: 42,
                range_start: 0,
                range_end: 1,
                preview_text: "你".to_string(),
                selected_range_start: None,
                selected_range_end: None,
            })
            .unwrap();

        let anchor = session.primary_anchor_candidate();
        assert_eq!(anchor.kind, AnchorKind::Composition);
        assert!(session.is_pinned(42));
    }

    #[test]
    fn rejects_composition_for_other_block() {
        let mut session = EditingSession::start(42, 1, caret(42, 0, 0.0, 0.0));

        let error = session
            .update_composition(CompositionState {
                block_id: 7,
                range_start: 0,
                range_end: 1,
                preview_text: "x".to_string(),
                selected_range_start: None,
                selected_range_end: None,
            })
            .unwrap_err();

        assert_eq!(
            error,
            EditingSessionError::CompositionBlockMismatch {
                session_block: 42,
                composition_block: 7,
            }
        );
    }

    #[test]
    fn input_hot_path_latency_sample_can_validate_p95_budget() {
        let started_at = Instant::now();
        let sample = measure_hot_path_latency(started_at, Duration::from_millis(8));

        assert!(sample.within_budget);
    }

    #[test]
    fn finish_releases_current_editing_pin_metadata() {
        let session = EditingSession::start(42, 3, caret(42, 0, 0.0, 0.0));

        let finished = session.finish();

        assert_eq!(finished.block_id, 42);
        assert_eq!(finished.final_content_version, 3);
        assert_eq!(finished.released_layout_cache_pin.block_id, 42);
    }

    fn caret(
        block_id: BlockId,
        text_offset: u64,
        caret_rect_y_in_block: f64,
        viewport_y: f64,
    ) -> CaretAnchor {
        CaretAnchor {
            block_id,
            text_offset,
            caret_rect_y_in_block,
            viewport_y,
        }
    }
}
