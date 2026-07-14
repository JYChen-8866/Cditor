pub mod acceptance;
pub mod content;
pub mod document_runtime;
pub mod editing;
pub mod projection;
pub mod scheduling;

pub use acceptance::editing::{
    EditingAcceptanceConfig, EditingAcceptanceResult, EditingAcceptanceScenario,
    run_editing_acceptance,
};
pub use acceptance::open::{
    AcceptanceFixture, AcceptanceFixtureKind, OpenAcceptanceConfig, OpenAcceptanceResult,
    TextProfile, fixture_10mb_code_block, fixture_50k_row_table, fixture_100k_one_line_blocks,
    fixture_100k_uneven_heights, fixture_emoji_cjk_bidi, fixture_image_dense, run_open_acceptance,
};
pub use acceptance::scroll::{
    ScrollAcceptanceConfig, ScrollAcceptanceResult, ScrollAcceptanceScenario,
    evaluate_scroll_trace, run_scroll_acceptance,
};
pub use acceptance::structure_edit::{
    StructureEditAcceptanceConfig, StructureEditAcceptanceResult, StructureEditScenario,
    run_structure_edit_acceptance,
};
pub use cditor_core::demo_fixtures::{
    LARGE_MIXED_DEMO_BLOCKS, LARGE_MIXED_DEMO_DOCUMENT_ID, large_mixed_demo_document,
    large_mixed_rich_text_document,
};
pub use content::media_cache::{
    MediaCache, MediaCacheEntry, MediaCachePolicy, MediaCacheStats, MediaDecodeDecision,
    MediaDecodeKind, MediaDecodeLane, MediaDecodeRequest, MediaDecodeTrigger, MediaMetadata,
    MediaResourceId, MediaStableBox, MemoryPressure,
};
pub use content::paste_import::{
    ClipboardInput, MediaMetadataTask, NormalizedPasteBlock, PasteImportConfig,
    PasteImportPipeline, PasteImportResult, PastePipelinePhase, PasteProgress, PasteRunOptions,
    PayloadPersistTask, PendingMediaResource,
};
pub use content::payload_window::PayloadWindow;
pub use content::query_index::{
    BLOCK_FTS_SCHEMA, BlockPayloadForQuery, DocumentQueryIndex, FtsApplyResult, FtsEntry,
    FtsUpdateTask, QueryResult, QueryScrollTarget,
};
pub use content::security::{
    DataUrlPolicy, EmbedPolicy, ExternalContentPolicy, ExternalResourceAction,
    ExternalResourceDecision, ExternalResourceKind, FileUrlPolicy, PrivacyMode,
    RemoteResourcePolicy, SanitizedHtml, SvgPolicy, sanitize_external_html,
};
pub use document_runtime::{
    AiApplyMode, AiRequestDispatch, AiRequestPresentation, AiSessionSnapshot, AiSessionStatus,
    AiStreamApplyResult, DocumentRuntime, DocumentTextSelectionFragment, RichTextSelectionSnapshot,
    RuntimeAiTarget, TableClipboardSnapshot,
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
    CaretGeometryVersion, CompositionState, EditingPriority, EditingSession, EditingSessionError,
    InputTarget, LayoutCachePin, TextLayoutVersion,
};
pub use projection::list::{
    BlockListProjectionEntry, ListProjectionCache, project_block_list_entry,
};
pub use projection::view::{
    AiPreviewKind, AiPreviewSnapshot, AiPreviewStatus, EditorViewProjection, TableCellPosition,
    TableViewState, TableVisibleCell, ViewBlockSnapshot,
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
