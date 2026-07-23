use std::borrow::Cow;
use std::ops::Range;

use gpui::UTF16Selection;

use cditor_core::ids::BlockId;
use cditor_runtime::{
    DocumentRuntime, InputSessionIdentity, RealtimeInput, RealtimeInputOutcome,
    RealtimeInputRequest,
};
use cditor_session::{
    InputContextSnapshot, SessionRealtimeError, project_input_context, project_realtime_input,
};

use crate::app::cditor_v2_view::GuiPlatformInputTarget;
use crate::app::input_trace::trace_input;
use crate::input::ime::{
    marked_preview_range_to_base_range, utf8_range_to_utf16_range, utf8_to_utf16_offset,
    utf16_range_to_utf8_range,
};
use crate::text::{RichTextPlatformLayout, TextPlatformLayoutIdentity};

pub(crate) trait InputContextSource {
    fn input_context(&self) -> Cow<'_, InputContextSnapshot>;
}

impl InputContextSource for InputContextSnapshot {
    fn input_context(&self) -> Cow<'_, InputContextSnapshot> {
        Cow::Borrowed(self)
    }
}

#[cfg(test)]
impl InputContextSource for DocumentRuntime {
    fn input_context(&self) -> Cow<'_, InputContextSnapshot> {
        Cow::Owned(project_input_context(self))
    }
}

pub(super) fn apply_platform_text_replacement(
    runtime: &mut DocumentRuntime,
    expected: InputSessionIdentity,
    range: Option<Range<usize>>,
    text: &str,
) -> Result<RealtimeInputOutcome, SessionRealtimeError> {
    let context = project_input_context(runtime);
    let has_active_selection = context.has_active_selection;
    let route = if text.is_empty() && has_active_selection {
        "delete_active_selection"
    } else {
        "replace_focused_range"
    };
    trace_input(
        "platform_text_replacement.start",
        format_args!(
            "route={route} text_len={} explicit_range={range:?} focus={:?} active_selection={has_active_selection} cross_block={} focused_range={:?} session_range={:?}",
            text.len(),
            context.focused_block_id,
            context.has_cross_block_text_selection,
            context.focused_text_selection_range,
            context.selected_range,
        ),
    );
    let result = project_realtime_input(
        runtime,
        RealtimeInputRequest {
            expected,
            input: RealtimeInput::ReplaceText { range, text },
        },
        false,
    );
    trace_input(
        "platform_text_replacement.end",
        format_args!("route={route} result={result:?}"),
    );
    result
}

pub(super) fn apply_platform_unmark(
    runtime: &mut DocumentRuntime,
    expected: InputSessionIdentity,
) -> Result<RealtimeInputOutcome, SessionRealtimeError> {
    project_realtime_input(
        runtime,
        RealtimeInputRequest {
            expected,
            input: RealtimeInput::UnmarkComposition,
        },
        false,
    )
}

pub(in crate::app) fn platform_input_target_allows<S: InputContextSource + ?Sized>(
    registered: Option<GuiPlatformInputTarget>,
    registered_identity: Option<InputSessionIdentity>,
    source: &S,
) -> bool {
    let context = source.input_context();
    let Some(registered) = registered else {
        return false;
    };
    let Some(runtime_target) = context.target else {
        return false;
    };
    registered.matches_runtime_target(runtime_target) && registered_identity == context.identity
}

pub(in crate::app) fn platform_input_geometry_allows<S: InputContextSource + ?Sized>(
    registered: Option<GuiPlatformInputTarget>,
    registered_session: Option<InputSessionIdentity>,
    registered_layout: Option<TextPlatformLayoutIdentity>,
    source: &S,
    cache: &RichTextPlatformLayout,
) -> bool {
    platform_input_target_allows(registered, registered_session, source)
        && registered_layout == Some(cache.identity())
        && cache.input_session_identity == registered_session
}

pub(in crate::app) fn code_language_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    block_id: BlockId,
) -> bool {
    let Some(registered) = registered else {
        return true;
    };
    registered.is_code_language_for(block_id)
}

pub(in crate::app) fn ai_prompt_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    block_id: BlockId,
) -> bool {
    registered.is_some_and(|target| target.is_ai_prompt_for(block_id))
}

pub(in crate::app) fn table_menu_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    block_id: BlockId,
) -> bool {
    registered.is_some_and(|target| target.is_table_menu_query_for(block_id))
}

pub(in crate::app) fn platform_selected_text_range<S: InputContextSource + ?Sized>(
    source: &S,
) -> Option<UTF16Selection> {
    let context = source.input_context();
    let focused = context.focused_text.as_ref()?;
    let block_id = focused.block_id;
    let text = &focused.text;
    if let Some(selection) = &context.selected_range {
        return Some(UTF16Selection {
            range: utf8_range_to_utf16_range(text, selection),
            reversed: context.selection_reversed,
        });
    }
    if let Some(marked_range) = context
        .composition
        .as_ref()
        .and_then(|composition| composition.preview_marked_range.as_ref())
    {
        let caret = utf8_to_utf16_offset(text, marked_range.end.min(text.len()));
        return Some(UTF16Selection {
            range: caret..caret,
            reversed: false,
        });
    }
    if context.target.is_some() {
        trace_input(
            "selected_text_range.missing_session_selection",
            format_args!("block={block_id} target={:?}", context.target),
        );
        return None;
    }
    if let Some(selection) = &context.focused_text_selection_range {
        return Some(UTF16Selection {
            range: utf8_range_to_utf16_range(text, selection),
            reversed: false,
        });
    }
    let caret = context
        .focused_table_cell_offset
        .filter(|(focused_block_id, _, _, _)| *focused_block_id == block_id)
        .map(|(_, _, _, offset)| offset)
        .unwrap_or(0)
        .min(text.len());
    let caret = utf8_to_utf16_offset(text, caret);
    Some(UTF16Selection {
        range: caret..caret,
        reversed: false,
    })
}

pub(in crate::app) fn platform_input_fallback_range<S: InputContextSource + ?Sized>(
    source: &S,
    block_id: BlockId,
) -> Range<usize> {
    let context = source.input_context();
    context
        .composition
        .as_ref()
        .filter(|composition| composition.block_id == block_id)
        .map(|composition| composition.base_range.clone())
        .or_else(|| context.marked_range.clone())
        .or_else(|| context.selected_range.clone())
        .unwrap_or_else(|| {
            if context.target.is_some() {
                trace_input(
                    "platform_input_fallback_range.missing_session_selection",
                    format_args!("block={block_id} target={:?}", context.target),
                );
                return 0..0;
            }
            let caret = context
                .focused_text_selection_range
                .as_ref()
                .map(|range| range.end)
                .unwrap_or(0);
            caret..caret
        })
}

pub(super) fn ime_replacement_range<S: InputContextSource + ?Sized>(
    source: &S,
    range_utf16: Option<Range<usize>>,
) -> Option<Range<usize>> {
    let context = source.input_context();
    let range_utf16 = range_utf16?;
    let text = &context.focused_text.as_ref()?.text;
    let preview_range = utf16_range_to_utf8_range(text, &range_utf16);
    let Some(composition) = &context.composition else {
        return Some(preview_range);
    };
    let preview_marked_range = composition.preview_marked_range.clone()?;
    let base_marked_range = composition.base_range.clone();
    Some(marked_preview_range_to_base_range(
        preview_range,
        base_marked_range,
        preview_marked_range,
    ))
}

pub(super) fn is_empty_line_ai_platform_input(
    range_utf16: Option<&Range<usize>>,
    text: &str,
) -> bool {
    text == " " && range_utf16.is_none_or(|range| range.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        apply_platform_text_replacement, apply_platform_unmark, is_empty_line_ai_platform_input,
        platform_input_geometry_allows, platform_input_target_allows,
    };
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
    use cditor_runtime::{DocumentRuntime, RealtimeInput, RealtimeInputRequest};
    use gpui::{point, px, size};

    use crate::app::GuiPlatformInputTarget;
    use crate::text::test_platform_layout;

    fn undo(runtime: &mut DocumentRuntime) -> bool {
        runtime
            .dispatch(CommandEnvelope::new(
                EditorCommand::Undo,
                CommandSource::Ime,
            ))
            .unwrap()
            .changed()
    }

    fn update_composition(runtime: &mut DocumentRuntime, text: &str) {
        let expected = runtime.input_session_identity().unwrap();
        runtime
            .apply_realtime_input(RealtimeInputRequest {
                expected,
                input: RealtimeInput::UpdateComposition {
                    range: 1..1,
                    text,
                    selected_range: None,
                },
            })
            .unwrap();
    }

    #[test]
    fn platform_space_commit_is_recognized_without_replacing_text() {
        assert!(is_empty_line_ai_platform_input(None, " "));
        assert!(is_empty_line_ai_platform_input(Some(&(0..0)), " "));
        assert!(!is_empty_line_ai_platform_input(Some(&(0..1)), " "));
        assert!(!is_empty_line_ai_platform_input(None, "x"));
    }

    #[test]
    fn printable_platform_callback_creates_exactly_one_undoable_edit() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "ab",
            )],
            720.0,
        );
        crate::test_support::focus_block_at_offset(&mut runtime, 1, 1);

        let expected = runtime.input_session_identity().unwrap();
        assert!(
            apply_platform_text_replacement(&mut runtime, expected, None, "中")
                .unwrap()
                .document_changed
        );
        assert_eq!(runtime.focused_text(), Some("a中b"));
        assert!(undo(&mut runtime));
        assert_eq!(runtime.focused_text(), Some("ab"));
        assert!(!undo(&mut runtime));
    }

    #[test]
    fn printable_platform_callback_replaces_selection_without_second_insertion() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        crate::test_support::set_document_text_selection(&mut runtime, 1, 1, 1, 3);

        let expected = runtime.input_session_identity().unwrap();
        assert!(
            apply_platform_text_replacement(&mut runtime, expected, None, "X")
                .unwrap()
                .document_changed
        );
        assert_eq!(runtime.focused_text(), Some("aXd"));
        assert_eq!(runtime.caret_offset_for_block(1), Some(2));
        assert!(undo(&mut runtime));
        assert_eq!(runtime.focused_text(), Some("abcd"));
        assert!(!undo(&mut runtime));
    }

    #[test]
    fn platform_unmark_commits_preview_as_one_undoable_edit() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "ab",
            )],
            720.0,
        );
        crate::test_support::focus_block_at_offset(&mut runtime, 1, 1);
        update_composition(&mut runtime, "中");

        let expected = runtime.input_session_identity().unwrap();
        assert!(
            apply_platform_unmark(&mut runtime, expected)
                .unwrap()
                .document_changed
        );
        assert_eq!(runtime.focused_text(), Some("a中b"));
        assert!(runtime.active_composition().is_none());
        assert!(undo(&mut runtime));
        assert_eq!(runtime.focused_text(), Some("ab"));
    }

    #[test]
    fn candidate_geometry_rejects_stale_session_and_composition_identity() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abc",
            )],
            720.0,
        );
        crate::test_support::focus_block_at_offset(&mut runtime, 1, 1);
        let registered_session = runtime.input_session_identity();
        let target = Some(GuiPlatformInputTarget::BlockText { block_id: 1 });
        let mut cache = test_platform_layout(
            1,
            1,
            "abc",
            gpui::Bounds::new(point(px(0.0), px(0.0)), size(px(200.0), px(40.0))),
            None,
        );
        cache.input_session_identity = registered_session;
        let registered_layout = Some(cache.identity());

        assert!(platform_input_target_allows(
            target,
            registered_session,
            &runtime
        ));
        assert!(platform_input_geometry_allows(
            target,
            registered_session,
            registered_layout,
            &runtime,
            &cache,
        ));

        update_composition(&mut runtime, "你");
        assert!(!platform_input_geometry_allows(
            target,
            registered_session,
            registered_layout,
            &runtime,
            &cache,
        ));

        let current_session = runtime.input_session_identity();
        assert!(!platform_input_geometry_allows(
            target,
            current_session,
            registered_layout,
            &runtime,
            &cache,
        ));
        cache.input_session_identity = current_session;
        assert!(platform_input_geometry_allows(
            target,
            current_session,
            registered_layout,
            &runtime,
            &cache,
        ));
    }

    #[test]
    fn empty_platform_replacement_deletes_cross_block_document_selection() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "ab"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "middle"),
                BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "cd"),
            ],
            720.0,
        );
        crate::test_support::set_document_text_selection(&mut runtime, 1, 1, 3, 1);

        let expected = runtime.input_session_identity().unwrap();
        assert!(
            apply_platform_text_replacement(&mut runtime, expected, Some(1..1), "")
                .unwrap()
                .document_changed
        );
        assert_eq!(runtime.focused_text(), Some("ad"));
        assert_eq!(runtime.projection_for_window().blocks.len(), 1);
        assert!(!runtime.has_active_selection());
    }

    #[test]
    fn empty_platform_replacement_deletes_same_block_document_selection() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        crate::test_support::set_document_text_selection(&mut runtime, 1, 1, 1, 3);

        let expected = runtime.input_session_identity().unwrap();
        assert!(
            apply_platform_text_replacement(&mut runtime, expected, Some(1..3), "")
                .unwrap()
                .document_changed
        );
        assert_eq!(runtime.focused_text(), Some("ad"));
        assert_eq!(runtime.caret_offset_for_block(1), Some(1));
        assert!(!runtime.has_active_selection());
    }
}
