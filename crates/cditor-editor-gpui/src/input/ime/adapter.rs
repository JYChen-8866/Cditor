use std::ops::Range;

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
use cditor_runtime::InputTarget;

use super::support::{
    ai_prompt_input_target_allows, apply_platform_unmark, platform_input_geometry_allows,
    table_menu_input_target_allows,
};
pub(crate) use super::support::{
    code_language_input_target_allows, platform_input_target_allows, platform_selected_text_range,
};

impl EntityInputHandler for CditorV2View {
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
            actual_range.replace(actual.clone());
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
        let selection = platform_selected_text_range(&context);
        trace_input(
            "selected_text_range",
            format_args!(
                "focused={:?} selection={selection:?}",
                context.focused_block_id
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
        match target {
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
        }
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
        !self.status.readonly
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
