use cditor_core::edit::DocumentSelection;
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::DocumentRuntime;

use crate::EditorSessionHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDocumentSnapshot {
    pub document_id: u64,
    pub title: Option<String>,
    pub revision: u64,
    pub block_count: usize,
    pub readonly: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub selection: Option<DocumentSelection>,
}

pub fn project_document_snapshot(
    runtime: &DocumentRuntime,
    readonly: bool,
) -> SessionDocumentSnapshot {
    SessionDocumentSnapshot {
        document_id: runtime.document_id(),
        title: runtime.document_title().map(ToOwned::to_owned),
        revision: runtime.revision(),
        block_count: runtime.document_block_count(),
        readonly,
        can_undo: !readonly && runtime.can_undo(),
        can_redo: !readonly && runtime.can_redo(),
        selection: runtime.document_selection_snapshot(),
    }
}

pub fn project_selected_text(runtime: &DocumentRuntime) -> Option<String> {
    runtime.selected_focused_text()
}

impl EditorSessionHandle {
    pub fn document_snapshot(&self) -> Result<SessionDocumentSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_document_snapshot(
            &session.runtime,
            session.readonly,
        ))
    }

    pub fn selected_text(&self) -> Result<Option<String>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_selected_text(&session.runtime))
    }
}

#[cfg(test)]
mod tests {
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

    use super::*;
    use crate::EditorSession;

    #[test]
    fn document_snapshot_owns_metadata_and_applies_readonly_history_policy() {
        let runtime = DocumentRuntime::demo();
        let block_id = runtime.visible_block_ids()[0];
        let handle = EditorSession::new(runtime, true).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id },
                CommandSource::Sdk,
            ))
            .unwrap();

        let snapshot = handle.document_snapshot().unwrap();
        assert_eq!(snapshot.document_id, handle.snapshot().unwrap().document_id);
        assert!(snapshot.readonly);
        assert!(!snapshot.can_undo);
        assert!(!snapshot.can_redo);
        assert!(snapshot.selection.is_some());
    }
}
