pub mod content;
pub mod document_runtime;
pub mod editing;
pub mod projection;
pub mod scheduling;

pub use cditor_core::demo_fixtures::{
    LARGE_MIXED_DEMO_BLOCKS, LARGE_MIXED_DEMO_DOCUMENT_ID, large_mixed_demo_document,
    large_mixed_rich_text_document,
};
pub use cditor_import_export::paste_import::{
    ClipboardInput, MediaMetadataTask, NormalizedPasteBlock, PasteImportConfig,
    PasteImportPipeline, PasteImportResult, PastePipelinePhase, PasteProgress, PasteRunOptions,
    PayloadPersistTask, PendingMediaResource,
};
pub use cditor_import_export::security::{
    DataUrlPolicy, EmbedPolicy, ExternalContentPolicy, ExternalResourceAction,
    ExternalResourceDecision, ExternalResourceKind, FileUrlPolicy, PrivacyMode,
    RemoteResourcePolicy, SanitizedHtml, SvgPolicy, sanitize_external_html,
};
pub use content::media_cache::{
    MediaCache, MediaCacheEntry, MediaCachePolicy, MediaCacheStats, MediaDecodeDecision,
    MediaDecodeKind, MediaDecodeLane, MediaDecodeRequest, MediaDecodeTrigger, MediaMetadata,
    MediaResourceId, MediaStableBox, MemoryPressure,
};
pub use content::payload_cache::{
    DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_BYTES, DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_ENTRIES,
    PayloadCachePolicy, PayloadCacheTrimReport,
};
pub use content::payload_window::PayloadWindow;
pub use document_runtime::{
    AiApplyMode, AiRequestDispatch, AiRequestPresentation, AiSessionSnapshot, AiSessionStatus,
    AiStreamApplyResult, CompositionFocusTransition, DocumentRuntime,
    DocumentTextSelectionFragment, RichTextDelta, RichTextSelectionSnapshot, RuntimeAiTarget,
    RuntimeTextSurface, SelectionMaterializationApplyDecision, SelectionMaterializationRequest,
    TableClipboardSnapshot, TextSurface, TextSurfaceCapabilities, TextSurfaceEditResult,
    TextSurfaceRegistry, TextSurfaceRole, TextSurfaceSnapshot, TextSurfaceSnapshotIdentity,
    TransactionApplyError,
};
pub use editing::composition::{
    CompositionCancelResult, CompositionCommitResult, CompositionController, CompositionError,
    CompositionPreviewResult, CompositionState as RuntimeCompositionState,
};
pub use editing::hot_path::{
    AsyncTaskKind, AsyncTaskQueue, ForbiddenSyncWorkGuard, IncrementalLayoutRequest, InlineAttrs,
    InlineRun, InputHotPathConfig, InputHotPathError, InputHotPathResult, LayoutDirtyRange,
    LayoutDirtyReason, PieceTableTextModel, ScheduledAsyncTask, SingleCharInputHotPath,
};
pub use editing::session::{
    CaretGeometryVersion, CompositionBaseSelection, CompositionState, EditingPriority,
    EditingSession, EditingSessionError, InputSessionIdentity, InputTarget, LayoutCachePin,
    TextLayoutVersion,
};
pub use projection::list::{
    BlockListProjectionEntry, ListProjectionCache, project_block_list_entry,
};
pub use projection::view::{
    AiPreviewKind, AiPreviewSnapshot, AiPreviewStatus, EditorViewProjection,
    PayloadWindowFailureView, TableCellPosition, TableViewState, TableVisibleCell,
    ViewBlockSnapshot,
};
pub use scheduling::async_version_control::{
    AsyncLayoutVersion, AsyncResultDecision, AsyncTaskKind as RuntimeAsyncTaskKind,
    AsyncVersionController, DiscardReason, HistoricalLayoutHint, LayoutTaskRequest,
    LayoutTaskResult, PageWindowRequest, PageWindowResult,
};
pub use scheduling::layout_scheduler::{
    LayoutFrameResult, LayoutScheduler, LayoutSchedulerConfig, LayoutSchedulerDebugOverlay,
    LayoutTask, LayoutTaskKind, LayoutTaskLane, LayoutTaskOutcome, ScheduleDecision,
};
pub use scheduling::main_thread_budget::{
    FrameBudgetState, FrameRunResult, InteractionMode, MainThreadBudget, MainThreadBudgetArbiter,
    MainThreadTask, MainThreadWorkKind, QueueDecision, TaskOutcome, WorkCost,
};
pub use scheduling::worker_pool_policy::{
    WorkerDispatchBatch, WorkerEnqueueDecision, WorkerLane, WorkerPoolDebugOverlay,
    WorkerPoolPolicy, WorkerPoolScheduler, WorkerTask, WorkerTaskKind,
};
