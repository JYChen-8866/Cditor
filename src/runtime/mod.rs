pub mod async_version_control;
pub mod composition;
pub mod demo_fixtures;
pub mod document_query_index;
pub mod document_runtime;
pub mod editing_acceptance;
pub mod editing_session;
pub mod external_content_security;
pub mod input_hot_path;
pub mod layout_scheduler;
pub mod list_projection;
pub mod main_thread_budget;
pub mod media_cache;
pub mod open_acceptance;
pub mod paste_import_pipeline;
pub mod payload_window;
pub mod scroll_acceptance;
pub mod structure_edit_acceptance;
pub mod view_projection;
pub mod worker_pool_policy;

pub use async_version_control::{
    AsyncLayoutVersion, AsyncResultDecision, AsyncTaskKind as RuntimeAsyncTaskKind,
    AsyncVersionController, DiscardReason, HistoricalLayoutHint, LayoutTaskRequest,
    LayoutTaskResult, PageWindowRequest, PageWindowResult,
};
pub use composition::{
    CompositionCancelResult, CompositionCommitResult, CompositionController, CompositionError,
    CompositionPreviewResult, CompositionState as RuntimeCompositionState,
};
pub use demo_fixtures::{
    LARGE_MIXED_DEMO_BLOCKS, LARGE_MIXED_DEMO_DOCUMENT_ID, large_mixed_demo_document,
    large_mixed_rich_text_document,
};
pub use document_query_index::{
    BLOCK_FTS_SCHEMA, BlockPayloadForQuery, DocumentQueryIndex, FtsApplyResult, FtsEntry,
    FtsUpdateTask, QueryResult, QueryScrollTarget,
};
pub use document_runtime::DocumentRuntime;
pub use editing_acceptance::{
    EditingAcceptanceConfig, EditingAcceptanceResult, EditingAcceptanceScenario,
    run_editing_acceptance,
};
pub use editing_session::{
    CaretGeometryVersion, CompositionState, EditingPriority, EditingSession, EditingSessionError,
    LayoutCachePin, TextLayoutVersion,
};
pub use external_content_security::{
    DataUrlPolicy, EmbedPolicy, ExternalContentPolicy, ExternalResourceAction,
    ExternalResourceDecision, ExternalResourceKind, FileUrlPolicy, PrivacyMode,
    RemoteResourcePolicy, SanitizedHtml, SvgPolicy, sanitize_external_html,
};
pub use input_hot_path::{
    AsyncTaskKind, AsyncTaskQueue, ForbiddenSyncWorkGuard, IncrementalLayoutRequest, InlineAttrs,
    InlineRun, InputHotPathConfig, InputHotPathError, InputHotPathResult, LayoutDirtyRange,
    LayoutDirtyReason, PieceTableTextModel, ScheduledAsyncTask, SingleCharInputHotPath,
};
pub use layout_scheduler::{
    LayoutFrameResult, LayoutScheduler, LayoutSchedulerConfig, LayoutSchedulerDebugOverlay,
    LayoutTask, LayoutTaskKind, LayoutTaskLane, LayoutTaskOutcome, ScheduleDecision,
};
pub use list_projection::{
    BlockListProjectionEntry, ListProjectionCache, project_block_list_entry,
};
pub use main_thread_budget::{
    FrameBudgetState, FrameRunResult, InteractionMode, MainThreadBudget, MainThreadBudgetArbiter,
    MainThreadTask, MainThreadWorkKind, QueueDecision, TaskOutcome, WorkCost,
};
pub use media_cache::{
    MediaCache, MediaCacheEntry, MediaCachePolicy, MediaCacheStats, MediaDecodeDecision,
    MediaDecodeKind, MediaDecodeLane, MediaDecodeRequest, MediaDecodeTrigger, MediaMetadata,
    MediaResourceId, MediaStableBox, MemoryPressure,
};
pub use open_acceptance::{
    AcceptanceFixture, AcceptanceFixtureKind, OpenAcceptanceConfig, OpenAcceptanceResult,
    TextProfile, fixture_10mb_code_block, fixture_50k_row_table, fixture_100k_one_line_blocks,
    fixture_100k_uneven_heights, fixture_emoji_cjk_bidi, fixture_image_dense, run_open_acceptance,
};
pub use paste_import_pipeline::{
    ClipboardInput, MediaMetadataTask, NormalizedPasteBlock, PasteImportConfig,
    PasteImportPipeline, PasteImportResult, PastePipelinePhase, PasteProgress, PasteRunOptions,
    PayloadPersistTask, PendingMediaResource,
};
pub use payload_window::PayloadWindow;
pub use scroll_acceptance::{
    ScrollAcceptanceConfig, ScrollAcceptanceResult, ScrollAcceptanceScenario,
    evaluate_scroll_trace, run_scroll_acceptance,
};
pub use structure_edit_acceptance::{
    StructureEditAcceptanceConfig, StructureEditAcceptanceResult, StructureEditScenario,
    run_structure_edit_acceptance,
};
pub use view_projection::{EditorViewProjection, ViewBlockSnapshot};
pub use worker_pool_policy::{
    WorkerDispatchBatch, WorkerEnqueueDecision, WorkerLane, WorkerPoolDebugOverlay,
    WorkerPoolPolicy, WorkerPoolScheduler, WorkerTask, WorkerTaskKind,
};
