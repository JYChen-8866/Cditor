use std::sync::Arc;

use cditor_ai::{
    AiProvider, AiProviderError, AiStreamEvent, MockAiProvider, OpenAiCompatibleProvider,
    bounded_ai_stream,
};
use cditor_runtime::{AiApplyMode, AiRequestPresentation, AiStreamApplyResult, RuntimeAiTarget};
use gpui::{AppContext, Context, px};

use crate::gui::app::cditor_v2_view::{CditorV2View, GuiPlatformInputTarget};
use crate::gui::input::{
    AiPromptEditAction, AiPromptKeyResult, AiPromptState, apply_ai_prompt_action,
};
use crate::gui::persistence::EditorSaveStatus;

pub(in crate::gui::app) fn default_ai_provider() -> Arc<dyn AiProvider> {
    OpenAiCompatibleProvider::from_env()
        .map(|provider| Arc::new(provider) as Arc<dyn AiProvider>)
        .unwrap_or_else(|_| Arc::new(MockAiProvider::default()))
}

impl CditorV2View {
    pub(crate) fn invoke_empty_line_ai_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if self.readonly
            || self.ai_prompt.is_some()
            || self.slash_menu.is_some()
            || self.code_language_edit.is_some()
            || self
                .ready_runtime_ref()
                .is_some_and(|runtime| runtime.ai_session_snapshot().is_some())
        {
            return false;
        }
        let Some((block_id, caret)) = self
            .ready_runtime_ref()
            .and_then(|runtime| runtime.focused_empty_text_block_for_ai())
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
        let presentation = if self.gutter_toolbar_block_id.is_some() {
            AiRequestPresentation::AssistantPanel
        } else {
            AiRequestPresentation::Automatic
        };
        self.open_ai_prompt_from_gui_with_presentation(x, y, presentation, cx)
    }

    pub(in crate::gui::app) fn open_ai_prompt_from_gui_with_presentation(
        &mut self,
        x: f32,
        y: f32,
        presentation: AiRequestPresentation,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let Some(block_id) = self.ready_runtime_ref().and_then(|runtime| {
            runtime
                .ai_session_snapshot()
                .and_then(|session| match session.target {
                    RuntimeAiTarget::InlineCaret(position) => Some(position.block_id),
                    RuntimeAiTarget::TextSelection(selection) => Some(selection.focus.block_id),
                })
                .or_else(|| runtime.focused_block_id())
        }) else {
            return false;
        };
        if let Some(runtime) = self.ready_runtime() {
            runtime.cancel_ai_request();
        }
        self.slash_menu = None;
        self.code_language_edit = None;
        self.ai_prompt = Some(AiPromptState::with_presentation(
            block_id,
            px(x),
            px(y),
            presentation,
        ));
        self.platform_input_target = Some(GuiPlatformInputTarget::ai_prompt(block_id));
        cx.notify();
        true
    }

    pub(crate) fn submit_ai_prompt_instruction_from_gui(
        &mut self,
        instruction: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let instruction = instruction.into();
        let presentation = self
            .ai_prompt
            .as_ref()
            .map(|prompt| prompt.presentation)
            .unwrap_or_else(|| {
                if self.gutter_toolbar_block_id.is_some() {
                    AiRequestPresentation::AssistantPanel
                } else {
                    AiRequestPresentation::Automatic
                }
            });
        let dispatch = match self
            .ready_runtime()
            .ok_or_else(|| "runtime is not ready".to_owned())
            .and_then(|runtime| {
                runtime.begin_ai_request_with_presentation(instruction, presentation)
            }) {
            Ok(dispatch) => dispatch,
            Err(error) => {
                self.save_status = EditorSaveStatus::Failed(error);
                cx.notify();
                return false;
            }
        };
        self.ai_prompt = None;
        self.platform_input_target = None;

        let provider = self.ai_provider.clone();
        let request_id = dispatch.request.request_id;
        let cancellation = dispatch.cancellation.clone();
        let (sender, receiver) = bounded_ai_stream(cditor_ai::DEFAULT_AI_STREAM_CAPACITY);
        let error_sender = sender.clone();
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

        cx.spawn(async move |view, cx| {
            while let Ok(event) = receiver.recv().await {
                let terminal = matches!(
                    event,
                    AiStreamEvent::Done { .. } | AiStreamEvent::Error { .. }
                );
                let result = view.update(cx, |view, cx| {
                    let result = view
                        .ready_runtime()
                        .map(|runtime| runtime.apply_ai_stream_event(event))
                        .unwrap_or(AiStreamApplyResult::IgnoredRequest);
                    cx.notify();
                    result
                });
                if terminal || !matches!(result, Ok(AiStreamApplyResult::Applied)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
        true
    }

    pub(crate) fn submit_ai_prompt_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(prompt) = self.ai_prompt.as_ref() else {
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
        let Some(prompt) = self.ai_prompt.as_mut() else {
            return false;
        };
        match apply_ai_prompt_action(prompt, action) {
            AiPromptKeyResult::Submit => self.submit_ai_prompt_from_gui(cx),
            AiPromptKeyResult::Cancel => self.cancel_ai_prompt(cx),
            AiPromptKeyResult::Changed => {
                cx.notify();
                true
            }
            AiPromptKeyResult::Ignored => false,
        }
    }

    pub(crate) fn cancel_ai_prompt(&mut self, cx: &mut Context<Self>) -> bool {
        let had_prompt = self.ai_prompt.take().is_some();
        if had_prompt {
            self.platform_input_target = None;
            cx.notify();
        }
        had_prompt
    }

    pub(crate) fn accept_ai_preview_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let mode = self
            .ready_runtime_ref()
            .and_then(|runtime| runtime.ai_session_snapshot())
            .map(|session| match session.target {
                RuntimeAiTarget::InlineCaret(_) => AiApplyMode::InsertAfter,
                RuntimeAiTarget::TextSelection(_) => AiApplyMode::Replace,
            });
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
        let result = self
            .ready_runtime()
            .ok_or_else(|| "runtime is not ready".to_owned())
            .and_then(|runtime| runtime.apply_ai_preview(mode));
        match result {
            Ok(true) => {
                self.mark_dirty(cx);
                cx.notify();
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.save_status = EditorSaveStatus::Failed(error);
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn reject_ai_preview_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self
            .ready_runtime()
            .is_some_and(|runtime| runtime.reject_ai_preview());
        if changed {
            cx.notify();
        }
        changed
    }
}
