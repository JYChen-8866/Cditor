use cditor_core::{ids::BlockId, rich_text::BlockPayloadRecord};
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::{
    DocumentRuntime, SelectionMaterializationApplyDecision, SelectionMaterializationRequest,
};

use crate::EditorSessionHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionMaterializationSnapshot {
    pub decision: SelectionMaterializationApplyDecision,
    pub replay_ready: bool,
}

pub fn project_selection_materialization_result(
    runtime: &mut DocumentRuntime,
    request: &SelectionMaterializationRequest,
    records: Vec<BlockPayloadRecord>,
    missing_block_ids: &[BlockId],
) -> SelectionMaterializationSnapshot {
    let decision =
        runtime.apply_selection_materialization_result(request, records, missing_block_ids);
    let replay_ready = missing_block_ids.is_empty()
        && decision == SelectionMaterializationApplyDecision::Applied
        && runtime.selection_materialization_request().is_none();
    SelectionMaterializationSnapshot {
        decision,
        replay_ready,
    }
}

impl EditorSessionHandle {
    pub fn selection_materialization_request(
        &self,
    ) -> Result<Option<SelectionMaterializationRequest>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(session.runtime.selection_materialization_request())
    }

    pub fn apply_selection_materialization_result(
        &self,
        request: &SelectionMaterializationRequest,
        records: Vec<BlockPayloadRecord>,
        missing_block_ids: &[BlockId],
    ) -> Result<SelectionMaterializationSnapshot, ProtocolError> {
        Ok(project_selection_materialization_result(
            &mut self.try_session_mut()?.runtime,
            request,
            records,
            missing_block_ids,
        ))
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::edit::TextPosition;
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};

    use super::*;
    use crate::EditorSession;

    fn current_request(runtime: &DocumentRuntime) -> SelectionMaterializationRequest {
        SelectionMaterializationRequest {
            document_id: runtime.document_id(),
            structure_version: runtime.structure_version(),
            selection: runtime.unified_document_selection_snapshot().unwrap(),
            block_ids: vec![1],
            payload_window_generation: runtime.payload_window_generation(),
        }
    }

    fn runtime_with_selection() -> DocumentRuntime {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "text",
            )],
            720.0,
        );
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::SetDocumentSelection {
                    selection: cditor_core::edit::DocumentSelection {
                        anchor: TextPosition::downstream(1, 0),
                        focus: TextPosition::downstream(1, 1),
                    },
                },
                CommandSource::Sdk,
            ))
            .unwrap();
        runtime
    }

    #[test]
    fn current_complete_result_is_atomically_ready_for_replay() {
        let runtime = runtime_with_selection();
        let request = current_request(&runtime);
        let handle = EditorSession::new(runtime, false).into_handle();

        let snapshot = handle
            .apply_selection_materialization_result(&request, Vec::new(), &[])
            .unwrap();
        assert_eq!(
            snapshot.decision,
            SelectionMaterializationApplyDecision::Applied
        );
        assert!(snapshot.replay_ready);
    }

    #[test]
    fn stale_or_incomplete_result_never_requests_replay() {
        let runtime = runtime_with_selection();
        let mut request = current_request(&runtime);
        request.structure_version = request.structure_version.saturating_add(1);
        let handle = EditorSession::new(runtime, false).into_handle();
        let stale = handle
            .apply_selection_materialization_result(&request, Vec::new(), &[])
            .unwrap();
        assert_eq!(
            stale.decision,
            SelectionMaterializationApplyDecision::DiscardedStale
        );
        assert!(!stale.replay_ready);

        let mut runtime = runtime_with_selection();
        let request = current_request(&runtime);
        let incomplete =
            project_selection_materialization_result(&mut runtime, &request, Vec::new(), &[1]);
        assert_eq!(
            incomplete.decision,
            SelectionMaterializationApplyDecision::Applied
        );
        assert!(!incomplete.replay_ready);
    }
}
