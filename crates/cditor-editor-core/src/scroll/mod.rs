pub mod anchor;
pub mod global_offset;
pub mod height_correction;
pub mod scrollbar;
pub mod virtual_scroll;
pub mod wheel;

pub use cditor_core::edit::ScrollAnchor;

pub use anchor::{
    AnchorCandidate, AnchorFrame, AnchorGlobalOffsetResolver, AnchorKind, AnchorRestoreResult,
    AnchorTraceFrame, CaretAnchor,
};
pub use global_offset::{
    GlobalOffsetMapper, GlobalOffsetTarget, RenderWindowGeometry, TargetContent,
    ViewportLocalCoordinate,
};
pub use height_correction::{
    FrameScrollContext, HeightChange as ScrollHeightChange, HeightChangeQueue,
    HeightCorrectionConfig, HeightCorrectionDebugOverlay, HeightCorrectionFrameResult,
    HeightCorrectionPipeline, HeightErrorAccumulator, HeightErrorBudget, LoadedPageLayoutUpdater,
};
pub use scrollbar::{
    PendingHeightCorrection, ScrollbarDragEnd, ScrollbarDragSession, ScrollbarDragUpdate,
    ScrollbarPolicy, ScrollbarVisualState,
};
pub use virtual_scroll::{
    BlockScrollResolver, LayoutPx, ResolvedBlockScrollTarget, ScrollOrigin, ScrollPrecision,
    VirtualScrollError, VirtualScrollState, VirtualScrollTarget,
};
pub use wheel::{
    HeightCorrectionPriority, ScrollAccumulator, ScrollDeltaMode, ScrollDevice, ScrollInput,
    ScrollInteractionState, ScrollPhase, WheelPipelineConfig,
};
