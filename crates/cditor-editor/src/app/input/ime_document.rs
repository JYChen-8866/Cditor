use std::ops::Range;

use gpui::Context;

use crate::app::cditor_v2_view::CditorV2View;
use crate::app::input_trace::trace_input;
use crate::platform::{is_single_line_break_commit, normalize_external_line_endings};

use super::ime_support::{
    apply_platform_text_replacement, ime_replacement_range, is_empty_line_ai_platform_input,
    platform_input_fallback_range, platform_input_target_allows,
};

impl CditorV2View {
    pub(super) fn apply_document_realtime_replacement(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        let registered_target = self.platform_input_target;
        let empty_line_ai_input = is_empty_line_ai_platform_input(range_utf16.as_ref(), text)
            && self.ready_runtime_ref().is_some_and(|runtime| {
                platform_input_target_allows(
                    registered_target,
                    self.platform_input_session_identity,
                    runtime,
                ) && runtime.focused_empty_text_block_for_ai().is_some()
            });
        if empty_line_ai_input && self.invoke_empty_line_ai_from_gui(cx) {
            cx.notify();
            return;
        }
        if is_single_line_break_commit(text) {
            trace_input(
                "replace_text_in_range.native_enter_fallback",
                format_args!("registered={registered_target:?}"),
            );
            self.apply_input_command(crate::input::GuiInputCommand::HandleEnter, cx);
            self.sync_slash_menu_from_runtime(cx);
            cx.notify();
            return;
        }
        let registered_identity = self.platform_input_session_identity;
        let Some(runtime) = self.ready_runtime() else {
            return;
        };
        if !platform_input_target_allows(registered_target, registered_identity, runtime) {
            trace_input(
                "replace_text_in_range.rejected_target",
                format_args!(
                    "registered={registered_target:?} runtime={:?}",
                    runtime.input_session_target()
                ),
            );
            return;
        }
        let focused = runtime.focused_block_id();
        let range = ime_replacement_range(runtime, range_utf16.clone());
        trace_input(
            "replace_text_in_range",
            format_args!(
                "focused={focused:?} range_utf16={range_utf16:?} resolved_utf8={range:?} text_len={}",
                text.len()
            ),
        );
        let text = normalize_external_line_endings(text);
        match apply_platform_text_replacement(
            runtime,
            registered_identity.expect("validated platform input identity"),
            range,
            text.as_ref(),
        ) {
            Ok(outcome) if outcome.document_changed => {
                self.platform_input_session_identity = outcome.input_identity;
                self.mark_dirty_at_revision(
                    cditor_core::edit::ChangeOrigin::User,
                    outcome.revision,
                    cx,
                );
                self.sync_slash_menu_from_runtime(cx);
                cx.notify();
            }
            Ok(outcome) => {
                self.platform_input_session_identity = outcome.input_identity;
                if outcome.state_changed {
                    cx.notify();
                }
            }
            Err(error) => {
                self.save_status = crate::persistence::EditorSaveStatus::Failed(error.to_string());
                cx.notify();
            }
        }
    }

    pub(super) fn apply_document_realtime_composition(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        let registered_target = self.platform_input_target;
        let registered_identity = self.platform_input_session_identity;
        let Some(runtime) = self.ready_runtime() else {
            return;
        };
        if !platform_input_target_allows(registered_target, registered_identity, runtime) {
            trace_input(
                "replace_and_mark_text_in_range.rejected_target",
                format_args!(
                    "registered={registered_target:?} runtime={:?}",
                    runtime.input_session_target()
                ),
            );
            return;
        }
        let Some(block_id) = runtime.focused_block_id() else {
            return;
        };
        let range_from_ime = ime_replacement_range(runtime, range_utf16.clone());
        let range = range_from_ime
            .clone()
            .unwrap_or_else(|| platform_input_fallback_range(runtime, block_id));
        let selected_range = new_selected_range
            .clone()
            .map(|range| crate::input::ime::utf16_range_to_utf8_range(new_text, &range));
        trace_input(
            "replace_and_mark_text_in_range",
            format_args!(
                "block={block_id} range_utf16={range_utf16:?} range_from_ime={range_from_ime:?} resolved_utf8={range:?} new_text_len={} new_selected_utf16={new_selected_range:?} selected_utf8={selected_range:?}",
                new_text.len()
            ),
        );
        let result = cditor_session::project_realtime_input(
            runtime,
            cditor_runtime::RealtimeInputRequest {
                expected: registered_identity.expect("validated platform input identity"),
                input: cditor_runtime::RealtimeInput::UpdateComposition {
                    range,
                    text: new_text,
                    selected_range,
                },
            },
            false,
        );
        match result {
            Ok(outcome) => {
                self.platform_input_session_identity = outcome.input_identity;
                self.sync_slash_menu_from_runtime(cx);
                cx.notify();
            }
            Err(error) => {
                self.save_status = crate::persistence::EditorSaveStatus::Failed(error.to_string());
                cx.notify();
            }
        }
    }
}
