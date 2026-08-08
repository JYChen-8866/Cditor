use std::ops::Range;

#[cfg(feature = "mobile-text-session")]
use unicode_segmentation::UnicodeSegmentation;

#[cfg(feature = "mobile-text-session")]
use gpui::SelectionMenuPresentation;
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, px};

use crate::editor_view::{CditorV2View, CditorViewState};
use crate::features::table::menu::TABLE_MENU_SEARCH_FONT_SIZE_PX;
use crate::input::ime::{
    utf8_range_to_utf16_range, utf8_to_utf16_offset, utf16_range_to_utf8_range,
};
use crate::input::trace::trace_input;
use crate::input::{SINGLE_LINE_INPUT_FONT_SIZE_PX, single_line_text_offset_for_x};
use crate::platform::{is_single_line_break_commit, normalize_external_line_endings};
use crate::text::record_unavailable_geometry;
#[cfg(feature = "mobile-text-session")]
use cditor_core::edit::{DocumentSelection, TextAffinity, TextPosition};
#[cfg(feature = "mobile-text-session")]
use cditor_editor_protocol::command::{CditorCommand, CommandEnvelope, CommandSource};
use cditor_runtime::InputTarget;

use super::support::{
    ai_prompt_input_target_allows, apply_platform_unmark, platform_input_geometry_allows,
    table_menu_input_target_allows,
};
pub(crate) use super::support::{
    code_language_input_target_allows, platform_input_target_allows, platform_selected_text_range,
};

#[cfg(feature = "mobile-text-session")]
fn native_selection_collapse_position(
    has_cross_block_selection: bool,
    block_id: Option<cditor_core::ids::BlockId>,
    range: Option<Range<usize>>,
) -> Option<TextPosition> {
    if has_cross_block_selection {
        return None;
    }
    Some(TextPosition::downstream(block_id?, range?.end))
}

#[cfg(feature = "mobile-text-session")]
fn native_selection_target_supported(target: Option<InputTarget>) -> bool {
    matches!(
        target,
        Some(
            InputTarget::BlockText { .. }
                | InputTarget::TableCell { .. }
                | InputTarget::ImageCaption { .. }
                | InputTarget::CollectionTitle { .. }
        )
    )
}

#[cfg(feature = "mobile-text-session")]
fn word_range_utf16(text: &str, offset_utf16: usize) -> Range<usize> {
    let offset_utf8 = utf16_range_to_utf8_range(text, &(offset_utf16..offset_utf16)).start;
    let mut word_ending_at_offset = None;
    for (start, word) in text.unicode_word_indices() {
        let end = start + word.len();
        if start <= offset_utf8 && offset_utf8 < end {
            return utf8_range_to_utf16_range(text, &(start..end));
        }
        if end == offset_utf8 {
            word_ending_at_offset = Some((start, end));
        }
    }
    if let Some((start, end)) = word_ending_at_offset {
        return utf8_range_to_utf16_range(text, &(start..end));
    }
    let Some((start, grapheme)) = text.grapheme_indices(true).find(|(start, grapheme)| {
        let end = *start + grapheme.len();
        *start <= offset_utf8 && offset_utf8 < end
    }) else {
        return offset_utf16..offset_utf16;
    };
    if grapheme.chars().all(char::is_whitespace) {
        return offset_utf16..offset_utf16;
    }
    utf8_range_to_utf16_range(text, &(start..start + grapheme.len()))
}

impl EntityInputHandler for CditorV2View {
    #[cfg(feature = "mobile-text-session")]
    fn handles_native_selection(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> bool {
        if self.focus.ai_prompt.is_focused(window)
            || self.focus.code_language.is_focused(window)
            || self
                .interaction
                .table_interaction_mode
                .axis_selection()
                .is_some()
        {
            return false;
        }
        let registered_target = self.input.target;
        let registered_identity = self.input.session_identity;
        let Some(session) = self.ready_session() else {
            return false;
        };
        let Ok(context) = session.input_context() else {
            return false;
        };
        native_selection_target_supported(context.target)
            && context.focused_text.is_some()
            && platform_input_target_allows(registered_target, registered_identity, &context)
    }

    #[cfg(feature = "mobile-text-session")]
    fn native_selection_allowed_at(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let handles_selection = self.handles_native_selection(window, cx);
        let surface_reachable = self
            .input
            .hitbox_id
            .is_none_or(|id| window.point_reaches_hitbox(id, point));
        let character_index = (handles_selection && surface_reachable)
            .then(|| self.character_index_for_point(point, window, cx))
            .flatten();
        trace_input(
            "native_selection_allowed_at",
            format_args!(
                "point={point:?} handles_selection={handles_selection} surface_reachable={surface_reachable} character_index={character_index:?}"
            ),
        );
        character_index.is_some()
    }

    #[cfg(feature = "mobile-text-session")]
    fn selection_menu_presentation(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> SelectionMenuPresentation {
        SelectionMenuPresentation::SystemAndCustomActions
    }

    #[cfg(feature = "mobile-text-session")]
    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.handles_native_selection(window, cx) {
            return;
        }
        let registered_target = self.input.target;
        let Some(session) = self.ready_session() else {
            return;
        };
        let Ok(context) = session.input_context() else {
            return;
        };
        let Some(focused) = context.focused_text else {
            return;
        };
        let range = utf16_range_to_utf8_range(&focused.text, &range_utf16);
        trace_input(
            "set_selected_text_range",
            format_args!(
                "block={} utf16={range_utf16:?} utf8={range:?} text_len={}",
                focused.block_id,
                focused.text.len()
            ),
        );
        let command = match context.target {
            Some(InputTarget::BlockText { .. }) => CditorCommand::SetDocumentSelection {
                selection: DocumentSelection {
                    anchor: TextPosition::downstream(focused.block_id, range.start),
                    focus: TextPosition::downstream(focused.block_id, range.end),
                },
            },
            Some(target) if target.surface_id().is_some() => {
                CditorCommand::SetTextSurfaceSelection {
                    surface_id: target.surface_id().expect("checked above"),
                    anchor_offset: range.start,
                    focus_offset: range.end,
                    focus_affinity: TextAffinity::Downstream,
                }
            }
            _ => return,
        };
        if session
            .dispatch(CommandEnvelope::new(command, CommandSource::Ime))
            .is_ok()
        {
            if let Some(target) = registered_target {
                let candidate = self.input.native_selection_candidate_for(Some(target));
                self.input.set_native_selection(target, range_utf16, false);
                trace_input(
                    "native_selection_candidate.cleared",
                    format_args!(
                        "target={target:?} candidate_utf16={candidate:?} reason=selection_committed"
                    ),
                );
            }
            cx.notify();
        }
    }

    #[cfg(feature = "mobile-text-session")]
    fn adjusted_native_selection_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if !self.handles_native_selection(window, cx) || !range_utf16.is_empty() {
            return None;
        }
        let candidate = self
            .input
            .native_selection_candidate_for(self.input.target)?;
        let context = self.ready_session()?.input_context().ok()?;
        let text = context.focused_text?.text;
        let resolved = word_range_utf16(&text, candidate);
        trace_input(
            "native_selection_candidate.resolved",
            format_args!(
                "target={:?} candidate_utf16={candidate} expanded_utf16={resolved:?}",
                self.input.target
            ),
        );
        (!resolved.is_empty()).then_some(resolved)
    }

    #[cfg(feature = "mobile-text-session")]
    fn clear_selected_text_range(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.handles_native_selection(window, cx) {
            return;
        }
        let target = self.input.target;
        let candidate = self.input.native_selection_candidate_for(target);
        self.input.clear_native_selection();
        trace_input(
            "native_selection_candidate.cleared",
            format_args!(
                "target={target:?} candidate_utf16={candidate:?} reason=selection_dismissed"
            ),
        );
        let Some(session) = self.ready_session() else {
            return;
        };
        let Ok(context) = session.input_context() else {
            return;
        };
        // UIKit selection dismissal is a local surface operation. A
        // cross-block document selection remains owned by Cditor and is not
        // silently truncated here.
        let Some(focused) = context.focused_text.as_ref() else {
            return;
        };
        let Some(range) = context.focused_text_selection_range.clone() else {
            return;
        };
        let command = match context.target {
            Some(InputTarget::BlockText { .. }) => {
                let Some(position) = native_selection_collapse_position(
                    context.has_cross_block_text_selection,
                    Some(focused.block_id),
                    Some(range.clone()),
                ) else {
                    return;
                };
                CditorCommand::SetDocumentSelection {
                    selection: DocumentSelection::caret(position),
                }
            }
            Some(target) if target.surface_id().is_some() => {
                CditorCommand::SetTextSurfaceSelection {
                    surface_id: target.surface_id().expect("checked above"),
                    anchor_offset: range.end,
                    focus_offset: range.end,
                    focus_affinity: TextAffinity::Downstream,
                }
            }
            _ => return,
        };
        trace_input(
            "clear_selected_text_range",
            format_args!("block={} offset={}", focused.block_id, range.end),
        );
        if session
            .dispatch(CommandEnvelope::new(command, CommandSource::Ime))
            .is_ok()
        {
            cx.notify();
        }
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        if let Some(selection) = self.interaction.table_interaction_mode.axis_selection() {
            if !table_menu_input_target_allows(self.input.target, selection.block_id) {
                return None;
            }
            let range = utf16_range_to_utf8_range(&self.overlay.table_menu_ui.query, &range_utf16);
            let actual = utf8_range_to_utf16_range(&self.overlay.table_menu_ui.query, &range);
            actual_range.replace(actual);
            return self
                .overlay
                .table_menu_ui
                .query
                .get(range)
                .map(ToOwned::to_owned);
        }
        if self.focus.ai_prompt.is_focused(_window) {
            let registered_target = self.input.target;
            let prompt = self.overlay.ai_prompt.as_ref()?;
            if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                return None;
            }
            let range = utf16_range_to_utf8_range(&prompt.draft, &range_utf16);
            let actual = utf8_range_to_utf16_range(&prompt.draft, &range);
            actual_range.replace(actual);
            return prompt.draft.get(range).map(ToOwned::to_owned);
        }
        if self.focus.code_language.is_focused(_window) {
            let registered_target = self.input.target;
            let edit = self.overlay.code_language_edit.as_ref()?;
            if !code_language_input_target_allows(registered_target, edit.block_id) {
                trace_input(
                    "text_for_range.code_language_rejected_target",
                    format_args!("registered={:?} block={}", registered_target, edit.block_id),
                );
                return None;
            }
            let range = utf16_range_to_utf8_range(&edit.draft, &range_utf16);
            let actual = utf8_range_to_utf16_range(&edit.draft, &range);
            actual_range.replace(actual);
            return edit.draft.get(range).map(ToOwned::to_owned);
        }
        let registered_target = self.input.target;
        let registered_identity = self.input.session_identity;
        let context = self
            .ready_session()
            .and_then(|session| session.input_context().ok())?;
        if !platform_input_target_allows(registered_target, registered_identity, &context) {
            trace_input(
                "text_for_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    registered_target, context.target
                ),
            );
            return None;
        }
        let focused = context.focused_text?;
        let (block_id, text) = (focused.block_id, focused.text);
        let range = utf16_range_to_utf8_range(&text, &range_utf16);
        let actual = utf8_range_to_utf16_range(&text, &range);
        actual_range.replace(actual.clone());
        trace_input(
            "text_for_range",
            format_args!(
                "block={block_id} range_utf16={range_utf16:?} utf8_range={range:?} actual_utf16={actual:?} text_len={}",
                text.len()
            ),
        );
        text.get(range).map(ToOwned::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if let Some(selection) = self.interaction.table_interaction_mode.axis_selection() {
            if !table_menu_input_target_allows(self.input.target, selection.block_id) {
                return None;
            }
            let caret = utf8_to_utf16_offset(
                &self.overlay.table_menu_ui.query,
                self.overlay.table_menu_ui.caret_offset,
            );
            return Some(UTF16Selection {
                range: caret..caret,
                reversed: false,
            });
        }
        if self.focus.ai_prompt.is_focused(_window) {
            let registered_target = self.input.target;
            let prompt = self.overlay.ai_prompt.as_ref()?;
            if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                return None;
            }
            let caret = utf8_to_utf16_offset(&prompt.draft, prompt.caret_offset);
            return Some(UTF16Selection {
                range: caret..caret,
                reversed: false,
            });
        }
        if self.focus.code_language.is_focused(_window) {
            let registered_target = self.input.target;
            let edit = self.overlay.code_language_edit.as_ref()?;
            if !code_language_input_target_allows(registered_target, edit.block_id) {
                trace_input(
                    "selected_text_range.code_language_rejected_target",
                    format_args!("registered={:?} block={}", registered_target, edit.block_id),
                );
                return None;
            }
            let caret = utf8_to_utf16_offset(&edit.draft, edit.caret_offset);
            return Some(UTF16Selection {
                range: caret..caret,
                reversed: false,
            });
        }
        let registered_target = self.input.target;
        let registered_identity = self.input.session_identity;
        let context = self
            .ready_session()
            .and_then(|session| session.input_context().ok())?;
        if !platform_input_target_allows(registered_target, registered_identity, &context) {
            trace_input(
                "selected_text_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    registered_target, context.target
                ),
            );
            return None;
        }
        // UIKit first asks for the current range after a long-press hit test.
        // Keep the hit-test candidate as a collapsed range until UIKit calls
        // setSelectedTextRange; that callback is where the candidate is
        // expanded to a word and committed to the document model. Returning
        // the previous document caret here makes UIKit place the magnifier at
        // the wrong anchor and never promote the interaction to a selection.
        let candidate = self.input.native_selection_candidate_for(registered_target);
        let selection = self
            .input
            .native_selection_for(registered_target)
            .or_else(|| {
                candidate.map(|offset| UTF16Selection {
                    range: offset..offset,
                    reversed: false,
                })
            })
            .or_else(|| platform_selected_text_range(&context));
        trace_input(
            "selected_text_range",
            format_args!(
                "focused={:?} candidate={candidate:?} selection={selection:?}",
                context.focused_block_id,
            ),
        );
        selection
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if let Some(selection) = self.interaction.table_interaction_mode.axis_selection() {
            if !table_menu_input_target_allows(self.input.target, selection.block_id) {
                return None;
            }
            return self
                .overlay
                .table_menu_ui
                .marked_range
                .as_ref()
                .map(|range| utf8_range_to_utf16_range(&self.overlay.table_menu_ui.query, range));
        }
        if self.focus.ai_prompt.is_focused(_window) {
            let registered_target = self.input.target;
            let prompt = self.overlay.ai_prompt.as_ref()?;
            if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                return None;
            }
            return prompt
                .marked_range
                .as_ref()
                .map(|range| utf8_range_to_utf16_range(&prompt.draft, range));
        }
        if self.focus.code_language.is_focused(_window) {
            let registered_target = self.input.target;
            let edit = self.overlay.code_language_edit.as_ref()?;
            if !code_language_input_target_allows(registered_target, edit.block_id) {
                trace_input(
                    "marked_text_range.code_language_rejected_target",
                    format_args!("registered={:?} block={}", registered_target, edit.block_id),
                );
                return None;
            }
            return edit
                .marked_range
                .as_ref()
                .map(|range| utf8_range_to_utf16_range(&edit.draft, range));
        }
        let session = self.ready_session()?;
        let context = session.input_context().ok()?;
        if !platform_input_target_allows(self.input.target, self.input.session_identity, &context) {
            trace_input(
                "marked_text_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    self.input.target, context.target
                ),
            );
            return None;
        }
        let focused = context.focused_text?;
        let (block_id, text) = (focused.block_id, focused.text);
        let marked = context
            .marked_range
            .or_else(|| context.composition?.preview_marked_range)
            .map(|range| utf8_range_to_utf16_range(&text, &range));
        trace_input(
            "marked_text_range",
            format_args!(
                "block={block_id} marked_utf16={marked:?} text_len={}",
                text.len()
            ),
        );
        marked
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pause_caret_blink(cx);
        if let Some(selection) = self.interaction.table_interaction_mode.axis_selection() {
            if table_menu_input_target_allows(self.input.target, selection.block_id) {
                self.overlay.table_menu_ui.unmark();
                window.invalidate_character_coordinates();
                cx.notify();
            }
            return;
        }
        if self.focus.ai_prompt.is_focused(window) {
            let registered_target = self.input.target;
            if let Some(prompt) = self.overlay.ai_prompt.as_mut()
                && ai_prompt_input_target_allows(registered_target, prompt.block_id)
            {
                prompt.unmark();
                window.invalidate_character_coordinates();
                cx.notify();
            }
            return;
        }
        if self.focus.code_language.is_focused(window) {
            let registered_target = self.input.target;
            if let Some(edit) = self.overlay.code_language_edit.as_mut() {
                if !code_language_input_target_allows(registered_target, edit.block_id) {
                    trace_input(
                        "unmark_text.code_language_rejected_target",
                        format_args!("registered={:?} block={}", registered_target, edit.block_id),
                    );
                    return;
                }
                edit.unmark();
                window.invalidate_character_coordinates();
                cx.notify();
            }
            return;
        }
        let expected = self.input.session_identity;
        let result = self
            .ready_session()
            .and_then(|runtime| expected.map(|expected| apply_platform_unmark(runtime, expected)));
        match result {
            Some(Ok(outcome)) if outcome.document_changed => {
                self.input.session_identity = outcome.input_identity;
                self.mark_dirty_at_revision(
                    cditor_core::edit::ChangeOrigin::Ime,
                    outcome.revision,
                    cx,
                );
                self.sync_slash_menu_from_runtime(cx);
                window.invalidate_character_coordinates();
                cx.notify();
            }
            Some(Ok(outcome)) => {
                self.input.session_identity = outcome.input_identity;
                if outcome.state_changed {
                    window.invalidate_character_coordinates();
                    cx.notify();
                }
            }
            None => {}
            Some(Err(error)) => {
                self.status.save_status =
                    crate::persistence::EditorSaveStatus::Failed(error.to_string());
                cx.notify();
            }
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pause_caret_blink(cx);
        if let Some(selection) = self.interaction.table_interaction_mode.axis_selection() {
            if !table_menu_input_target_allows(self.input.target, selection.block_id) {
                return;
            }
            if is_single_line_break_commit(text) {
                let _ = self.confirm_table_menu_from_gui(cx);
                window.invalidate_character_coordinates();
                return;
            }
            let range = range_utf16
                .map(|range| utf16_range_to_utf8_range(&self.overlay.table_menu_ui.query, &range))
                .unwrap_or_else(|| self.overlay.table_menu_ui.input_replacement_range());
            let text = normalize_external_line_endings(text).replace('\n', " ");
            self.overlay.table_menu_ui.replace_range(range, &text);
            window.invalidate_character_coordinates();
            cx.notify();
            return;
        }
        if self.focus.ai_prompt.is_focused(window) {
            if is_single_line_break_commit(text) {
                let _ = self.submit_ai_prompt_from_gui(cx);
                window.invalidate_character_coordinates();
                return;
            }
            let registered_target = self.input.target;
            if let Some(prompt) = self.overlay.ai_prompt.as_mut() {
                if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                    return;
                }
                let range = range_utf16
                    .map(|range| utf16_range_to_utf8_range(&prompt.draft, &range))
                    .unwrap_or_else(|| prompt.input_replacement_range());
                let text = normalize_external_line_endings(text);
                prompt.replace_range(range, text.as_ref());
                window.invalidate_character_coordinates();
                cx.notify();
            }
            return;
        }
        if self.focus.code_language.is_focused(window) {
            if is_single_line_break_commit(text) {
                self.commit_code_language_edit(cx);
                window.invalidate_character_coordinates();
                cx.notify();
                return;
            }
            let registered_target = self.input.target;
            if let Some(edit) = self.overlay.code_language_edit.as_mut() {
                if !code_language_input_target_allows(registered_target, edit.block_id) {
                    trace_input(
                        "replace_text_in_range.code_language_rejected_target",
                        format_args!("registered={:?} block={}", registered_target, edit.block_id),
                    );
                    return;
                }
                let range = range_utf16
                    .map(|range| utf16_range_to_utf8_range(&edit.draft, &range))
                    .unwrap_or_else(|| edit.input_replacement_range());
                let text = normalize_external_line_endings(text);
                edit.replace_range(range, text.as_ref());
                window.invalidate_character_coordinates();
                cx.notify();
            }
            return;
        }
        // A newly opened AI prompt may not own the window focus until the next
        // render pass. Do not let the triggering space leak into the document.
        if self.overlay.ai_prompt.is_some() {
            if is_single_line_break_commit(text) {
                let _ = self.submit_ai_prompt_from_gui(cx);
            }
            return;
        }
        self.apply_document_realtime_replacement(range_utf16, text, cx);
        window.invalidate_character_coordinates();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pause_caret_blink(cx);
        if let Some(selection) = self.interaction.table_interaction_mode.axis_selection() {
            if !table_menu_input_target_allows(self.input.target, selection.block_id) {
                return;
            }
            let range = range_utf16
                .map(|range| utf16_range_to_utf8_range(&self.overlay.table_menu_ui.query, &range))
                .unwrap_or_else(|| self.overlay.table_menu_ui.input_replacement_range());
            let new_text = normalize_external_line_endings(new_text).replace('\n', " ");
            let selected_range =
                new_selected_range.map(|range| utf16_range_to_utf8_range(&new_text, &range));
            self.overlay
                .table_menu_ui
                .replace_and_mark_range(range, &new_text, selected_range);
            window.invalidate_character_coordinates();
            cx.notify();
            return;
        }
        if self.focus.ai_prompt.is_focused(window) {
            let registered_target = self.input.target;
            if let Some(prompt) = self.overlay.ai_prompt.as_mut() {
                if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                    return;
                }
                let range = range_utf16
                    .map(|range| utf16_range_to_utf8_range(&prompt.draft, &range))
                    .unwrap_or_else(|| prompt.input_replacement_range());
                let selected_range =
                    new_selected_range.map(|range| utf16_range_to_utf8_range(new_text, &range));
                prompt.replace_and_mark_range(range, new_text, selected_range);
                window.invalidate_character_coordinates();
                cx.notify();
            }
            return;
        }
        if self.focus.code_language.is_focused(window) {
            let registered_target = self.input.target;
            if let Some(edit) = self.overlay.code_language_edit.as_mut() {
                if !code_language_input_target_allows(registered_target, edit.block_id) {
                    trace_input(
                        "replace_and_mark_text_in_range.code_language_rejected_target",
                        format_args!("registered={:?} block={}", registered_target, edit.block_id),
                    );
                    return;
                }
                let range = range_utf16
                    .map(|range| utf16_range_to_utf8_range(&edit.draft, &range))
                    .unwrap_or_else(|| edit.input_replacement_range());
                let selected_range =
                    new_selected_range.map(|range| utf16_range_to_utf8_range(new_text, &range));
                edit.replace_and_mark_range(range, new_text, selected_range);
                window.invalidate_character_coordinates();
                cx.notify();
            }
            return;
        }
        self.apply_document_realtime_composition(range_utf16, new_text, new_selected_range, cx);
        window.invalidate_character_coordinates();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        self.ime_bounds_for_range(range_utf16, element_bounds, window, cx)
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        if let Some(selection) = self.interaction.table_interaction_mode.axis_selection() {
            if !table_menu_input_target_allows(self.input.target, selection.block_id) {
                return None;
            }
            let utf8 = single_line_text_offset_for_x(
                &self.overlay.table_menu_ui.query,
                point.x,
                px(TABLE_MENU_SEARCH_FONT_SIZE_PX),
                _window,
            );
            return Some(utf8_to_utf16_offset(
                &self.overlay.table_menu_ui.query,
                utf8,
            ));
        }
        if self.focus.ai_prompt.is_focused(_window) {
            let registered_target = self.input.target;
            let prompt = self.overlay.ai_prompt.as_ref()?;
            if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                return None;
            }
            let utf8 = single_line_text_offset_for_x(
                &prompt.draft,
                point.x,
                px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
                _window,
            );
            return Some(utf8_to_utf16_offset(&prompt.draft, utf8));
        }
        if self.focus.code_language.is_focused(_window) {
            let registered_target = self.input.target;
            let edit = self.overlay.code_language_edit.as_ref()?;
            if !code_language_input_target_allows(registered_target, edit.block_id) {
                trace_input(
                    "character_index_for_point.code_language_rejected_target",
                    format_args!("registered={:?} block={}", registered_target, edit.block_id),
                );
                return None;
            }
            let utf8 = single_line_text_offset_for_x(
                &edit.draft,
                point.x,
                px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
                _window,
            );
            return Some(utf8_to_utf16_offset(&edit.draft, utf8));
        }
        let session = self.ready_session()?;
        let context = session.input_context().ok()?;
        if !platform_input_target_allows(self.input.target, self.input.session_identity, &context) {
            trace_input(
                "character_index_for_point.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    self.input.target, context.target
                ),
            );
            return None;
        }
        let focused = context.focused_text.as_ref()?;
        let (block_id, text) = (focused.block_id, &focused.text);
        let target = context.target?;
        let surface_id = target.surface_id()?;
        let current = session.surface_version(surface_id).ok().flatten()?;
        let resolved = match target {
            InputTarget::TableCell {
                block_id: target_block_id,
                row,
                col,
            } if target_block_id == block_id => {
                let Some(geometry) =
                    self.resolved_table_cell_text_geometry(current, block_id, row, col)
                else {
                    record_unavailable_geometry();
                    return None;
                };
                let cache = geometry.layout();
                if !platform_input_geometry_allows(
                    self.input.target,
                    self.input.session_identity,
                    self.input.layout_identity,
                    &context,
                    cache,
                ) {
                    record_unavailable_geometry();
                    return None;
                }
                let utf8 = geometry
                    .position_for_window_point(point)
                    .offset
                    .min(text.len());
                Some(utf8_to_utf16_offset(text, utf8))
            }
            InputTarget::BlockText {
                block_id: target_block_id,
            } if target_block_id == block_id => {
                let Some(cache) = self.current_text_layout_cache(current, block_id) else {
                    record_unavailable_geometry();
                    return None;
                };
                if !platform_input_geometry_allows(
                    self.input.target,
                    self.input.session_identity,
                    self.input.layout_identity,
                    &context,
                    cache,
                ) {
                    record_unavailable_geometry();
                    return None;
                }
                let Some(geometry) = self.projected_text_geometry_for_block(current, block_id)
                else {
                    record_unavailable_geometry();
                    return None;
                };
                let utf8 = geometry
                    .position_for_window_point(point)
                    .offset
                    .min(text.len());
                let utf16 = utf8_to_utf16_offset(text, utf8);
                trace_input(
                    "character_index_for_point",
                    format_args!(
                        "block={block_id} point={point:?} utf8={utf8} utf16={utf16} text_len={}",
                        text.len()
                    ),
                );
                Some(utf16)
            }
            target @ (InputTarget::ImageCaption {
                block_id: target_block_id,
            }
            | InputTarget::CollectionTitle {
                block_id: target_block_id,
            }) if target_block_id == block_id => self.ime_character_index_for_text_surface(
                session,
                target.surface_id()?,
                point,
                text,
            ),
            _ => None,
        };
        if let (Some(registered_target), Some(offset_utf16)) = (self.input.target, resolved) {
            self.input
                .set_native_selection_candidate(registered_target, offset_utf16);
            trace_input(
                "native_selection_candidate.stored",
                format_args!("target={registered_target:?} utf16={offset_utf16}"),
            );
        }
        resolved
    }

    #[cfg(feature = "mobile-text-session")]
    fn allows_native_edit_action(
        &mut self,
        action: gpui::PlatformTextEditAction,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        CditorV2View::allows_native_edit_action(self, action)
    }

    #[cfg(feature = "mobile-text-session")]
    fn perform_native_edit_action(
        &mut self,
        action: gpui::PlatformTextEditAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        CditorV2View::perform_native_edit_action(self, action, window, cx)
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        if let Some(selection) = self.interaction.table_interaction_mode.axis_selection() {
            return table_menu_input_target_allows(self.input.target, selection.block_id);
        }
        if self.focus.ai_prompt.is_focused(_window) {
            return self.overlay.ai_prompt.as_ref().is_some_and(|prompt| {
                ai_prompt_input_target_allows(self.input.target, prompt.block_id)
            });
        }
        if self.focus.code_language.is_focused(_window) {
            return self
                .overlay
                .code_language_edit
                .as_ref()
                .is_some_and(|edit| {
                    code_language_input_target_allows(self.input.target, edit.block_id)
                });
        }
        self.focus.editor.is_focused(_window)
            && !self.status.readonly
            && matches!(self.state, CditorViewState::Ready(_))
            && self.ready_session().is_none_or(|session| {
                let Ok(context) = session.input_context() else {
                    return false;
                };
                platform_input_target_allows(
                    self.input.target,
                    self.input.session_identity,
                    &context,
                )
            })
    }
}

#[cfg(all(test, feature = "mobile-text-session"))]
mod native_selection_tests {
    use super::{
        native_selection_collapse_position, native_selection_target_supported, word_range_utf16,
    };
    use cditor_runtime::InputTarget;

    #[test]
    fn local_native_selection_collapses_at_focus_end() {
        let position = native_selection_collapse_position(false, Some(42), Some(3..11)).unwrap();
        assert_eq!(position.block_id, 42);
        assert_eq!(position.offset, 11);
    }

    #[test]
    fn native_dismissal_does_not_truncate_cross_block_selection() {
        assert!(native_selection_collapse_position(true, Some(42), Some(3..11)).is_none());
    }

    #[test]
    fn native_selection_supports_document_text_surfaces_only() {
        assert!(native_selection_target_supported(Some(
            InputTarget::BlockText { block_id: 1 }
        )));
        assert!(native_selection_target_supported(Some(
            InputTarget::TableCell {
                block_id: 2,
                row: 0,
                col: 1,
            }
        )));
        assert!(native_selection_target_supported(Some(
            InputTarget::ImageCaption { block_id: 3 }
        )));
        assert!(native_selection_target_supported(Some(
            InputTarget::CollectionTitle { block_id: 4 }
        )));
        assert!(!native_selection_target_supported(Some(
            InputTarget::ComplexBlock { block_id: 5 }
        )));
        assert!(!native_selection_target_supported(Some(
            InputTarget::BlockChrome { block_id: 6 }
        )));
        assert!(!native_selection_target_supported(None));
    }

    #[test]
    fn collapsed_native_candidate_expands_to_unicode_word() {
        let text = "hello, 世界 👩‍💻 rust";
        assert_eq!(word_range_utf16(text, 2), 0..5);
        assert_eq!(word_range_utf16(text, 5), 0..5);

        let cjk_start = text[..text.find('世').unwrap()].encode_utf16().count();
        assert!(!word_range_utf16(text, cjk_start).is_empty());

        let emoji_start = text[..text.find('👩').unwrap()].encode_utf16().count();
        assert_eq!(
            word_range_utf16(text, emoji_start),
            emoji_start..emoji_start + "👩‍💻".encode_utf16().count()
        );
        assert!(word_range_utf16(text, 6).is_empty());
    }

    #[test]
    fn cjk_candidates_expand_without_splitting_utf16_offsets() {
        let text = "A中文测试B";
        for character in ['中', '文', '测', '试'] {
            let byte_offset = text.find(character).unwrap();
            let candidate = text[..byte_offset].encode_utf16().count();
            let resolved = word_range_utf16(text, candidate);

            assert!(resolved.start <= candidate);
            assert!(resolved.end > candidate);
            assert!(resolved.end <= text.encode_utf16().count());
        }
    }

    #[test]
    fn emoji_candidate_expands_to_the_entire_extended_grapheme() {
        let text = "a👨‍👩‍👧‍👦b";
        let emoji_start = "a".encode_utf16().count();
        let emoji_len = "👨‍👩‍👧‍👦".encode_utf16().count();

        assert_eq!(
            word_range_utf16(text, emoji_start + 2),
            emoji_start..emoji_start + emoji_len
        );
    }

    #[gpui::test]
    fn occluding_hitbox_blocks_native_selection_without_disabling_the_editor(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::{
            AppContext as _, EntityInputHandler as _, InteractiveElement as _, ParentElement as _,
            Render, Styled as _, point, px,
        };

        struct OccludedHost {
            editor: gpui::Entity<crate::editor_view::CditorV2View>,
        }

        impl Render for OccludedHost {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl gpui::IntoElement {
                gpui::div()
                    .relative()
                    .size_full()
                    .child(self.editor.clone())
                    .child(gpui::div().absolute().inset_0().occlude())
            }
        }

        let mut runtime = cditor_runtime::DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
                1,
                cditor_core::rich_text::RichBlockKind::Paragraph,
                "selectable text",
            )],
            720.0,
        );
        crate::test_support::focus_block_at_offset(&mut runtime, 1, 4);
        let session = cditor_session::EditorSession::new(runtime, false).into_handle();
        let (host, cx) = cx.add_window_view(|_, cx| {
            let editor = cx.new(|cx| {
                let mut editor = crate::editor_view::CditorV2View::loading("", false, cx);
                editor.apply_loaded_session(session, cx);
                editor
            });
            OccludedHost { editor }
        });

        cx.update(|window, cx| {
            let editor = host.read(cx).editor.clone();
            editor.update(cx, |view, cx| view.focus.editor.focus(window, cx));
            let _ = window.draw(cx);
        });

        cx.update(|window, cx| {
            let editor = host.read(cx).editor.clone();
            editor.update(cx, |view, cx| {
                assert!(view.handles_native_selection(window, cx));
                let bounds = view.input.element_bounds.expect("input bounds registered");
                let point = point(bounds.left() + px(2.0), bounds.top() + px(2.0));
                let hitbox_id = view.input.hitbox_id.expect("input hitbox registered");
                assert!(!window.point_reaches_hitbox(hitbox_id, point));
                assert!(!view.native_selection_allowed_at(point, window, cx));
            });
        });
    }
}
