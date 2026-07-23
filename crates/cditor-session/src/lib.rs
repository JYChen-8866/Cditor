//! Headless application service for one open editor document.
//!
//! The session is the single owner of `DocumentRuntime`. Hosts interact with
//! it through a cloneable, same-thread handle that exposes typed protocol
//! operations without returning runtime borrows.

mod ai_port;
mod cold_start;
mod diagnostics_snapshot;
mod document_snapshot;
mod history_port;
mod input_port;
mod layout_port;
mod persistence_port;
mod render_port;
mod selection_materialization_port;
mod session;
mod surface_port;
mod table_port;
mod toolbar_snapshot;
mod ui_snapshot;

pub use ai_port::{AiContextSnapshot, project_ai_context, project_ai_session_request};
pub use cold_start::{SessionColdStartRequest, SessionColdStartResult, open_editor_session};
pub use diagnostics_snapshot::{SessionDiagnosticsSnapshot, project_diagnostics_snapshot};
pub use document_snapshot::{
    SessionDocumentSnapshot, TextBlockContextSnapshot, project_block_attrs,
    project_document_snapshot, project_focused_text_block_context, project_selected_text,
    project_text_block_context, project_visible_block_subset, project_whiteboard_scene,
};
pub use history_port::{
    HistoryActionSnapshot, HistoryDirection, UndoBlobSpillApplySnapshot, UndoBlobWriteResult,
    project_apply_undo_blob_write_result, project_begin_undo_blob_cleanup,
    project_begin_undo_blob_spill, project_finish_undo_blob_cleanup, project_history_action,
    project_hydrated_history_action,
};
pub use input_port::{
    FocusedPlatformTextSnapshot, InputCompositionSnapshot, InputContextSnapshot,
    project_input_context,
};
pub use layout_port::{
    LayoutScrollSnapshot, LayoutViewportSnapshot, project_begin_scrollbar_drag,
    project_drag_scrollbar, project_finish_scrollbar_drag, project_layout_viewport,
    project_measured_block_height, project_scroll_by_delta, project_scroll_focused_block_into_view,
    project_scroll_input_frame, project_scroll_to_block,
};
pub use persistence_port::{
    PersistenceCaptureRequest, PersistenceSaveCapture, project_persistence_save_capture,
    project_persistence_save_failure, project_persistence_save_success,
};
pub use render_port::{
    RenderFrameRequest, RenderFrameSnapshot, RenderFrameWarnings, activate_resident_payload_window,
    apply_payload_window_error, apply_payload_window_result, apply_table_horizontal_scroll_offset,
    plan_payload_window_load, project_render_frame, retry_failed_payload_window,
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
pub use surface_port::{SurfaceVersionSnapshot, project_surface_version};
pub use table_port::{
    FocusedTableCellSnapshot, TableInteractionSnapshot, TablePayloadSummary, TableRangeRequest,
    project_table_horizontal_scroll_offset, project_table_interaction, project_table_range,
};
pub use toolbar_snapshot::{
    GutterToolbarSnapshot, ToolbarContextRequest, ToolbarContextSnapshot,
    VersionedSelectionFragment, project_toolbar_context,
};
pub use ui_snapshot::SessionUiSnapshot;
