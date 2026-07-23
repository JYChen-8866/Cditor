use std::ops::Range;

use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::DocumentRuntime;

use crate::EditorSessionHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDiagnosticsSnapshot {
    pub revision: u64,
    pub editing_active: bool,
    pub pending_layout_tasks: usize,
    pub pending_payload_loads: usize,
    pub document_blocks: usize,
    pub payload_window: Range<usize>,
    pub page_window: Range<usize>,
    pub loaded_payloads: usize,
    pub payload_and_undo_bytes: usize,
    pub payload_cache_over_budget: bool,
}

impl EditorSessionHandle {
    pub fn diagnostics_snapshot(&self) -> Result<SessionDiagnosticsSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_diagnostics_snapshot(&session.runtime))
    }
}

pub fn project_diagnostics_snapshot(runtime: &DocumentRuntime) -> SessionDiagnosticsSnapshot {
    let estimated_payload_bytes = runtime.estimated_payload_memory_bytes();
    let loaded_payloads = runtime.loaded_payload_count();
    SessionDiagnosticsSnapshot {
        revision: runtime.revision(),
        editing_active: runtime.input_session_target().is_some(),
        pending_layout_tasks: runtime.pending_layout_task_count(),
        pending_payload_loads: runtime.pending_payload_load_count(),
        document_blocks: runtime.document_block_count(),
        payload_window: runtime.payload_window_range(),
        page_window: runtime.current_page_window(),
        loaded_payloads,
        payload_and_undo_bytes: estimated_payload_bytes
            .saturating_add(runtime.estimated_text_undo_memory_bytes()),
        payload_cache_over_budget: loaded_payloads
            > cditor_runtime::DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_ENTRIES
            || estimated_payload_bytes > cditor_runtime::DEFAULT_POSTGRES_PAYLOAD_CACHE_MAX_BYTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditorSession;

    #[test]
    fn diagnostics_snapshot_is_scalar_and_revisioned() {
        let handle = EditorSession::new(DocumentRuntime::large_mixed_demo(), false).into_handle();
        let snapshot = handle.diagnostics_snapshot().unwrap();
        let session = handle.snapshot().unwrap();

        assert_eq!(snapshot.revision, session.revision);
        assert_eq!(snapshot.document_blocks, session.block_count);
        assert!(snapshot.loaded_payloads > 0);
        assert!(snapshot.payload_window.end >= snapshot.payload_window.start);
        assert!(snapshot.page_window.end >= snapshot.page_window.start);
    }
}
