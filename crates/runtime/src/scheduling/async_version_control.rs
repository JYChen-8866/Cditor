use std::collections::HashMap;
use std::ops::Range;

use cditor_core::ids::BlockId;
use cditor_core::layout::HeightConfidence;
use cditor_editor::scroll::VirtualScrollTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AsyncLayoutVersion {
    pub content_version: u64,
    pub layout_version: u64,
    pub width_bucket: u16,
    pub exact_width_px: u32,
    pub font_version: u64,
    pub theme_version: u64,
}

impl AsyncLayoutVersion {
    pub const fn new(
        content_version: u64,
        layout_version: u64,
        width_bucket: u16,
        exact_width_px: u32,
        font_version: u64,
        theme_version: u64,
    ) -> Self {
        Self {
            content_version,
            layout_version,
            width_bucket,
            exact_width_px,
            font_version,
            theme_version,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncTaskKind {
    TextShaping,
    LayoutMeasure,
    ImageDecode,
    SyntaxHighlight,
    PageWindowLoad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutTaskRequest {
    pub generation: u64,
    pub block_id: BlockId,
    pub content_version: u64,
    pub layout_version: u64,
    pub width_bucket: u16,
    pub exact_width_px: u32,
    pub font_version: u64,
    pub theme_version: u64,
    pub kind: AsyncTaskKind,
}

impl LayoutTaskRequest {
    pub fn new(
        generation: u64,
        block_id: BlockId,
        version: AsyncLayoutVersion,
        kind: AsyncTaskKind,
    ) -> Self {
        Self {
            generation,
            block_id,
            content_version: version.content_version,
            layout_version: version.layout_version,
            width_bucket: version.width_bucket,
            exact_width_px: version.exact_width_px,
            font_version: version.font_version,
            theme_version: version.theme_version,
            kind,
        }
    }

    pub fn version(&self) -> AsyncLayoutVersion {
        AsyncLayoutVersion {
            content_version: self.content_version,
            layout_version: self.layout_version,
            width_bucket: self.width_bucket,
            exact_width_px: self.exact_width_px,
            font_version: self.font_version,
            theme_version: self.theme_version,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutTaskResult {
    pub request: LayoutTaskRequest,
    pub measured_height: u64,
    pub confidence: HeightConfidence,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageWindowRequest {
    pub generation: u64,
    pub page_range: Range<usize>,
    pub target: VirtualScrollTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageWindowResult {
    pub request: PageWindowRequest,
    pub loaded_block_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncResultDecision {
    ApplyExact,
    StoreHistoricalHint,
    Discard(DiscardReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardReason {
    GenerationMismatch { expected: u64, actual: u64 },
    UnknownBlock(BlockId),
    ContentVersionMismatch { expected: u64, actual: u64 },
    LayoutVersionMismatch { expected: u64, actual: u64 },
    WidthBucketMismatch { expected: u16, actual: u16 },
    ExactWidthMismatch { expected: u32, actual: u32 },
    FontVersionMismatch { expected: u64, actual: u64 },
    ThemeVersionMismatch { expected: u64, actual: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalLayoutHint {
    pub block_id: BlockId,
    pub generation: u64,
    pub height: u64,
    pub stale_reason: DiscardReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsyncVersionController {
    current_generation: u64,
    block_versions: HashMap<BlockId, AsyncLayoutVersion>,
    historical_hints: Vec<HistoricalLayoutHint>,
}

impl AsyncVersionController {
    pub fn new(current_generation: u64) -> Self {
        Self {
            current_generation,
            block_versions: HashMap::new(),
            historical_hints: Vec::new(),
        }
    }

    pub fn current_generation(&self) -> u64 {
        self.current_generation
    }

    pub fn bump_generation(&mut self) -> u64 {
        self.current_generation = self.current_generation.saturating_add(1);
        self.current_generation
    }

    pub fn set_block_version(&mut self, block_id: BlockId, version: AsyncLayoutVersion) {
        self.block_versions.insert(block_id, version);
    }

    pub fn request_for(&self, block_id: BlockId, kind: AsyncTaskKind) -> Option<LayoutTaskRequest> {
        let version = *self.block_versions.get(&block_id)?;
        Some(LayoutTaskRequest::new(
            self.current_generation,
            block_id,
            version,
            kind,
        ))
    }

    pub fn validate_layout_result(&mut self, result: LayoutTaskResult) -> AsyncResultDecision {
        let decision = self.validate_request(result.request);
        match decision {
            AsyncResultDecision::Discard(reason) => {
                self.historical_hints.push(HistoricalLayoutHint {
                    block_id: result.request.block_id,
                    generation: result.request.generation,
                    height: result.measured_height,
                    stale_reason: reason,
                });
                AsyncResultDecision::StoreHistoricalHint
            }
            other => other,
        }
    }

    pub fn validate_request(&self, request: LayoutTaskRequest) -> AsyncResultDecision {
        if request.generation != self.current_generation {
            return AsyncResultDecision::Discard(DiscardReason::GenerationMismatch {
                expected: self.current_generation,
                actual: request.generation,
            });
        }

        let Some(current) = self.block_versions.get(&request.block_id).copied() else {
            return AsyncResultDecision::Discard(DiscardReason::UnknownBlock(request.block_id));
        };

        let requested = request.version();
        if requested.content_version != current.content_version {
            return AsyncResultDecision::Discard(DiscardReason::ContentVersionMismatch {
                expected: current.content_version,
                actual: requested.content_version,
            });
        }
        if requested.layout_version != current.layout_version {
            return AsyncResultDecision::Discard(DiscardReason::LayoutVersionMismatch {
                expected: current.layout_version,
                actual: requested.layout_version,
            });
        }
        if requested.width_bucket != current.width_bucket {
            return AsyncResultDecision::Discard(DiscardReason::WidthBucketMismatch {
                expected: current.width_bucket,
                actual: requested.width_bucket,
            });
        }
        if requested.exact_width_px != current.exact_width_px {
            return AsyncResultDecision::Discard(DiscardReason::ExactWidthMismatch {
                expected: current.exact_width_px,
                actual: requested.exact_width_px,
            });
        }
        if requested.font_version != current.font_version {
            return AsyncResultDecision::Discard(DiscardReason::FontVersionMismatch {
                expected: current.font_version,
                actual: requested.font_version,
            });
        }
        if requested.theme_version != current.theme_version {
            return AsyncResultDecision::Discard(DiscardReason::ThemeVersionMismatch {
                expected: current.theme_version,
                actual: requested.theme_version,
            });
        }
        AsyncResultDecision::ApplyExact
    }

    pub fn validate_page_window_result(&self, result: &PageWindowResult) -> AsyncResultDecision {
        if result.request.generation == self.current_generation {
            AsyncResultDecision::ApplyExact
        } else {
            AsyncResultDecision::Discard(DiscardReason::GenerationMismatch {
                expected: self.current_generation,
                actual: result.request.generation,
            })
        }
    }

    pub fn historical_hints(&self) -> &[HistoricalLayoutHint] {
        &self.historical_hints
    }
}

impl Default for AsyncVersionController {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_editor::scroll::ScrollPrecision;

    #[test]
    fn generation_discard_test_for_layout_result() {
        let mut controller = controller_with_block(42, version(1, 1, 10, 800, 1, 1));
        let request = controller
            .request_for(42, AsyncTaskKind::LayoutMeasure)
            .unwrap();
        controller.bump_generation();

        let decision = controller.validate_layout_result(LayoutTaskResult {
            request,
            measured_height: 120,
            confidence: HeightConfidence::Exact,
        });

        assert_eq!(decision, AsyncResultDecision::StoreHistoricalHint);
        assert_eq!(controller.historical_hints().len(), 1);
        assert!(matches!(
            controller.historical_hints()[0].stale_reason,
            DiscardReason::GenerationMismatch {
                expected: 2,
                actual: 1
            }
        ));
    }

    #[test]
    fn width_bucket_mismatch_does_not_cover_current_exact_height() {
        let mut controller = controller_with_block(42, version(1, 1, 11, 880, 1, 1));
        let request = LayoutTaskRequest::new(
            1,
            42,
            version(1, 1, 10, 800, 1, 1),
            AsyncTaskKind::LayoutMeasure,
        );

        let decision = controller.validate_layout_result(LayoutTaskResult {
            request,
            measured_height: 90,
            confidence: HeightConfidence::Exact,
        });

        assert_eq!(decision, AsyncResultDecision::StoreHistoricalHint);
        assert!(matches!(
            controller.historical_hints()[0].stale_reason,
            DiscardReason::WidthBucketMismatch {
                expected: 11,
                actual: 10
            }
        ));
    }

    #[test]
    fn content_version_mismatch_discards_current_editing_block_shaping() {
        let mut controller = controller_with_block(42, version(2, 4, 10, 800, 1, 1));
        let old_shaping = LayoutTaskRequest::new(
            1,
            42,
            version(1, 4, 10, 800, 1, 1),
            AsyncTaskKind::TextShaping,
        );

        let decision = controller.validate_layout_result(LayoutTaskResult {
            request: old_shaping,
            measured_height: 100,
            confidence: HeightConfidence::Exact,
        });

        assert_eq!(decision, AsyncResultDecision::StoreHistoricalHint);
        assert!(matches!(
            controller.historical_hints()[0].stale_reason,
            DiscardReason::ContentVersionMismatch {
                expected: 2,
                actual: 1
            }
        ));
    }

    #[test]
    fn old_font_or_theme_measure_cannot_apply_as_exact() {
        let mut controller = controller_with_block(42, version(1, 1, 10, 800, 2, 3));
        let old_font = LayoutTaskRequest::new(
            1,
            42,
            version(1, 1, 10, 800, 1, 3),
            AsyncTaskKind::LayoutMeasure,
        );

        let decision = controller.validate_layout_result(LayoutTaskResult {
            request: old_font,
            measured_height: 100,
            confidence: HeightConfidence::Exact,
        });

        assert_eq!(decision, AsyncResultDecision::StoreHistoricalHint);
        assert!(matches!(
            controller.historical_hints()[0].stale_reason,
            DiscardReason::FontVersionMismatch {
                expected: 2,
                actual: 1
            }
        ));
    }

    #[test]
    fn matching_request_applies_exact() {
        let mut controller = controller_with_block(42, version(1, 1, 10, 800, 1, 1));
        let request = controller
            .request_for(42, AsyncTaskKind::LayoutMeasure)
            .unwrap();

        let decision = controller.validate_layout_result(LayoutTaskResult {
            request,
            measured_height: 120,
            confidence: HeightConfidence::Exact,
        });

        assert_eq!(decision, AsyncResultDecision::ApplyExact);
        assert!(controller.historical_hints().is_empty());
    }

    #[test]
    fn fast_scrollbar_drag_discards_old_page_window_requests() {
        let mut controller = AsyncVersionController::new(1);
        let page10 = page_request(1, 10..11);
        controller.bump_generation();
        let page40 = page_request(2, 40..41);
        controller.bump_generation();
        let page80 = page_request(3, 80..81);

        assert!(matches!(
            controller.validate_page_window_result(&PageWindowResult {
                request: page10,
                loaded_block_count: 100,
            }),
            AsyncResultDecision::Discard(DiscardReason::GenerationMismatch {
                expected: 3,
                actual: 1
            })
        ));
        assert!(matches!(
            controller.validate_page_window_result(&PageWindowResult {
                request: page40,
                loaded_block_count: 100,
            }),
            AsyncResultDecision::Discard(DiscardReason::GenerationMismatch {
                expected: 3,
                actual: 2
            })
        ));
        assert_eq!(
            controller.validate_page_window_result(&PageWindowResult {
                request: page80,
                loaded_block_count: 100,
            }),
            AsyncResultDecision::ApplyExact
        );
    }

    fn controller_with_block(
        block_id: BlockId,
        block_version: AsyncLayoutVersion,
    ) -> AsyncVersionController {
        let mut controller = AsyncVersionController::new(1);
        controller.set_block_version(block_id, block_version);
        controller
    }

    fn version(
        content_version: u64,
        layout_version: u64,
        width_bucket: u16,
        exact_width_px: u32,
        font_version: u64,
        theme_version: u64,
    ) -> AsyncLayoutVersion {
        AsyncLayoutVersion::new(
            content_version,
            layout_version,
            width_bucket,
            exact_width_px,
            font_version,
            theme_version,
        )
    }

    fn page_request(generation: u64, page_range: Range<usize>) -> PageWindowRequest {
        PageWindowRequest {
            generation,
            page_range,
            target: VirtualScrollTarget {
                block_id: None,
                block_index: None,
                offset_in_block: 0.0,
                global_scroll_top: 0.0,
                precision: ScrollPrecision::Estimated,
            },
        }
    }
}
