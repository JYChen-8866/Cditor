//! Headless application service for one open editor document.
//!
//! The session is the single owner of `DocumentRuntime`. Hosts interact with
//! it through a cloneable, same-thread handle that exposes typed protocol
//! operations without returning runtime borrows.

mod render_port;
mod session;
mod toolbar_snapshot;
mod ui_snapshot;

pub use render_port::{RenderFrameRequest, RenderFrameSnapshot};
pub use session::{
    EditorSession, EditorSessionHandle, SessionId, SessionRealtimeError, SessionSnapshot,
};
pub use toolbar_snapshot::{
    GutterToolbarSnapshot, ToolbarContextRequest, ToolbarContextSnapshot,
    VersionedSelectionFragment, project_toolbar_context,
};
pub use ui_snapshot::SessionUiSnapshot;
