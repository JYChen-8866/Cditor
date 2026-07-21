use std::collections::HashMap;
use std::ops::Range;

use cditor_core::ids::{BlockId, SurfaceId};
use cditor_core::layout::HeightConfidence;
use cditor_core::version::{SnapshotIdentity, SnapshotIdentityMismatch};
use cditor_editor_core::scroll::VirtualScrollTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AsyncLayoutVersion {
    pub content_version: u64,
    pub layout_version: u64,
    pub width_bucket: u16,
    pub exact_width_px: u32,
    pub theme_version: u64,
}

impl AsyncLayoutVersion {
    pub const fn new(
        content_version: u64,
        layout_version: u64,
        width_bucket: u16,
        exact_width_px: u32,
        theme_version: u64,
    ) -> Self {
        Self {
            content_version,
            layout_version,
            width_bucket,
            exact_width_px,
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
    pub identity: SnapshotIdentity,
    pub width_bucket: u16,
    pub exact_width_px: u32,
    pub theme_version: u64,
    pub kind: AsyncTaskKind,
}

impl LayoutTaskRequest {
    pub fn new(
        identity: SnapshotIdentity,
        version: AsyncLayoutVersion,
        kind: AsyncTaskKind,
    ) -> Self {
        debug_assert_eq!(identity.content_version, Some(version.content_version));
        debug_assert_eq!(identity.layout_version, version.layout_version);
        Self {
            identity,
            width_bucket: version.width_bucket,
            exact_width_px: version.exact_width_px,
            theme_version: version.theme_version,
            kind,
        }
    }

    pub fn surface_id(&self) -> Option<SurfaceId> {
        self.identity.surface_id
    }

    pub fn block_id(&self) -> Option<BlockId> {
        self.surface_id().and_then(SurfaceId::block_id)
    }

    pub fn version(&self) -> AsyncLayoutVersion {
        AsyncLayoutVersion {
            content_version: self.identity.content_version.unwrap_or_default(),
            layout_version: self.identity.layout_version,
            width_bucket: self.width_bucket,
            exact_width_px: self.exact_width_px,
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
    pub identity: SnapshotIdentity,
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
    MissingSurfaceIdentity,
    UnknownSurface(SurfaceId),
    SnapshotIdentity(SnapshotIdentityMismatch),
    WidthBucketMismatch { expected: u16, actual: u16 },
    ExactWidthMismatch { expected: u32, actual: u32 },
    ThemeVersionMismatch { expected: u64, actual: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalLayoutHint {
    pub identity: SnapshotIdentity,
    pub height: u64,
    pub stale_reason: DiscardReason,
}

impl HistoricalLayoutHint {
    pub fn block_id(&self) -> Option<BlockId> {
        self.identity.surface_id.and_then(SurfaceId::block_id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsyncVersionController {
    current_document_identity: SnapshotIdentity,
    surface_versions: HashMap<SurfaceId, AsyncLayoutVersion>,
    historical_hints: Vec<HistoricalLayoutHint>,
}

impl AsyncVersionController {
    pub fn new(current_document_identity: SnapshotIdentity) -> Self {
        assert!(
            current_document_identity.surface_id.is_none()
                && current_document_identity.content_version.is_none(),
            "controller identity must describe the document scope"
        );
        Self {
            current_document_identity,
            surface_versions: HashMap::new(),
            historical_hints: Vec::new(),
        }
    }

    pub fn current_identity(&self) -> SnapshotIdentity {
        self.current_document_identity
    }

    pub fn current_generation(&self) -> u64 {
        self.current_document_identity.generation
    }

    pub fn set_current_document_identity(&mut self, identity: SnapshotIdentity) {
        assert!(
            identity.surface_id.is_none() && identity.content_version.is_none(),
            "controller identity must describe the document scope"
        );
        if identity.document_id != self.current_document_identity.document_id {
            self.surface_versions.clear();
            self.historical_hints.clear();
        }
        self.current_document_identity = identity;
    }

    pub fn bump_generation(&mut self) -> u64 {
        self.current_document_identity.generation =
            self.current_document_identity.generation.saturating_add(1);
        self.current_document_identity.generation
    }

    pub fn set_block_version(&mut self, block_id: BlockId, version: AsyncLayoutVersion) {
        self.set_surface_version(SurfaceId::Block(block_id), version);
    }

    pub fn set_surface_version(&mut self, surface_id: SurfaceId, version: AsyncLayoutVersion) {
        self.surface_versions.insert(surface_id, version);
    }

    pub fn request_for(&self, block_id: BlockId, kind: AsyncTaskKind) -> Option<LayoutTaskRequest> {
        self.request_for_surface(SurfaceId::Block(block_id), kind)
    }

    pub fn request_for_surface(
        &self,
        surface_id: SurfaceId,
        kind: AsyncTaskKind,
    ) -> Option<LayoutTaskRequest> {
        let version = *self.surface_versions.get(&surface_id)?;
        Some(LayoutTaskRequest::new(
            self.identity_for_surface(surface_id, version),
            version,
            kind,
        ))
    }

    pub fn page_window_request(
        &self,
        page_range: Range<usize>,
        target: VirtualScrollTarget,
    ) -> PageWindowRequest {
        PageWindowRequest {
            identity: self.current_document_identity,
            page_range,
            target,
        }
    }

    pub fn validate_layout_result(&mut self, result: LayoutTaskResult) -> AsyncResultDecision {
        let decision = self.validate_request(result.request);
        match decision {
            AsyncResultDecision::Discard(reason) => {
                self.historical_hints.push(HistoricalLayoutHint {
                    identity: result.request.identity,
                    height: result.measured_height,
                    stale_reason: reason,
                });
                AsyncResultDecision::StoreHistoricalHint
            }
            other => other,
        }
    }

    pub fn validate_request(&self, request: LayoutTaskRequest) -> AsyncResultDecision {
        let Some(surface_id) = request.identity.surface_id else {
            return AsyncResultDecision::Discard(DiscardReason::MissingSurfaceIdentity);
        };
        let Some(current_version) = self.surface_versions.get(&surface_id).copied() else {
            return AsyncResultDecision::Discard(DiscardReason::UnknownSurface(surface_id));
        };
        let current_identity = self.identity_for_surface(surface_id, current_version);
        if let Err(mismatch) = request.identity.validate_against(current_identity) {
            return AsyncResultDecision::Discard(DiscardReason::SnapshotIdentity(mismatch));
        }

        let requested = request.version();
        if requested.width_bucket != current_version.width_bucket {
            return AsyncResultDecision::Discard(DiscardReason::WidthBucketMismatch {
                expected: current_version.width_bucket,
                actual: requested.width_bucket,
            });
        }
        if requested.exact_width_px != current_version.exact_width_px {
            return AsyncResultDecision::Discard(DiscardReason::ExactWidthMismatch {
                expected: current_version.exact_width_px,
                actual: requested.exact_width_px,
            });
        }
        if requested.theme_version != current_version.theme_version {
            return AsyncResultDecision::Discard(DiscardReason::ThemeVersionMismatch {
                expected: current_version.theme_version,
                actual: requested.theme_version,
            });
        }
        AsyncResultDecision::ApplyExact
    }

    pub fn validate_page_window_result(&self, result: &PageWindowResult) -> AsyncResultDecision {
        match result
            .request
            .identity
            .validate_against(self.current_document_identity)
        {
            Ok(()) => AsyncResultDecision::ApplyExact,
            Err(mismatch) => {
                AsyncResultDecision::Discard(DiscardReason::SnapshotIdentity(mismatch))
            }
        }
    }

    pub fn historical_hints(&self) -> &[HistoricalLayoutHint] {
        &self.historical_hints
    }

    fn identity_for_surface(
        &self,
        surface_id: SurfaceId,
        version: AsyncLayoutVersion,
    ) -> SnapshotIdentity {
        self.current_document_identity.with_surface(
            surface_id,
            version.content_version,
            version.layout_version,
        )
    }
}

impl Default for AsyncVersionController {
    fn default() -> Self {
        Self::new(SnapshotIdentity::document(0, 0, 0, 0, 0, 0, 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_editor_core::scroll::ScrollPrecision;

    #[test]
    fn generation_mismatch_stores_only_a_historical_hint() {
        let mut controller = controller_with_block(42, version(1, 1, 10, 800, 1));
        let request = controller
            .request_for(42, AsyncTaskKind::LayoutMeasure)
            .unwrap();
        controller.bump_generation();

        let decision = controller.validate_layout_result(result(request, 120));

        assert_eq!(decision, AsyncResultDecision::StoreHistoricalHint);
        assert_eq!(controller.historical_hints().len(), 1);
        assert_eq!(controller.historical_hints()[0].block_id(), Some(42));
        assert!(matches!(
            controller.historical_hints()[0].stale_reason,
            DiscardReason::SnapshotIdentity(SnapshotIdentityMismatch::Generation {
                expected: 10,
                actual: 9,
            })
        ));
    }

    #[test]
    fn width_bucket_mismatch_does_not_cover_current_exact_height() {
        let mut controller = controller_with_block(42, version(1, 1, 11, 880, 1));
        let identity = controller
            .current_identity()
            .with_surface(SurfaceId::Block(42), 1, 1);
        let request = LayoutTaskRequest::new(
            identity,
            version(1, 1, 10, 800, 1),
            AsyncTaskKind::LayoutMeasure,
        );

        let decision = controller.validate_layout_result(result(request, 90));

        assert_eq!(decision, AsyncResultDecision::StoreHistoricalHint);
        assert!(matches!(
            controller.historical_hints()[0].stale_reason,
            DiscardReason::WidthBucketMismatch {
                expected: 11,
                actual: 10,
            }
        ));
    }

    #[test]
    fn every_snapshot_identity_dimension_rejects_stale_layout_results() {
        assert_identity_mismatch(
            |identity| identity.document_id -= 1,
            SnapshotIdentityMismatch::Document {
                expected: 7,
                actual: 6,
            },
        );
        assert_identity_mismatch(
            |identity| identity.structure_version -= 1,
            SnapshotIdentityMismatch::Structure {
                expected: 3,
                actual: 2,
            },
        );
        assert_identity_mismatch(
            |identity| identity.content_version = Some(0),
            SnapshotIdentityMismatch::Content {
                expected: Some(1),
                actual: Some(0),
            },
        );
        assert_identity_mismatch(
            |identity| identity.layout_version = 0,
            SnapshotIdentityMismatch::Layout {
                expected: 1,
                actual: 0,
            },
        );
        assert_identity_mismatch(
            |identity| identity.font_epoch -= 1,
            SnapshotIdentityMismatch::Font {
                expected: 5,
                actual: 4,
            },
        );
        assert_identity_mismatch(
            |identity| identity.scale_epoch -= 1,
            SnapshotIdentityMismatch::Scale {
                expected: 6,
                actual: 5,
            },
        );
        assert_identity_mismatch(
            |identity| identity.viewport_epoch -= 1,
            SnapshotIdentityMismatch::Viewport {
                expected: 8,
                actual: 7,
            },
        );
        assert_identity_mismatch(
            |identity| identity.generation -= 1,
            SnapshotIdentityMismatch::Generation {
                expected: 9,
                actual: 8,
            },
        );
    }

    #[test]
    fn unknown_or_missing_surface_identity_is_rejected() {
        let controller = controller_with_block(42, version(1, 1, 10, 800, 1));
        let document_request = LayoutTaskRequest {
            identity: controller.current_identity(),
            width_bucket: 10,
            exact_width_px: 800,
            theme_version: 1,
            kind: AsyncTaskKind::TextShaping,
        };
        assert_eq!(
            controller.validate_request(document_request),
            AsyncResultDecision::Discard(DiscardReason::MissingSurfaceIdentity)
        );

        let unknown = LayoutTaskRequest::new(
            controller
                .current_identity()
                .with_surface(SurfaceId::Block(99), 1, 1),
            version(1, 1, 10, 800, 1),
            AsyncTaskKind::TextShaping,
        );
        assert_eq!(
            controller.validate_request(unknown),
            AsyncResultDecision::Discard(DiscardReason::UnknownSurface(SurfaceId::Block(99)))
        );
    }

    #[test]
    fn table_cells_have_independent_async_identities() {
        let mut controller = controller();
        let first = SurfaceId::TableCell {
            block_id: 42,
            row: 0,
            column: 0,
        };
        let second = SurfaceId::TableCell {
            block_id: 42,
            row: 0,
            column: 1,
        };
        controller.set_surface_version(first, version(1, 1, 10, 800, 1));
        controller.set_surface_version(second, version(2, 3, 10, 800, 1));

        let first_request = controller
            .request_for_surface(first, AsyncTaskKind::TextShaping)
            .unwrap();
        let second_request = controller
            .request_for_surface(second, AsyncTaskKind::TextShaping)
            .unwrap();

        assert_ne!(first_request.identity, second_request.identity);
        assert_eq!(
            controller.validate_request(first_request),
            AsyncResultDecision::ApplyExact
        );
        assert_eq!(
            controller.validate_request(second_request),
            AsyncResultDecision::ApplyExact
        );
    }

    #[test]
    fn font_epoch_change_rejects_old_measurement() {
        let mut controller = controller_with_block(42, version(1, 1, 10, 800, 1));
        let request = controller
            .request_for(42, AsyncTaskKind::LayoutMeasure)
            .unwrap();
        let mut current = controller.current_identity();
        current.font_epoch += 1;
        controller.set_current_document_identity(current);

        let decision = controller.validate_layout_result(result(request, 100));

        assert_eq!(decision, AsyncResultDecision::StoreHistoricalHint);
        assert!(matches!(
            controller.historical_hints()[0].stale_reason,
            DiscardReason::SnapshotIdentity(SnapshotIdentityMismatch::Font { .. })
        ));
    }

    #[test]
    fn matching_request_applies_exact() {
        let mut controller = controller_with_block(42, version(1, 1, 10, 800, 1));
        let request = controller
            .request_for(42, AsyncTaskKind::LayoutMeasure)
            .unwrap();

        let decision = controller.validate_layout_result(result(request, 120));

        assert_eq!(decision, AsyncResultDecision::ApplyExact);
        assert!(controller.historical_hints().is_empty());
    }

    #[test]
    fn page_window_results_validate_the_full_document_identity() {
        let mut controller = controller();
        let page10 = controller.page_window_request(10..11, scroll_target());
        controller.bump_generation();
        let mut current = controller.current_identity();
        current.viewport_epoch += 1;
        controller.set_current_document_identity(current);
        let page40 = controller.page_window_request(40..41, scroll_target());

        assert!(matches!(
            controller.validate_page_window_result(&PageWindowResult {
                request: page10,
                loaded_block_count: 100,
            }),
            AsyncResultDecision::Discard(DiscardReason::SnapshotIdentity(
                SnapshotIdentityMismatch::Viewport { .. }
                    | SnapshotIdentityMismatch::Generation { .. }
            ))
        ));
        assert_eq!(
            controller.validate_page_window_result(&PageWindowResult {
                request: page40,
                loaded_block_count: 100,
            }),
            AsyncResultDecision::ApplyExact
        );
    }

    fn assert_identity_mismatch(
        mutate: impl FnOnce(&mut SnapshotIdentity),
        expected: SnapshotIdentityMismatch,
    ) {
        let controller = controller_with_block(42, version(1, 1, 10, 800, 1));
        let mut request = controller
            .request_for(42, AsyncTaskKind::TextShaping)
            .unwrap();
        mutate(&mut request.identity);

        assert_eq!(
            controller.validate_request(request),
            AsyncResultDecision::Discard(DiscardReason::SnapshotIdentity(expected))
        );
    }

    fn controller_with_block(
        block_id: BlockId,
        block_version: AsyncLayoutVersion,
    ) -> AsyncVersionController {
        let mut controller = controller();
        controller.set_block_version(block_id, block_version);
        controller
    }

    fn controller() -> AsyncVersionController {
        AsyncVersionController::new(SnapshotIdentity::document(7, 3, 0, 5, 6, 8, 9))
    }

    fn version(
        content_version: u64,
        layout_version: u64,
        width_bucket: u16,
        exact_width_px: u32,
        theme_version: u64,
    ) -> AsyncLayoutVersion {
        AsyncLayoutVersion::new(
            content_version,
            layout_version,
            width_bucket,
            exact_width_px,
            theme_version,
        )
    }

    fn result(request: LayoutTaskRequest, measured_height: u64) -> LayoutTaskResult {
        LayoutTaskResult {
            request,
            measured_height,
            confidence: HeightConfidence::Exact,
        }
    }

    fn scroll_target() -> VirtualScrollTarget {
        VirtualScrollTarget {
            block_id: None,
            block_index: None,
            offset_in_block: 0.0,
            global_scroll_top: 0.0,
            precision: ScrollPrecision::Estimated,
        }
    }
}
