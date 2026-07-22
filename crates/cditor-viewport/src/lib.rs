pub mod debug_overlay;
pub mod hit_test;
pub mod scroll;
pub mod scroll_trace_replay;
pub mod trace_event_log;
pub mod window;

pub use debug_overlay::{
    DebugAnchor, DebugOverlayLine, DebugOverlaySnapshot, DebugOverlayViewModel,
    HeightConfidenceRegion, PageBoundaryDebug, caret_jitter_px, scroll_jitter_px,
};
pub use scroll_trace_replay::{
    RegressionGateConfig, RegressionGateResult, ScrollTraceFrame, ScrollTraceReplay,
    ScrollTraceReplayReport, TraceInput,
};
pub use trace_event_log::{
    AnchorRestoredEvent, AsyncResultDiscardedEvent, EntityEvictedEvent, LayoutTaskDeferredEvent,
    OldRequestDiscardedEvent, PageHeightCorrectedEvent, PinChangedEvent, PinTraceReason,
    ScrollbarDragFrozenTotalHeightEvent, TraceEvent, TraceEventKind, TraceEventLog,
    TraceEventPayload, TraceEventSnapshot, TraceScrollStateSample, WindowChangedEvent,
};
