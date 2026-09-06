//! Headless application service for one open editor document.
//!
//! The session is the single owner of `DocumentRuntime`. Hosts interact with
//! it through a cloneable, same-thread handle that exposes typed protocol
//! operations without returning runtime borrows.

mod agent_bridge;
mod agent_chat_provider;
mod ai_port;
mod clipboard_port;
mod cold_start;
mod diagnostics_snapshot;
mod document_persistence;
mod document_snapshot;
mod emergency_log;
mod history_port;
mod import_port;
mod input_port;
mod io_executor;
mod layout_port;
mod materialized_rebuild;
mod persistence_failure;
mod persistence_pipeline;
mod persistence_port;
mod persistence_session;
mod remote_transaction;
mod render_port;
mod selection_materialization_port;
mod session;
mod storage_io;
mod surface_port;
mod table_port;
mod task_port;
mod toolbar_snapshot;
mod ui_snapshot;

pub use agent_bridge::{
    AgentEditOperation, AgentEditOutcome, AgentEditRequest, AgentOutline, AgentOutlineRequest,
    project_agent_edit, project_agent_outline,
};
pub use ai_port::{AiContextSnapshot, project_ai_context, project_ai_session_request};
pub use clipboard_port::SessionClipboardSnapshot;
pub use cold_start::{
    PreparedEditorSession, PreparedSessionColdStartResult, SessionColdStartRequest,
    SessionColdStartResult, open_editor_session, open_editor_session_with_persistence,
    prepare_editor_session_with_persistence,
};
pub use diagnostics_snapshot::{SessionDiagnosticsSnapshot, project_diagnostics_snapshot};
pub use document_persistence::DocumentPersistence;
pub use document_snapshot::{
    SessionDocumentSnapshot, TextBlockContextSnapshot, project_block_attrs,
    project_document_snapshot, project_focused_block_kind, project_focused_text_block_context,
    project_selected_text, project_text_block_context, project_visible_block_subset,
    project_whiteboard_scene,
};
pub use emergency_log::{
    EMERGENCY_EXPORT_FORMAT, EMERGENCY_EXPORT_VERSION, EmergencyExportArtifact,
    EmergencyRecoveryDecision, EmergencyRecoveryPlan, EmergencyRecoveryReport,
    MAX_EMERGENCY_AFFECTED_BLOCKS, MAX_EMERGENCY_LOG_BYTES, MAX_EMERGENCY_LOG_ENTRIES,
    decode_emergency_export, plan_emergency_recovery, project_emergency_export,
    project_emergency_payload_request, project_emergency_payload_result,
    project_emergency_recovery,
};
pub use history_port::{
    HistoryActionSnapshot, HistoryDirection, UndoBlobSpillApplySnapshot, UndoBlobWriteResult,
    project_apply_undo_blob_write_result, project_begin_undo_blob_cleanup,
    project_begin_undo_blob_spill, project_finish_undo_blob_cleanup, project_history_action,
    project_hydrated_history_action,
};
pub use import_port::{
    ImportDispatchReport, project_ai_preview_import, project_clipboard_import,
    project_markdown_import,
};
pub use input_port::{
    FocusedPlatformTextSnapshot, InputCompositionSnapshot, InputContextSnapshot,
    project_input_context,
};
pub use io_executor::SessionIoExecutor;
pub use layout_port::{
    LayoutScrollSnapshot, LayoutViewportSnapshot, project_apply_animated_block_height,
    project_apply_measured_block_height, project_begin_block_height_animation,
    project_begin_scrollbar_drag, project_drag_scrollbar, project_drag_scrollbar_to_ratio,
    project_end_block_height_animation, project_finish_block_height_animation,
    project_finish_scrollbar_drag, project_flush_scroll_input_frame, project_layout_viewport,
    project_measured_block_height, project_queue_scroll_input, project_scroll_by_delta,
    project_scroll_focused_block_into_view, project_scroll_input_frame, project_scroll_to_block,
};
pub use materialized_rebuild::prepare_editor_session_from_rebuild_plan;
pub use persistence_failure::{PersistenceFailure, PersistenceFailureKind};
pub use persistence_pipeline::{
    DEFAULT_STORAGE_SAVE_DEBOUNCE, PersistenceBarrierKind, PersistenceBarrierReport,
    PersistencePipeline, PersistencePipelineError, ReadyPersistenceBarrier, StorageSaveRequest,
    run_storage_save_with_timeout, save_storage_batch,
};
pub use persistence_port::{
    PersistenceCaptureRequest, PersistenceRuntimeSnapshot, PersistenceSaveCapture,
    project_note_content_changed, project_persistence_runtime_snapshot,
    project_persistence_save_capture, project_persistence_save_failure,
    project_persistence_save_success,
};
pub use persistence_session::{
    PersistenceSaveApply, PersistenceSessionSnapshot, StorageFlushRequest, execute_storage_flush,
    run_storage_flush_with_timeout,
};
pub use remote_transaction::{RemoteTransactionSnapshot, project_remote_transaction};
pub use render_port::{
    RenderFrameRequest, RenderFrameSnapshot, RenderFrameWarnings, activate_resident_payload_window,
    apply_table_horizontal_scroll_offset, project_render_frame, retry_failed_payload_window,
    trim_payload_cache,
};
pub use selection_materialization_port::{
    SelectionMaterializationSnapshot, project_selection_materialization_result,
};
pub use session::{
    CommandDispatchSnapshot, EditorSession, EditorSessionHandle, SessionId, SessionRealtimeError,
    SessionSnapshot, project_block_plain_text, project_command_dispatch, project_command_query,
    project_end_input_batch, project_realtime_input,
};
pub use storage_io::{
    PayloadStorageRequest, UndoBlobDeleteRequest, UndoBlobReadRequest, UndoBlobWriteRequest,
    execute_payload_load, execute_undo_blob_delete, execute_undo_blob_read,
    execute_undo_blob_write, run_payload_load, run_undo_blob_delete, run_undo_blob_read,
    run_undo_blob_write,
};
pub use surface_port::{
    SurfaceVersionSnapshot, TextSurfaceStateSnapshot, project_surface_version,
    project_text_surface_state,
};
pub use table_port::{
    FocusedTableCellSnapshot, TableInteractionSnapshot, TablePayloadSummary, TableRangeRequest,
    project_table_horizontal_scroll_offset, project_table_interaction, project_table_range,
};
pub use task_port::{
    PayloadWindowTaskSchedule, SessionTaskAdmission, SessionTaskKind, SessionTaskLane,
    SessionTaskToken,
};
pub use toolbar_snapshot::{
    GutterToolbarSnapshot, ToolbarContextRequest, ToolbarContextSnapshot,
    VersionedSelectionFragment, project_toolbar_context,
};
pub use ui_snapshot::SessionUiSnapshot;
