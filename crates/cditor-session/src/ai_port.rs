use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::{AiSessionOutcome, AiSessionRequest, DocumentRuntime};

use crate::EditorSessionHandle;

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
    pub fn apply_ai_session_request(
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
            .apply_ai_session_request(AiSessionRequest::Begin {
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
            .apply_ai_session_request(AiSessionRequest::Stream(AiStreamEvent::Delta {
                request_id: request_id + 1,
                text: "stale".to_owned(),
            }))
            .unwrap();
        assert!(matches!(
            stale,
            AiSessionOutcome::StreamApplied(AiStreamApplyResult::IgnoredRequest)
        ));

        let matching = handle
            .apply_ai_session_request(AiSessionRequest::Stream(AiStreamEvent::Delta {
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
            handle
                .apply_ai_session_request(AiSessionRequest::Cancel)
                .unwrap(),
            AiSessionOutcome::Cancelled(true)
        ));

        begin(&handle);
        assert!(matches!(
            handle
                .apply_ai_session_request(AiSessionRequest::RejectPreview)
                .unwrap(),
            AiSessionOutcome::PreviewRejected(true)
        ));
    }

    #[test]
    fn readonly_rejects_begin_and_stream_but_allows_cleanup() {
        let readonly = EditorSession::new(focused_runtime(), true).into_handle();
        let error = readonly
            .apply_ai_session_request(AiSessionRequest::Begin {
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
            .apply_ai_session_request(AiSessionRequest::Stream(AiStreamEvent::Done {
                request_id: dispatch.request.request_id,
            }))
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::Readonly);
        assert!(matches!(
            readonly
                .apply_ai_session_request(AiSessionRequest::Cancel)
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
                .apply_ai_session_request(AiSessionRequest::RejectPreview)
                .unwrap(),
            AiSessionOutcome::PreviewRejected(true)
        ));
    }
}
