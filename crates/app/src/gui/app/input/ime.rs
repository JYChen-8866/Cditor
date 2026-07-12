use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, px};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState, GuiPlatformInputTarget};
use crate::gui::app::input_trace::trace_input;
use crate::gui::input::ime::{
    marked_preview_range_to_base_range, utf8_range_to_utf16_range, utf8_to_utf16_offset,
    utf16_range_to_utf8_range,
};
use crate::gui::input::{SINGLE_LINE_INPUT_FONT_SIZE_PX, single_line_text_offset_for_x};
use crate::gui::text::platform_index_for_point;
use cditor_core::ids::BlockId;
use cditor_runtime::{DocumentRuntime, InputTarget};

impl EntityInputHandler for CditorV2View {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        if self.ai_prompt_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let prompt = self.ai_prompt.as_ref()?;
            if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                return None;
            }
            let range = utf16_range_to_utf8_range(&prompt.draft, &range_utf16);
            let actual = utf8_range_to_utf16_range(&prompt.draft, &range);
            actual_range.replace(actual);
            return prompt.draft.get(range).map(ToOwned::to_owned);
        }
        if self.code_language_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let edit = self.code_language_edit.as_ref()?;
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
        let registered_target = self.platform_input_target;
        let runtime = self.ready_runtime()?;
        if !platform_input_target_allows(registered_target, runtime) {
            trace_input(
                "text_for_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    registered_target,
                    runtime.input_session_target()
                ),
            );
            return None;
        }
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
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
        if self.ai_prompt_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let prompt = self.ai_prompt.as_ref()?;
            if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                return None;
            }
            let caret = utf8_to_utf16_offset(&prompt.draft, prompt.caret_offset);
            return Some(UTF16Selection {
                range: caret..caret,
                reversed: false,
            });
        }
        if self.code_language_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let edit = self.code_language_edit.as_ref()?;
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
        let registered_target = self.platform_input_target;
        let runtime = self.ready_runtime()?;
        if !platform_input_target_allows(registered_target, runtime) {
            trace_input(
                "selected_text_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    registered_target,
                    runtime.input_session_target()
                ),
            );
            return None;
        }
        let selection = platform_selected_text_range(runtime);
        trace_input(
            "selected_text_range",
            format_args!(
                "focused={:?} selection={selection:?}",
                runtime.focused_block_id()
            ),
        );
        selection
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if self.ai_prompt_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let prompt = self.ai_prompt.as_ref()?;
            if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                return None;
            }
            return prompt
                .marked_range
                .as_ref()
                .map(|range| utf8_range_to_utf16_range(&prompt.draft, range));
        }
        if self.code_language_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let edit = self.code_language_edit.as_ref()?;
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
        let runtime = self.ready_runtime_ref()?;
        if !platform_input_target_allows(self.platform_input_target, runtime) {
            trace_input(
                "marked_text_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    self.platform_input_target,
                    runtime.input_session_target()
                ),
            );
            return None;
        }
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let marked = runtime
            .input_session_marked_range()
            .or_else(|| runtime.active_composition_marked_range())
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

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_prompt_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            if let Some(prompt) = self.ai_prompt.as_mut()
                && ai_prompt_input_target_allows(registered_target, prompt.block_id)
            {
                prompt.unmark();
                cx.notify();
            }
            return;
        }
        if self.code_language_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            if let Some(edit) = self.code_language_edit.as_mut() {
                if !code_language_input_target_allows(registered_target, edit.block_id) {
                    trace_input(
                        "unmark_text.code_language_rejected_target",
                        format_args!("registered={:?} block={}", registered_target, edit.block_id),
                    );
                    return;
                }
                edit.unmark();
                cx.notify();
            }
            return;
        }
        if let Some(runtime) = self.ready_runtime() {
            runtime.cancel_composition();
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai_prompt_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            if let Some(prompt) = self.ai_prompt.as_mut() {
                if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                    return;
                }
                let range = range_utf16
                    .map(|range| utf16_range_to_utf8_range(&prompt.draft, &range))
                    .unwrap_or_else(|| prompt.input_replacement_range());
                prompt.replace_range(range, text);
                cx.notify();
            }
            return;
        }
        if self.code_language_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            if let Some(edit) = self.code_language_edit.as_mut() {
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
                edit.replace_range(range, text);
                cx.notify();
            }
            return;
        }
        // A newly opened AI prompt may not own the window focus until the next
        // render pass. Do not let the triggering space leak into the document.
        if self.ai_prompt.is_some() {
            return;
        }
        if self.readonly {
            return;
        }
        let registered_target = self.platform_input_target;
        let empty_line_ai_input = is_empty_line_ai_platform_input(range_utf16.as_ref(), text)
            && self.ready_runtime_ref().is_some_and(|runtime| {
                platform_input_target_allows(registered_target, runtime)
                    && runtime.focused_empty_text_block_for_ai().is_some()
            });
        if empty_line_ai_input && self.invoke_empty_line_ai_from_gui(cx) {
            cx.notify();
            return;
        }
        let Some(runtime) = self.ready_runtime() else {
            return;
        };
        if !platform_input_target_allows(registered_target, runtime) {
            trace_input(
                "replace_text_in_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    registered_target,
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
        match runtime.replace_text_in_focused_range(range, text) {
            Ok(true) => {
                self.mark_dirty(cx);
                self.sync_slash_menu_from_runtime(cx);
                cx.notify();
            }
            Ok(false) => {}
            Err(error) => {
                self.save_status = crate::gui::persistence::EditorSaveStatus::Failed(error);
                cx.notify();
            }
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai_prompt_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            if let Some(prompt) = self.ai_prompt.as_mut() {
                if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                    return;
                }
                let range = range_utf16
                    .map(|range| utf16_range_to_utf8_range(&prompt.draft, &range))
                    .unwrap_or_else(|| prompt.input_replacement_range());
                let selected_range =
                    new_selected_range.map(|range| utf16_range_to_utf8_range(new_text, &range));
                prompt.replace_and_mark_range(range, new_text, selected_range);
                cx.notify();
            }
            return;
        }
        if self.code_language_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            if let Some(edit) = self.code_language_edit.as_mut() {
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
                cx.notify();
            }
            return;
        }
        if self.readonly {
            return;
        }
        let registered_target = self.platform_input_target;
        let Some(runtime) = self.ready_runtime() else {
            return;
        };
        if !platform_input_target_allows(registered_target, runtime) {
            trace_input(
                "replace_and_mark_text_in_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    registered_target,
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
            .map(|range| utf16_range_to_utf8_range(new_text, &range));
        trace_input(
            "replace_and_mark_text_in_range",
            format_args!(
                "block={block_id} range_utf16={range_utf16:?} range_from_ime={range_from_ime:?} resolved_utf8={range:?} new_text_len={} new_selected_utf16={new_selected_range:?} selected_utf8={selected_range:?}",
                new_text.len()
            ),
        );
        if runtime
            .begin_or_update_composition_with_selection(block_id, range, new_text, selected_range)
            .is_ok()
        {
            self.sync_slash_menu_from_runtime(cx);
            cx.notify();
        }
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
        if self.ai_prompt_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let prompt = self.ai_prompt.as_ref()?;
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
        if self.code_language_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let edit = self.code_language_edit.as_ref()?;
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
        let runtime = self.ready_runtime_ref()?;
        if !platform_input_target_allows(self.platform_input_target, runtime) {
            trace_input(
                "character_index_for_point.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    self.platform_input_target,
                    runtime.input_session_target()
                ),
            );
            return None;
        }
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        match runtime.input_session_target()? {
            InputTarget::TableCell {
                block_id: target_block_id,
                row,
                col,
            } if target_block_id == block_id => {
                let cache = self.current_table_cell_layout_cache(runtime, block_id, row, col)?;
                let utf8 = platform_index_for_point(cache, point).min(text.len());
                Some(utf8_to_utf16_offset(&text, utf8))
            }
            InputTarget::BlockText {
                block_id: target_block_id,
            } if target_block_id == block_id => {
                let cache = self.current_text_layout_cache(runtime, block_id)?;
                let utf8 = platform_index_for_point(cache, point).min(text.len());
                let utf16 = utf8_to_utf16_offset(&text, utf8);
                trace_input(
                    "character_index_for_point",
                    format_args!(
                        "block={block_id} point={point:?} utf8={utf8} utf16={utf16} text_len={}",
                        text.len()
                    ),
                );
                Some(utf16)
            }
            _ => None,
        }
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        if self.ai_prompt_focus.is_focused(_window) {
            return self.ai_prompt.as_ref().is_some_and(|prompt| {
                ai_prompt_input_target_allows(self.platform_input_target, prompt.block_id)
            });
        }
        if self.code_language_focus.is_focused(_window) {
            return self.code_language_edit.as_ref().is_some_and(|edit| {
                code_language_input_target_allows(self.platform_input_target, edit.block_id)
            });
        }
        !self.readonly
            && matches!(self.state, CditorViewState::Ready(_))
            && self.ready_runtime_ref().is_none_or(|runtime| {
                platform_input_target_allows(self.platform_input_target, runtime)
            })
    }
}

pub(in crate::gui::app) fn platform_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    runtime: &DocumentRuntime,
) -> bool {
    let Some(registered) = registered else {
        return true;
    };
    let Some(runtime_target) = runtime.input_session_target() else {
        return false;
    };
    registered.matches_runtime_target(runtime_target)
}

pub(in crate::gui::app) fn code_language_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    block_id: BlockId,
) -> bool {
    let Some(registered) = registered else {
        return true;
    };
    registered.is_code_language_for(block_id)
}

pub(in crate::gui::app) fn ai_prompt_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    block_id: BlockId,
) -> bool {
    registered.is_some_and(|target| target.is_ai_prompt_for(block_id))
}

pub(in crate::gui::app) fn platform_selected_text_range(
    runtime: &DocumentRuntime,
) -> Option<UTF16Selection> {
    let (block_id, text) = runtime.focused_text_for_platform_input()?;
    if let Some(selection) = runtime.input_session_selected_range() {
        return Some(UTF16Selection {
            range: utf8_range_to_utf16_range(&text, &selection),
            reversed: runtime.input_session_selection_reversed(),
        });
    }
    if let Some(marked_range) = runtime.active_composition_marked_range() {
        let caret = utf8_to_utf16_offset(&text, marked_range.end.min(text.len()));
        return Some(UTF16Selection {
            range: caret..caret,
            reversed: false,
        });
    }
    if runtime.input_session_target().is_some() {
        trace_input(
            "selected_text_range.missing_session_selection",
            format_args!(
                "block={block_id} target={:?}",
                runtime.input_session_target()
            ),
        );
        return None;
    }
    if let Some(selection) = runtime.focused_text_selection_range() {
        return Some(UTF16Selection {
            range: utf8_range_to_utf16_range(&text, &selection),
            reversed: false,
        });
    }
    let caret = runtime
        .focused_table_cell_offset()
        .filter(|(focused_block_id, _, _, _)| *focused_block_id == block_id)
        .map(|(_, _, _, offset)| offset)
        .unwrap_or(0)
        .min(text.len());
    let caret = utf8_to_utf16_offset(&text, caret);
    Some(UTF16Selection {
        range: caret..caret,
        reversed: false,
    })
}

pub(in crate::gui::app) fn platform_input_fallback_range(
    runtime: &DocumentRuntime,
    block_id: BlockId,
) -> Range<usize> {
    runtime
        .active_composition()
        .filter(|composition| composition.block_id == block_id)
        .map(|composition| composition.range_start as usize..composition.range_end as usize)
        .or_else(|| runtime.input_session_marked_range())
        .or_else(|| runtime.input_session_selected_range())
        .unwrap_or_else(|| {
            if runtime.input_session_target().is_some() {
                trace_input(
                    "platform_input_fallback_range.missing_session_selection",
                    format_args!(
                        "block={block_id} target={:?}",
                        runtime.input_session_target()
                    ),
                );
                return 0..0;
            }
            let caret = runtime
                .focused_text_selection_range()
                .map(|range| range.end)
                .unwrap_or(0);
            caret..caret
        })
}

fn ime_replacement_range(
    runtime: &DocumentRuntime,
    range_utf16: Option<Range<usize>>,
) -> Option<Range<usize>> {
    let range_utf16 = range_utf16?;
    let (_block_id, text) = runtime.focused_text_for_platform_input()?;
    let preview_range = utf16_range_to_utf8_range(&text, &range_utf16);
    let Some(composition) = runtime.active_composition() else {
        return Some(preview_range);
    };
    let preview_marked_range = runtime.active_composition_marked_range()?;
    let base_marked_range = composition.range_start as usize..composition.range_end as usize;
    Some(marked_preview_range_to_base_range(
        preview_range,
        base_marked_range,
        preview_marked_range,
    ))
}

fn is_empty_line_ai_platform_input(range_utf16: Option<&Range<usize>>, text: &str) -> bool {
    text == " " && range_utf16.is_none_or(|range| range.is_empty())
}

#[cfg(test)]
mod tests {
    use super::is_empty_line_ai_platform_input;

    #[test]
    fn platform_space_commit_is_recognized_without_replacing_text() {
        assert!(is_empty_line_ai_platform_input(None, " "));
        assert!(is_empty_line_ai_platform_input(Some(&(0..0)), " "));
        assert!(!is_empty_line_ai_platform_input(Some(&(0..1)), " "));
        assert!(!is_empty_line_ai_platform_input(None, "x"));
    }
}
