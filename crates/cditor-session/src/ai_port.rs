use cditor_core::ids::BlockId;
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::{
    AiApplyMode, AiSessionOutcome, AiSessionRequest, DocumentRuntime, RuntimeAiTarget,
};

use crate::EditorSessionHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiContextSnapshot {
    pub session_active: bool,
    pub focused_empty_text_block: Option<(BlockId, usize)>,
    pub prompt_block_id: Option<BlockId>,
    pub apply_mode: Option<AiApplyMode>,
}

pub fn project_ai_context(runtime: &DocumentRuntime) -> AiContextSnapshot {
    let session = runtime.ai_session_snapshot();
    let target = session.as_ref().map(|session| &session.target);
    AiContextSnapshot {
        session_active: session.is_some(),
        focused_empty_text_block: runtime.focused_empty_text_block_for_ai(),
        prompt_block_id: target
            .map(target_block_id)
            .or_else(|| runtime.focused_block_id()),
        apply_mode: target.map(|target| match target {
            RuntimeAiTarget::InlineCaret(_) => AiApplyMode::InsertAfter,
            RuntimeAiTarget::TextSelection(_) => AiApplyMode::Replace,
        }),
    }
}

fn target_block_id(target: &RuntimeAiTarget) -> BlockId {
    match target {
        RuntimeAiTarget::InlineCaret(position) => position.block_id,
        RuntimeAiTarget::TextSelection(selection) => selection.focus.block_id,
    }
}

pub fn project_ai_session_request(
    runtime: &mut DocumentRuntime,
    request: AiSessionRequest,
    readonly: bool,
) -> Result<AiSessionOutcome, ProtocolError> {
    if readonly
        && matches!(
            &request,
            AiSessionRequest::Begin { .. } | AiSessionRequest::Stream(_)
        )
    {
        return Err(
            ProtocolError::new(ProtocolErrorCode::Readonly, "document is read-only")
                .with_document(runtime.document_id()),
        );
    }

    runtime
        .apply_ai_session_request(request)
        .map_err(|message| {
            ProtocolError::new(ProtocolErrorCode::ApplyFailed, message)
                .with_document(runtime.document_id())
        })
}

impl EditorSessionHandle {
    pub fn ai_context(&self) -> Result<AiContextSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_ai_context(&session.runtime))
    }

    pub fn request_ai_session(
        &self,
        request: AiSessionRequest,
    ) -> Result<AiSessionOutcome, ProtocolError> {
        let mut session = self.try_session_mut()?;
        let readonly = session.readonly;
        project_ai_session_request(&mut session.runtime, request, readonly)
    }
}

#[cfg(test)]
mod tests {
    use cditor_ai::AiStreamEvent;
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
    use cditor_runtime::{AiRequestPresentation, AiStreamApplyResult};

    use super::*;
    use crate::EditorSession;

    fn focused_runtime() -> DocumentRuntime {
        let mut runtime = DocumentRuntime::empty();
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Sdk,
            ))
            .unwrap();
        runtime
    }

    fn begin(handle: &EditorSessionHandle) -> u64 {
        let outcome = handle
            .request_ai_session(AiSessionRequest::Begin {
                instruction: "continue".to_owned(),
                presentation: AiRequestPresentation::Automatic,
            })
            .unwrap();
        let AiSessionOutcome::Started(dispatch) = outcome else {
            panic!("AI begin must return provider dispatch");
        };
        dispatch.request.request_id
    }

    #[test]
    fn matching_and_stale_stream_events_preserve_runtime_request_identity() {
        let handle = EditorSession::new(focused_runtime(), false).into_handle();
        let request_id = begin(&handle);

        let stale = handle
            .request_ai_session(AiSessionRequest::Stream(AiStreamEvent::Delta {
                request_id: request_id + 1,
                text: "stale".to_owned(),
            }))
            .unwrap();
        assert!(matches!(
            stale,
            AiSessionOutcome::StreamApplied(AiStreamApplyResult::IgnoredRequest)
        ));

        let matching = handle
            .request_ai_session(AiSessionRequest::Stream(AiStreamEvent::Delta {
                request_id,
                text: "next".to_owned(),
            }))
            .unwrap();
        assert!(matches!(
            matching,
            AiSessionOutcome::StreamApplied(AiStreamApplyResult::Applied)
        ));
    }

    #[test]
    fn cancel_and_reject_clear_transient_ai_state() {
        let handle = EditorSession::new(focused_runtime(), false).into_handle();
        begin(&handle);
        assert!(matches!(
            handle.request_ai_session(AiSessionRequest::Cancel).unwrap(),
            AiSessionOutcome::Cancelled(true)
        ));

        begin(&handle);
        assert!(matches!(
            handle
                .request_ai_session(AiSessionRequest::RejectPreview)
                .unwrap(),
            AiSessionOutcome::PreviewRejected(true)
        ));
    }

    #[test]
    fn readonly_rejects_begin_and_stream_but_allows_cleanup() {
        let readonly = EditorSession::new(focused_runtime(), true).into_handle();
        let error = readonly
            .request_ai_session(AiSessionRequest::Begin {
                instruction: "continue".to_owned(),
                presentation: AiRequestPresentation::Automatic,
            })
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::Readonly);

        let mut runtime = focused_runtime();
        let started = runtime
            .apply_ai_session_request(AiSessionRequest::Begin {
                instruction: "continue".to_owned(),
                presentation: AiRequestPresentation::Automatic,
            })
            .unwrap();
        let AiSessionOutcome::Started(dispatch) = started else {
            panic!("AI begin must return provider dispatch");
        };
        let readonly = EditorSession::new(runtime, true).into_handle();
        let error = readonly
            .request_ai_session(AiSessionRequest::Stream(AiStreamEvent::Done {
                request_id: dispatch.request.request_id,
            }))
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::Readonly);
        assert!(matches!(
            readonly
                .request_ai_session(AiSessionRequest::Cancel)
                .unwrap(),
            AiSessionOutcome::Cancelled(true)
        ));

        let mut runtime = focused_runtime();
        runtime
            .apply_ai_session_request(AiSessionRequest::Begin {
                instruction: "continue".to_owned(),
                presentation: AiRequestPresentation::Automatic,
            })
            .unwrap();
        let readonly = EditorSession::new(runtime, true).into_handle();
        assert!(matches!(
            readonly
                .request_ai_session(AiSessionRequest::RejectPreview)
                .unwrap(),
            AiSessionOutcome::PreviewRejected(true)
        ));
    }

    #[test]
    fn ai_context_projects_prompt_target_and_apply_mode_without_runtime_borrows() {
        let handle = EditorSession::new(focused_runtime(), false).into_handle();
        let before = handle.ai_context().unwrap();
        assert!(!before.session_active);
        assert_eq!(before.focused_empty_text_block, Some((1, 0)));
        assert_eq!(before.prompt_block_id, Some(1));
        assert_eq!(before.apply_mode, None);

        begin(&handle);
        let active = handle.ai_context().unwrap();
        assert!(active.session_active);
        assert_eq!(active.prompt_block_id, Some(1));
        assert_eq!(active.apply_mode, Some(AiApplyMode::InsertAfter));
    }
}
