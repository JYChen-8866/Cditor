use cditor_core::{edit::DocumentSelection, ids::BlockId, rich_text::RichBlockKind};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBlockContextSnapshot {
    pub block_id: BlockId,
    pub kind: RichBlockKind,
    pub text: String,
    pub caret: Option<usize>,
    pub content_version: u64,
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

pub fn project_text_block_context(
    runtime: &DocumentRuntime,
    block_id: BlockId,
) -> Option<TextBlockContextSnapshot> {
    let payload = runtime.block_payload_record(block_id)?;
    let text = payload.plain_text();
    Some(TextBlockContextSnapshot {
        block_id,
        kind: payload.kind,
        text,
        caret: runtime.caret_offset_for_block(block_id),
        content_version: payload.content_version,
    })
}

pub fn project_focused_text_block_context(
    runtime: &DocumentRuntime,
) -> Option<TextBlockContextSnapshot> {
    project_text_block_context(runtime, runtime.focused_block_id()?)
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

    pub fn text_block_context(
        &self,
        block_id: BlockId,
    ) -> Result<Option<TextBlockContextSnapshot>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_text_block_context(&session.runtime, block_id))
    }

    pub fn focused_text_block_context(
        &self,
    ) -> Result<Option<TextBlockContextSnapshot>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_focused_text_block_context(&session.runtime))
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

    #[test]
    fn text_block_context_is_owned_and_bounded_to_one_payload() {
        let runtime = DocumentRuntime::demo();
        let block_id = runtime.visible_block_ids()[0];
        let expected = runtime.block_payload_record(block_id).unwrap();
        let handle = EditorSession::new(runtime, false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id },
                CommandSource::Sdk,
            ))
            .unwrap();

        let context = handle.focused_text_block_context().unwrap().unwrap();
        assert_eq!(context.block_id, block_id);
        assert_eq!(context.kind, expected.kind);
        assert_eq!(context.text, expected.plain_text());
        assert_eq!(context.content_version, expected.content_version);
        assert!(context.caret.is_some());
    }
}
