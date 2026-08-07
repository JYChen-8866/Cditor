use std::ops::Range;

use cditor_core::ids::BlockId;
use cditor_runtime::{InputSessionIdentity, InputTarget};

use crate::{EditorSessionHandle, SessionSnapshot};

/// Small, owned state read by GPUI interaction and overlay code.
///
/// Text payloads and render blocks deliberately remain in dedicated bounded
/// projections. This snapshot contains only scalar session state and short
/// selection ranges, so polling it cannot clone document content.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionUiSnapshot {
    pub session: SessionSnapshot,
    pub structure_version: u64,
    pub global_scroll_top: f64,
    /// Current document height after estimated and measured block layout.
    pub content_height: f64,
    pub focused_block_id: Option<BlockId>,
    pub focused_table_cell: Option<(BlockId, usize, usize, usize)>,
    pub input_target: Option<InputTarget>,
    pub input_identity: Option<InputSessionIdentity>,
    pub input_selected_range: Option<Range<usize>>,
    pub input_marked_range: Option<Range<usize>>,
    pub input_selection_reversed: bool,
    pub has_document_text_selection: bool,
    pub has_cross_block_text_selection: bool,
    pub has_entire_document_text_selection: bool,
    pub has_active_selection: bool,
    pub ai_session_active: bool,
    pub can_begin_ai_request: bool,
}

impl EditorSessionHandle {
    pub fn ui_snapshot(&self) -> Result<SessionUiSnapshot, cditor_editor_protocol::ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            cditor_editor_protocol::ProtocolError::new(
                cditor_editor_protocol::ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        let runtime = &session.runtime;
        Ok(SessionUiSnapshot {
            session: session.snapshot(),
            structure_version: runtime.structure_version(),
            global_scroll_top: runtime.global_scroll_top(),
            content_height: runtime.model_total_height(),
            focused_block_id: runtime.focused_block_id(),
            focused_table_cell: runtime.focused_table_cell_offset(),
            input_target: runtime.input_session_target(),
            input_identity: runtime.input_session_identity(),
            input_selected_range: runtime.input_session_selected_range(),
            input_marked_range: runtime.input_session_marked_range(),
            input_selection_reversed: runtime.input_session_selection_reversed(),
            has_document_text_selection: runtime.has_document_text_selection(),
            has_cross_block_text_selection: runtime.has_cross_block_text_selection(),
            has_entire_document_text_selection: runtime.has_entire_document_text_selection(),
            has_active_selection: runtime.has_active_selection(),
            ai_session_active: runtime.ai_session_snapshot().is_some(),
            can_begin_ai_request: runtime.can_begin_ai_request(),
        })
    }
}

#[cfg(test)]
mod tests {
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
    use cditor_runtime::DocumentRuntime;

    use super::*;
    use crate::EditorSession;

    #[test]
    fn ui_snapshot_tracks_focus_without_cloning_document_payloads() {
        let runtime = DocumentRuntime::demo();
        let block_id = runtime.visible_block_ids()[0];
        let handle = EditorSession::new(runtime, false).into_handle();
        handle
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id },
                CommandSource::Automation,
            ))
            .unwrap();

        let snapshot = handle.ui_snapshot().unwrap();
        assert_eq!(snapshot.focused_block_id, Some(block_id));
        assert_eq!(
            snapshot.input_target.map(InputTarget::block_id),
            Some(block_id)
        );
        assert_eq!(
            snapshot.session.revision,
            handle.snapshot().unwrap().revision
        );
    }
}
