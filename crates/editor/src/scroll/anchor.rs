use crate::scroll::{ScrollAnchor, virtual_scroll::LayoutPx};
use cditor_core::ids::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnchorKind {
    ViewportTop,
    ExplicitScrollTarget,
    SelectionFocus,
    Caret,
    Composition,
}

impl AnchorKind {
    pub const fn priority(self) -> u8 {
        match self {
            Self::ViewportTop => 0,
            Self::ExplicitScrollTarget => 1,
            Self::SelectionFocus => 2,
            Self::Caret => 3,
            Self::Composition => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorCandidate {
    pub kind: AnchorKind,
    pub anchor: ScrollAnchor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretAnchor {
    pub block_id: BlockId,
    pub text_offset: u64,
    pub caret_rect_y_in_block: LayoutPx,
    pub viewport_y: LayoutPx,
}

impl From<CaretAnchor> for AnchorCandidate {
    fn from(anchor: CaretAnchor) -> Self {
        Self {
            kind: AnchorKind::Caret,
            anchor: ScrollAnchor {
                block_id: anchor.block_id,
                offset_in_block: anchor.caret_rect_y_in_block,
                viewport_y: anchor.viewport_y,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnchorFrame {
    pub frame_id: u64,
    primary: Option<AnchorCandidate>,
    restore_count: u8,
}

impl AnchorFrame {
    pub const fn new(frame_id: u64) -> Self {
        Self {
            frame_id,
            primary: None,
            restore_count: 0,
        }
    }

    pub fn offer(&mut self, candidate: AnchorCandidate) -> bool {
        let should_replace = self
            .primary
            .is_none_or(|primary| candidate.kind.priority() > primary.kind.priority());
        if should_replace {
            self.primary = Some(candidate);
        }
        should_replace
    }

    pub fn primary(&self) -> Option<AnchorCandidate> {
        self.primary
    }

    pub fn primary_kind(&self) -> Option<AnchorKind> {
        self.primary.map(|candidate| candidate.kind)
    }

    pub fn restore_once(
        &mut self,
        resolver: &impl AnchorGlobalOffsetResolver,
    ) -> Option<AnchorRestoreResult> {
        if self.restore_count > 0 {
            return None;
        }
        let primary = self.primary?;
        let block_top = resolver.global_offset_of_block(primary.anchor.block_id)?;
        let restored_global_scroll_top =
            block_top + primary.anchor.offset_in_block - primary.anchor.viewport_y;
        self.restore_count = self.restore_count.saturating_add(1);
        Some(AnchorRestoreResult {
            frame_id: self.frame_id,
            kind: primary.kind,
            anchor: primary.anchor,
            restored_global_scroll_top: restored_global_scroll_top.max(0.0),
            restore_count: self.restore_count,
        })
    }

    pub fn restore_count(&self) -> u8 {
        self.restore_count
    }

    pub fn trace(&self) -> AnchorTraceFrame {
        AnchorTraceFrame {
            frame_id: self.frame_id,
            primary_anchor_kind: self.primary_kind(),
            restore_count: self.restore_count,
        }
    }
}

pub trait AnchorGlobalOffsetResolver {
    fn global_offset_of_block(&self, block_id: BlockId) -> Option<LayoutPx>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorRestoreResult {
    pub frame_id: u64,
    pub kind: AnchorKind,
    pub anchor: ScrollAnchor,
    pub restored_global_scroll_top: LayoutPx,
    pub restore_count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorTraceFrame {
    pub frame_id: u64,
    pub primary_anchor_kind: Option<AnchorKind>,
    pub restore_count: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn anchor_priority_matches_ime_and_caret_requirements() {
        assert!(AnchorKind::Composition.priority() > AnchorKind::Caret.priority());
        assert!(AnchorKind::Caret.priority() > AnchorKind::SelectionFocus.priority());
        assert!(
            AnchorKind::SelectionFocus.priority() > AnchorKind::ExplicitScrollTarget.priority()
        );
        assert!(AnchorKind::ExplicitScrollTarget.priority() > AnchorKind::ViewportTop.priority());
    }

    #[test]
    fn low_priority_anchor_cannot_override_high_priority_anchor() {
        let mut frame = AnchorFrame::new(1);
        frame.offer(candidate(AnchorKind::Caret, 10, 20.0, 100.0));
        let replaced = frame.offer(candidate(AnchorKind::ViewportTop, 1, 0.0, 0.0));

        assert!(!replaced);
        assert_eq!(frame.primary_kind(), Some(AnchorKind::Caret));
    }

    #[test]
    fn composition_anchor_overrides_caret_for_ime_candidate_stability() {
        let mut frame = AnchorFrame::new(1);
        frame.offer(candidate(AnchorKind::Caret, 10, 20.0, 100.0));
        frame.offer(candidate(AnchorKind::Composition, 10, 25.0, 105.0));

        assert_eq!(frame.primary_kind(), Some(AnchorKind::Composition));
    }

    #[test]
    fn caret_anchor_restore_keeps_caret_viewport_y_stable_after_reflow() {
        let mut frame = AnchorFrame::new(7);
        frame.offer(
            CaretAnchor {
                block_id: 42,
                text_offset: 128,
                caret_rect_y_in_block: 60.0,
                viewport_y: 300.0,
            }
            .into(),
        );
        let resolver = MockResolver::new([(42, 1_000.0)]);

        let restore = frame.restore_once(&resolver).unwrap();

        assert_eq!(restore.kind, AnchorKind::Caret);
        assert_eq!(restore.restored_global_scroll_top, 760.0);
        assert_eq!(restore.restore_count, 1);
    }

    #[test]
    fn same_frame_allows_at_most_one_anchor_restore() {
        let mut frame = AnchorFrame::new(99);
        frame.offer(candidate(AnchorKind::Composition, 1, 10.0, 20.0));
        let resolver = MockResolver::new([(1, 100.0)]);

        assert!(frame.restore_once(&resolver).is_some());
        assert!(frame.restore_once(&resolver).is_none());
        assert_eq!(frame.restore_count(), 1);
    }

    #[test]
    fn trace_reports_primary_anchor_kind() {
        let mut frame = AnchorFrame::new(5);
        frame.offer(candidate(AnchorKind::SelectionFocus, 2, 0.0, 40.0));

        let trace = frame.trace();

        assert_eq!(trace.frame_id, 5);
        assert_eq!(trace.primary_anchor_kind, Some(AnchorKind::SelectionFocus));
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

    struct MockResolver {
        offsets: HashMap<BlockId, LayoutPx>,
    }

    impl MockResolver {
        fn new(entries: impl IntoIterator<Item = (BlockId, LayoutPx)>) -> Self {
            Self {
                offsets: entries.into_iter().collect(),
            }
        }
    }

    impl AnchorGlobalOffsetResolver for MockResolver {
        fn global_offset_of_block(&self, block_id: BlockId) -> Option<LayoutPx> {
            self.offsets.get(&block_id).copied()
        }
    }
}
