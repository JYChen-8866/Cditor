use std::sync::Arc;

use cditor_ai::{
    AiProvider, AiProviderError, AiStreamEvent, MockAiProvider, OpenAiCompatibleProvider,
    bounded_ai_stream,
};
use cditor_runtime::{
    AiApplyMode, AiRequestPresentation, AiSessionOutcome, AiSessionRequest, AiStreamApplyResult,
};
use cditor_session::SessionTaskKind;
use gpui::{AppContext, Context, px};

use crate::editor_view::{CditorV2View, GuiPlatformInputTarget};
use crate::input::{AiPromptEditAction, AiPromptKeyResult, AiPromptState, apply_ai_prompt_action};
use crate::persistence::EditorSaveStatus;
use cditor_editor_protocol::command::{
    AiApplyCommandMode, CditorCommand, CommandOutcomeStatus, CommandSource,
};

pub(crate) fn default_ai_provider() -> Arc<dyn AiProvider> {
    OpenAiCompatibleProvider::from_env()
        .map(|provider| Arc::new(provider) as Arc<dyn AiProvider>)
        .unwrap_or_else(|_| Arc::new(MockAiProvider::default()))
}

impl CditorV2View {
    pub(crate) fn invoke_empty_line_ai_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.features.ai_enabled
            || self.status.readonly
            || self.overlay.ai_prompt.is_some()
            || self.overlay.slash_menu.is_some()
            || self.overlay.code_language_edit.is_some()
            || self.ready_session().is_some_and(|session| {
                session
                    .ai_context()
                    .is_ok_and(|context| context.session_active)
            })
        {
            return false;
        }
        let Some((block_id, caret)) = self
            .ready_session()
            .and_then(|session| session.ai_context().ok()?.focused_empty_text_block)
        else {
            return false;
        };
        let (x, y) = self.ai_prompt_line_anchor(block_id, caret);
        self.open_ai_prompt_from_gui_with_presentation(
            x,
            y,
            AiRequestPresentation::AssistantPanel,
            cx,
        )
    }

    pub(crate) fn open_ai_prompt_from_gui(
        &mut self,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        let presentation = if self.overlay.gutter_toolbar_block_id.is_some() {
            AiRequestPresentation::AssistantPanel
        } else {
            AiRequestPresentation::Automatic
        };
        self.open_ai_prompt_from_gui_with_presentation(x, y, presentation, cx)
    }

    pub(crate) fn open_ai_prompt_from_gui_with_presentation(
        &mut self,
        x: f32,
        y: f32,
        presentation: AiRequestPresentation,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.features.ai_enabled || self.status.readonly {
            return false;
        }
        let Some(block_id) = self
            .ready_session()
            .and_then(|session| session.ai_context().ok()?.prompt_block_id)
        else {
            return false;
        };
        if !self.commit_document_composition_before_external_focus(cx) {
            return false;
        }
        if let Some(session) = self.ready_session() {
            let _ = session.request_ai_session(AiSessionRequest::Cancel);
        }
        self.overlay.slash_menu = None;
        self.overlay.code_language_edit = None;
        self.overlay.ai_prompt = Some(AiPromptState::with_presentation(
            block_id,
            px(x),
            px(y),
            presentation,
        ));
        self.input.target = Some(GuiPlatformInputTarget::ai_prompt(block_id));
        cx.notify();
        true
    }

    pub(crate) fn submit_ai_prompt_instruction_from_gui(
        &mut self,
        instruction: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.features.ai_enabled {
            return false;
        }
        let instruction = instruction.into();
        let presentation = self
            .overlay
            .ai_prompt
            .as_ref()
            .map(|prompt| prompt.presentation)
            .unwrap_or_else(|| {
                if self.overlay.gutter_toolbar_block_id.is_some() {
                    AiRequestPresentation::AssistantPanel
                } else {
                    AiRequestPresentation::Automatic
                }
            });
        let dispatch = match self
            .ready_session()
            .ok_or_else(|| "runtime is not ready".to_owned())
            .and_then(|session| {
                session
                    .request_ai_session(AiSessionRequest::Begin {
                        instruction,
                        presentation,
                    })
                    .map_err(|error| error.to_string())
            })
            .and_then(|outcome| match outcome {
                AiSessionOutcome::Started(dispatch) => Ok(dispatch),
                _ => Err("AI begin returned an unexpected session outcome".to_owned()),
            }) {
            Ok(dispatch) => dispatch,
            Err(error) => {
                self.status.save_status = EditorSaveStatus::Failed(error);
                cx.notify();
                return false;
            }
        };
        self.overlay.ai_prompt = None;
        self.input.target = None;

        let provider = self.features.ai_provider.clone();
        let request_id = dispatch.request.request_id;
        let stream_token = match self.ready_session().and_then(|session| {
            session
                .replace_session_task(SessionTaskKind::AiStream, request_id)
                .ok()
        }) {
            Some(token) => token,
            None => {
                self.status.save_status =
                    EditorSaveStatus::Failed("AI stream task could not be registered".to_owned());
                cx.notify();
                return false;
            }
        };
        let cancellation = dispatch.cancellation.clone();
        let timeout_cancellation = cancellation.clone();
        let (sender, receiver) = bounded_ai_stream(cditor_ai::DEFAULT_AI_STREAM_CAPACITY);
        let error_sender = sender.clone();
        let timeout_sender = sender.clone();
        cx.background_spawn(async move {
            if let Err(error) = provider.stream(dispatch.request, sender, cancellation)
                && !matches!(
                    error,
                    AiProviderError::Cancelled | AiProviderError::ChannelClosed
                )
            {
                let _ = error_sender.send_blocking(AiStreamEvent::Error {
                    request_id,
                    message: error.to_string(),
                });
            }
        })
        .detach();

        let timeout = cx.background_executor().timer(stream_token.timeout());
        cx.spawn(async move |view, cx| {
            timeout.await;
            let should_cancel = view
                .update(cx, |view, _cx| {
                    view.ready_session().is_some_and(|session| {
                        session.complete_session_task(stream_token).unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if should_cancel {
                timeout_cancellation.cancel();
                let _ = timeout_sender.send_blocking(AiStreamEvent::Error {
                    request_id,
                    message: format!("AI stream timed out after {:?}", stream_token.timeout()),
                });
            }
        })
        .detach();

        cx.spawn(async move |view, cx| {
            while let Ok(event) = receiver.recv().await {
                let terminal = matches!(
                    event,
                    AiStreamEvent::Done { .. } | AiStreamEvent::Error { .. }
                );
                let result = view.update(cx, |view, cx| {
                    let result = view
                        .ready_session()
                        .and_then(|session| {
                            session
                                .request_ai_session(AiSessionRequest::Stream(event))
                                .ok()
                        })
                        .and_then(|outcome| match outcome {
                            AiSessionOutcome::StreamApplied(result) => Some(result),
                            _ => None,
                        })
                        .unwrap_or(AiStreamApplyResult::IgnoredRequest);
                    cx.notify();
                    result
                });
                if terminal || !matches!(result, Ok(AiStreamApplyResult::Applied)) {
                    break;
                }
            }
            let _ = view.update(cx, |view, _cx| {
                if let Some(session) = view.ready_session() {
                    let _ = session.complete_session_task(stream_token);
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(crate) fn submit_ai_prompt_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(prompt) = self.overlay.ai_prompt.as_ref() else {
            return false;
        };
        let instruction = prompt.draft.trim().to_owned();
        if instruction.is_empty() {
            return false;
        }
        self.submit_ai_prompt_instruction_from_gui(instruction, cx)
    }

    pub(crate) fn apply_ai_prompt_action_from_gui(
        &mut self,
        action: AiPromptEditAction,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(prompt) = self.overlay.ai_prompt.as_mut() else {
            return false;
        };
        match apply_ai_prompt_action(prompt, action) {
            AiPromptKeyResult::Submit => self.submit_ai_prompt_from_gui(cx),
            AiPromptKeyResult::Cancel => self.cancel_ai_prompt(cx),
            AiPromptKeyResult::Changed => {
                cx.notify();
                true
            }
        }
    }

    pub(crate) fn cancel_ai_prompt(&mut self, cx: &mut Context<Self>) -> bool {
        let had_prompt = self.overlay.ai_prompt.take().is_some();
        if had_prompt {
            self.input.target = None;
            cx.notify();
        }
        had_prompt
    }

    pub(crate) fn accept_ai_preview_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let mode = self
            .ready_session()
            .and_then(|session| session.ai_context().ok()?.apply_mode);
        let Some(mode) = mode else {
            return false;
        };
        self.apply_ai_preview_from_gui(mode, cx)
    }

    pub(crate) fn apply_ai_preview_from_gui(
        &mut self,
        mode: AiApplyMode,
        cx: &mut Context<Self>,
    ) -> bool {
        let mode = match mode {
            AiApplyMode::Replace => AiApplyCommandMode::Replace,
            AiApplyMode::InsertAfter => AiApplyCommandMode::InsertAfter,
        };
        let result = self.dispatch_command(
            CditorCommand::ApplyAiPreview { mode },
            CommandSource::Ai,
            cx,
        );
        match result {
            Ok(outcome) => outcome.status == CommandOutcomeStatus::Applied,
            Err(error) => {
                self.status.save_status = EditorSaveStatus::Failed(error.to_string());
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn reject_ai_preview_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self
            .ready_session()
            .and_then(|session| {
                session
                    .request_ai_session(AiSessionRequest::RejectPreview)
                    .ok()
            })
            .is_some_and(|outcome| matches!(outcome, AiSessionOutcome::PreviewRejected(true)));
        if changed {
            cx.notify();
        }
        changed
    }
}
