use cditor_core::clipboard::ClipboardSelection;
use cditor_core::{ids::BlockId, rich_text::TableRange};
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};

use crate::EditorSessionHandle;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionClipboardSnapshot {
    pub document_id: u64,
    pub selection: Option<ClipboardSelection>,
    pub selected_text: Option<String>,
}

impl EditorSessionHandle {
    pub fn clipboard_snapshot(&self) -> Result<SessionClipboardSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| busy_error())?;
        Ok(SessionClipboardSnapshot {
            document_id: session.runtime.document_id(),
            selection: session.runtime.clipboard_selection_snapshot(),
            selected_text: session.runtime.selected_focused_text(),
        })
    }

    pub fn table_clipboard_selection(
        &self,
        block_id: BlockId,
        range: TableRange,
    ) -> Result<Option<ClipboardSelection>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| busy_error())?;
        Ok(session
            .runtime
            .table_clipboard_for_range(block_id, range)
            .map(|snapshot| ClipboardSelection::Table {
                table: snapshot.table,
            }))
    }
}

fn busy_error() -> ProtocolError {
    ProtocolError::new(
        ProtocolErrorCode::Busy,
        "editor session is already processing a synchronous request",
    )
    .retryable()
}

#[cfg(test)]
mod tests {
    use cditor_core::{
        edit::{DocumentSelection, TextPosition},
        rich_text::{BlockPayloadRecord, RichBlockKind},
    };
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
    use cditor_runtime::DocumentRuntime;

    use crate::EditorSession;

    #[test]
    fn clipboard_snapshot_is_owned_and_document_scoped() {
        let mut runtime = DocumentRuntime::from_payloads(
            7,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "hello",
            )],
            720.0,
        );
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::SetDocumentSelection {
                    selection: DocumentSelection {
                        anchor: TextPosition::downstream(1, 1),
                        focus: TextPosition::downstream(1, 4),
                    },
                },
                CommandSource::Automation,
            ))
            .unwrap();
        let handle = EditorSession::new(runtime, false).into_handle();

        let snapshot = handle.clipboard_snapshot().unwrap();
        assert_eq!(snapshot.document_id, 7);
        assert_eq!(snapshot.selected_text.as_deref(), Some("ell"));
        assert!(snapshot.selection.is_some());
    }
}
