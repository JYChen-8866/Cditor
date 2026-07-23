use cditor_ai::AiStreamEvent;

use super::{AiRequestDispatch, AiRequestPresentation, AiStreamApplyResult, DocumentRuntime};

#[derive(Debug, Clone)]
pub enum AiSessionRequest {
    Begin {
        instruction: String,
        presentation: AiRequestPresentation,
    },
    Stream(AiStreamEvent),
    Cancel,
    RejectPreview,
}

#[derive(Debug, Clone)]
pub enum AiSessionOutcome {
    Started(AiRequestDispatch),
    StreamApplied(AiStreamApplyResult),
    Cancelled(bool),
    PreviewRejected(bool),
}

impl DocumentRuntime {
    /// Version-checked ingress for the asynchronous AI session lifecycle.
    /// Applying accepted preview content remains a document command.
    pub fn apply_ai_session_request(
        &mut self,
        request: AiSessionRequest,
    ) -> Result<AiSessionOutcome, String> {
        Ok(match request {
            AiSessionRequest::Begin {
                instruction,
                presentation,
            } => AiSessionOutcome::Started(
                self.begin_ai_request_with_presentation(instruction, presentation)?,
            ),
            AiSessionRequest::Stream(event) => {
                AiSessionOutcome::StreamApplied(self.apply_ai_stream_event(event))
            }
            AiSessionRequest::Cancel => AiSessionOutcome::Cancelled(self.cancel_ai_request()),
            AiSessionRequest::RejectPreview => {
                AiSessionOutcome::PreviewRejected(self.reject_ai_preview())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use cditor_ai::AiStreamEvent;

    use super::*;

    #[test]
    fn ai_session_port_starts_streams_and_cancels_by_request_identity() {
        let mut runtime = DocumentRuntime::empty();
        runtime.focus_block_at_offset(1, 0).unwrap();
        let started = runtime
            .apply_ai_session_request(AiSessionRequest::Begin {
                instruction: "continue".to_owned(),
                presentation: AiRequestPresentation::Automatic,
            })
            .unwrap();
        let AiSessionOutcome::Started(dispatch) = started else {
            panic!("AI begin must return provider dispatch");
        };

        assert!(matches!(
            runtime
                .apply_ai_session_request(AiSessionRequest::Stream(AiStreamEvent::Delta {
                    request_id: dispatch.request.request_id,
                    text: "next".to_owned(),
                }))
                .unwrap(),
            AiSessionOutcome::StreamApplied(AiStreamApplyResult::Applied)
        ));
        assert!(matches!(
            runtime
                .apply_ai_session_request(AiSessionRequest::Cancel)
                .unwrap(),
            AiSessionOutcome::Cancelled(true)
        ));
    }
}
