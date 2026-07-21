use std::error::Error;
use std::fmt::{Display, Formatter};

use cditor_core::{edit::ScrollAnchor, ids::BlockId};

pub type LayoutPx = f64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualScrollState {
    pub global_scroll_top: LayoutPx,
    pub viewport_height: LayoutPx,
    pub model_total_height: LayoutPx,
    pub displayed_total_height: LayoutPx,
    pub anchor: Option<ScrollAnchor>,
    pub pending_target: Option<VirtualScrollTarget>,
    pub origin: ScrollOrigin,
    pub precision: ScrollPrecision,
}

impl VirtualScrollState {
    pub fn new(
        viewport_height: LayoutPx,
        model_total_height: LayoutPx,
    ) -> Result<Self, VirtualScrollError> {
        validate_non_negative_finite(viewport_height, "viewport_height")?;
        validate_non_negative_finite(model_total_height, "model_total_height")?;

        Ok(Self {
            global_scroll_top: 0.0,
            viewport_height,
            model_total_height,
            displayed_total_height: model_total_height,
            anchor: None,
            pending_target: None,
            origin: ScrollOrigin::ProgrammaticVirtualScroll,
            precision: ScrollPrecision::Exact,
        })
    }

    pub fn max_scroll_top(&self) -> LayoutPx {
        (self.model_total_height - self.viewport_height).max(0.0)
    }

    pub fn clamp_global_scroll_top(&self, global_y: LayoutPx) -> LayoutPx {
        if !global_y.is_finite() {
            return self.global_scroll_top;
        }
        global_y.clamp(0.0, self.max_scroll_top())
    }

    pub fn set_viewport_height(
        &mut self,
        viewport_height: LayoutPx,
    ) -> Result<(), VirtualScrollError> {
        validate_non_negative_finite(viewport_height, "viewport_height")?;
        self.viewport_height = viewport_height;
        self.global_scroll_top = self.clamp_global_scroll_top(self.global_scroll_top);
        Ok(())
    }

    pub fn set_model_total_height(
        &mut self,
        model_total_height: LayoutPx,
    ) -> Result<(), VirtualScrollError> {
        validate_non_negative_finite(model_total_height, "model_total_height")?;
        self.model_total_height = model_total_height;
        self.global_scroll_top = self.clamp_global_scroll_top(self.global_scroll_top);
        self.precision = if nearly_equal(self.displayed_total_height, self.model_total_height) {
            self.precision
        } else {
            ScrollPrecision::Converging
        };
        Ok(())
    }

    pub fn set_displayed_total_height(
        &mut self,
        displayed_total_height: LayoutPx,
    ) -> Result<(), VirtualScrollError> {
        validate_non_negative_finite(displayed_total_height, "displayed_total_height")?;
        self.displayed_total_height = displayed_total_height;
        self.precision = if nearly_equal(self.displayed_total_height, self.model_total_height) {
            self.precision
        } else {
            ScrollPrecision::Converging
        };
        Ok(())
    }

    pub fn scroll_to_global_offset(
        &mut self,
        global_y: LayoutPx,
        origin: ScrollOrigin,
    ) -> Result<Option<VirtualScrollTarget>, VirtualScrollError> {
        if origin == ScrollOrigin::LocalListSync {
            return Ok(None);
        }
        if !global_y.is_finite() {
            return Err(VirtualScrollError::InvalidCoordinate("global_y"));
        }

        let clamped = self.clamp_global_scroll_top(global_y);
        self.global_scroll_top = clamped;
        self.origin = origin;
        let target = VirtualScrollTarget {
            block_id: None,
            block_index: None,
            offset_in_block: 0.0,
            global_scroll_top: clamped,
            precision: self.precision,
        };
        self.pending_target = Some(target);
        Ok(Some(target))
    }

    pub fn scroll_by_delta(
        &mut self,
        delta_y: LayoutPx,
        origin: ScrollOrigin,
    ) -> Result<Option<VirtualScrollTarget>, VirtualScrollError> {
        if origin == ScrollOrigin::LocalListSync {
            return Ok(None);
        }
        if !delta_y.is_finite() {
            return Err(VirtualScrollError::InvalidCoordinate("delta_y"));
        }
        self.scroll_to_global_offset(self.global_scroll_top + delta_y, origin)
    }

    pub fn scroll_to_block(
        &mut self,
        block_id: BlockId,
        resolver: &impl BlockScrollResolver,
        origin: ScrollOrigin,
    ) -> Result<Option<VirtualScrollTarget>, VirtualScrollError> {
        if origin == ScrollOrigin::LocalListSync {
            return Ok(None);
        }

        let resolved = resolver
            .resolve_block_scroll_target(block_id)
            .ok_or(VirtualScrollError::UnknownBlock(block_id))?;
        let clamped = self.clamp_global_scroll_top(resolved.global_scroll_top);
        let target = VirtualScrollTarget {
            block_id: Some(block_id),
            block_index: Some(resolved.block_index),
            offset_in_block: resolved.offset_in_block,
            global_scroll_top: clamped,
            precision: resolved.precision,
        };

        self.global_scroll_top = clamped;
        self.origin = origin;
        self.precision = resolved.precision;
        self.pending_target = Some(target);
        Ok(Some(target))
    }

    pub fn set_anchor(&mut self, anchor: Option<ScrollAnchor>) {
        self.anchor = anchor;
    }
}

pub trait BlockScrollResolver {
    fn resolve_block_scroll_target(&self, block_id: BlockId) -> Option<ResolvedBlockScrollTarget>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedBlockScrollTarget {
    pub block_index: usize,
    pub offset_in_block: LayoutPx,
    pub global_scroll_top: LayoutPx,
    pub precision: ScrollPrecision,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualScrollTarget {
    pub block_id: Option<BlockId>,
    pub block_index: Option<usize>,
    pub offset_in_block: LayoutPx,
    pub global_scroll_top: LayoutPx,
    pub precision: ScrollPrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOrigin {
    UserWheel,
    UserScrollbar,
    ProgrammaticVirtualScroll,
    LocalListSync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollPrecision {
    Exact,
    LocalExact,
    Estimated,
    Converging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualScrollError {
    InvalidCoordinate(&'static str),
    UnknownBlock(BlockId),
}

impl Display for VirtualScrollError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCoordinate(name) => {
                write!(formatter, "invalid virtual scroll coordinate: {name}")
            }
            Self::UnknownBlock(block_id) => {
                write!(formatter, "unknown block scroll target: {block_id}")
            }
        }
    }
}

impl Error for VirtualScrollError {}

fn validate_non_negative_finite(
    value: LayoutPx,
    name: &'static str,
) -> Result<(), VirtualScrollError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(VirtualScrollError::InvalidCoordinate(name))
    }
}

fn nearly_equal(a: LayoutPx, b: LayoutPx) -> bool {
    (a - b).abs() <= f64::EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn clamps_to_top_and_bottom() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();

        let target = state
            .scroll_to_global_offset(-50.0, ScrollOrigin::ProgrammaticVirtualScroll)
            .unwrap()
            .unwrap();
        assert_eq!(target.global_scroll_top, 0.0);
        assert_eq!(state.global_scroll_top, 0.0);

        let target = state
            .scroll_to_global_offset(2_000.0, ScrollOrigin::ProgrammaticVirtualScroll)
            .unwrap()
            .unwrap();
        assert_eq!(target.global_scroll_top, 900.0);
        assert_eq!(state.global_scroll_top, 900.0);

        state.set_viewport_height(1_200.0).unwrap();
        assert_eq!(state.global_scroll_top, 0.0);
    }

    #[test]
    fn wheel_delta_positive_and_negative_update_global_scroll() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();

        state
            .scroll_by_delta(120.0, ScrollOrigin::UserWheel)
            .unwrap();
        assert_eq!(state.global_scroll_top, 120.0);
        assert_eq!(state.origin, ScrollOrigin::UserWheel);

        state
            .scroll_by_delta(-50.0, ScrollOrigin::UserWheel)
            .unwrap();
        assert_eq!(state.global_scroll_top, 70.0);

        state
            .scroll_by_delta(-1_000.0, ScrollOrigin::UserWheel)
            .unwrap();
        assert_eq!(state.global_scroll_top, 0.0);
    }

    #[test]
    fn origin_guard_ignores_local_list_sync() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();
        state
            .scroll_to_global_offset(300.0, ScrollOrigin::UserWheel)
            .unwrap();

        let ignored = state
            .scroll_to_global_offset(900.0, ScrollOrigin::LocalListSync)
            .unwrap();

        assert_eq!(ignored, None);
        assert_eq!(state.global_scroll_top, 300.0);
        assert_eq!(state.origin, ScrollOrigin::UserWheel);
    }

    #[test]
    fn scroll_to_block_uses_resolver_without_ui_dependency() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();
        let resolver = MockResolver::new([(
            42,
            ResolvedBlockScrollTarget {
                block_index: 7,
                offset_in_block: 0.0,
                global_scroll_top: 420.0,
                precision: ScrollPrecision::Estimated,
            },
        )]);

        let target = state
            .scroll_to_block(42, &resolver, ScrollOrigin::ProgrammaticVirtualScroll)
            .unwrap()
            .unwrap();

        assert_eq!(target.block_id, Some(42));
        assert_eq!(target.block_index, Some(7));
        assert_eq!(target.global_scroll_top, 420.0);
        assert_eq!(target.precision, ScrollPrecision::Estimated);
        assert_eq!(state.precision, ScrollPrecision::Estimated);
    }

    #[test]
    fn total_height_mismatch_marks_scroll_as_converging() {
        let mut state = VirtualScrollState::new(100.0, 1_000.0).unwrap();

        state.set_displayed_total_height(900.0).unwrap();

        assert_eq!(state.precision, ScrollPrecision::Converging);
    }

    struct MockResolver {
        targets: HashMap<BlockId, ResolvedBlockScrollTarget>,
    }

    impl MockResolver {
        fn new(entries: impl IntoIterator<Item = (BlockId, ResolvedBlockScrollTarget)>) -> Self {
            Self {
                targets: entries.into_iter().collect(),
            }
        }
    }

    impl BlockScrollResolver for MockResolver {
        fn resolve_block_scroll_target(
            &self,
            block_id: BlockId,
        ) -> Option<ResolvedBlockScrollTarget> {
            self.targets.get(&block_id).copied()
        }
    }
}
