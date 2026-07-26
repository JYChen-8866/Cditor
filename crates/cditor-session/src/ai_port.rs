use cditor_ai::AiProviderRequest;
use cditor_core::ids::BlockId;
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::{
    AiApplyMode, AiSessionOutcome, AiSessionRequest, DocumentRuntime, RuntimeAiTarget,
};

use crate::{EditorSessionHandle, SessionTaskKind};

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

    let mut outcome = runtime
        .apply_ai_session_request(request)
        .map_err(|message| {
            ProtocolError::new(ProtocolErrorCode::ApplyFailed, message)
                .with_document(runtime.document_id())
        })?;
    if let AiSessionOutcome::Started(dispatch) = &mut outcome {
        dispatch.redacted_fields = redact_ai_provider_request(&mut dispatch.request);
    }
    Ok(outcome)
}

fn redact_ai_provider_request(request: &mut AiProviderRequest) -> usize {
    let mut redacted = 0;
    for value in [
        &mut request.instruction,
        &mut request.selected_text,
        &mut request.prefix,
        &mut request.suffix,
    ] {
        let (next, count) = redact_sensitive_text(value);
        *value = next;
        redacted += count;
    }
    redacted
}

fn redact_sensitive_text(text: &str) -> (String, usize) {
    const MARKERS: [&str; 6] = [
        "authorization:",
        "bearer ",
        "api_key=",
        "api-key=",
        "password=",
        "secret=",
    ];
    let mut count = 0;
    let lines = text
        .split_inclusive('\n')
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            let Some((start, marker_len)) = MARKERS
                .iter()
                .filter_map(|marker| lower.find(marker).map(|start| (start, marker.len())))
                .min_by_key(|(start, _)| *start)
            else {
                return line.to_owned();
            };
            count += 1;
            let end = start + marker_len;
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            format!("{}[REDACTED]{newline}", &line[..end])
        })
        .collect();
    (lines, count)
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
        let cancels_stream = matches!(
            &request,
            AiSessionRequest::Cancel | AiSessionRequest::RejectPreview
        );
        let readonly = session.readonly;
        let outcome = project_ai_session_request(&mut session.runtime, request, readonly)?;
        if cancels_stream {
            session.tasks.cancel_kind(SessionTaskKind::AiStream);
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use cditor_ai::AiStreamEvent;
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
    use cditor_runtime::{
        AiRequestPresentation, AiStreamApplyResult, RealtimeInput, RealtimeInputRequest,
    };

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
        let crate::SessionTaskAdmission::Started(stream_task) = handle
            .begin_session_task(SessionTaskKind::AiStream, 1)
            .unwrap()
        else {
            panic!("AI stream task must start")
        };
        assert!(matches!(
            handle.request_ai_session(AiSessionRequest::Cancel).unwrap(),
            AiSessionOutcome::Cancelled(true)
        ));
        assert!(!handle.complete_session_task(stream_task).unwrap());

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

    #[test]
    fn provider_dispatch_redacts_sensitive_document_context_before_leaving_session() {
        let mut runtime = DocumentRuntime::empty();
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::FocusBlock { block_id: 1 },
                CommandSource::Sdk,
            ))
            .unwrap();
        let identity = runtime.input_session_identity().unwrap();
        runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected: identity,
                input: RealtimeInput::ReplaceText {
                    range: None,
                    text: "authorization: Bearer document-token",
                },
            })
            .unwrap();
        let handle = EditorSession::new(runtime, false).into_handle();
        let outcome = handle
            .request_ai_session(AiSessionRequest::Begin {
                instruction: "password=prompt-secret\nrewrite".to_owned(),
                presentation: AiRequestPresentation::Automatic,
            })
            .unwrap();
        let AiSessionOutcome::Started(dispatch) = outcome else {
            panic!("AI begin must return provider dispatch");
        };

        assert!(dispatch.redacted_fields >= 2);
        let provider_text = format!(
            "{}{}{}{}",
            dispatch.request.instruction,
            dispatch.request.selected_text,
            dispatch.request.prefix,
            dispatch.request.suffix
        );
        assert!(!provider_text.contains("prompt-secret"));
        assert!(!provider_text.contains("document-token"));
        assert!(provider_text.contains("[REDACTED]"));
    }
}
