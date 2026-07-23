//! Headless application service for one open editor document.
//!
//! The session is the single owner of `DocumentRuntime`. Hosts interact with
//! it through a cloneable, same-thread handle that exposes typed protocol
//! operations without returning runtime borrows.

mod ai_port;
mod history_port;
mod render_port;
mod session;
mod toolbar_snapshot;
mod ui_snapshot;

pub use ai_port::project_ai_session_request;
pub use history_port::{
    HistoryActionSnapshot, HistoryDirection, project_history_action,
    project_hydrated_history_action,
};
pub use render_port::{
    RenderFrameRequest, RenderFrameSnapshot, RenderFrameWarnings, activate_resident_payload_window,
    apply_table_horizontal_scroll_offset, plan_payload_window_load, project_render_frame,
};
pub use session::{
    CommandDispatchSnapshot, EditorSession, EditorSessionHandle, SessionId, SessionRealtimeError,
    SessionSnapshot, project_block_plain_text, project_command_dispatch, project_command_query,
    project_end_input_batch, project_realtime_input,
};
pub use toolbar_snapshot::{
    GutterToolbarSnapshot, ToolbarContextRequest, ToolbarContextSnapshot,
    VersionedSelectionFragment, project_toolbar_context,
};
pub use ui_snapshot::SessionUiSnapshot;
