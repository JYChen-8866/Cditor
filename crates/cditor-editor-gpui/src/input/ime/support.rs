use std::borrow::Cow;
use std::ops::Range;

use gpui::UTF16Selection;

use cditor_core::ids::BlockId;
use cditor_runtime::{
    InputSessionIdentity, RealtimeInput, RealtimeInputOutcome, RealtimeInputRequest,
};
use cditor_session::{EditorSessionHandle, InputContextSnapshot, SessionRealtimeError};

#[cfg(test)]
use cditor_runtime::DocumentRuntime;
#[cfg(test)]
use cditor_session::project_input_context;

use crate::editor_view::GuiPlatformInputTarget;
use crate::input::ime::{
    marked_preview_range_to_base_range, utf8_range_to_utf16_range, utf8_to_utf16_offset,
    utf16_range_to_utf8_range,
};
use crate::input::trace::trace_input;
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
    session: &EditorSessionHandle,
    expected: InputSessionIdentity,
    range: Option<Range<usize>>,
    text: &str,
) -> Result<RealtimeInputOutcome, SessionRealtimeError> {
    let context = session
        .input_context()
        .map_err(SessionRealtimeError::Protocol)?;
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
    let result = session.apply_realtime_input(RealtimeInputRequest {
        expected,
        input: RealtimeInput::ReplaceText { range, text },
    });
    trace_input(
        "platform_text_replacement.end",
        format_args!("route={route} result={result:?}"),
    );
    result
}

pub(super) fn apply_platform_unmark(
    session: &EditorSessionHandle,
    expected: InputSessionIdentity,
) -> Result<RealtimeInputOutcome, SessionRealtimeError> {
    session.apply_realtime_input(RealtimeInputRequest {
        expected,
        input: RealtimeInput::UnmarkComposition,
    })
}

pub(crate) fn platform_input_target_allows<S: InputContextSource + ?Sized>(
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

pub(crate) fn platform_input_geometry_allows<S: InputContextSource + ?Sized>(
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

pub(crate) fn code_language_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    block_id: BlockId,
) -> bool {
    let Some(registered) = registered else {
        return true;
    };
    registered.is_code_language_for(block_id)
}

pub(crate) fn ai_prompt_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    block_id: BlockId,
) -> bool {
    registered.is_some_and(|target| target.is_ai_prompt_for(block_id))
}

pub(crate) fn table_menu_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    block_id: BlockId,
) -> bool {
    registered.is_some_and(|target| target.is_table_menu_query_for(block_id))
}

pub(crate) fn platform_selected_text_range<S: InputContextSource + ?Sized>(
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

pub(crate) fn platform_input_fallback_range<S: InputContextSource + ?Sized>(
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
        ai_prompt_input_target_allows, apply_platform_text_replacement, apply_platform_unmark,
        code_language_input_target_allows, is_empty_line_ai_platform_input,
        platform_input_geometry_allows, platform_input_target_allows,
        table_menu_input_target_allows,
    };
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_runtime::{DocumentRuntime, RealtimeInput, RealtimeInputRequest};
    use gpui::{point, px, size};

    use crate::editor_view::GuiPlatformInputTarget;
    use crate::text::test_platform_layout;

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
    fn auxiliary_input_targets_are_isolated_from_document_selection() {
        let prompt = Some(GuiPlatformInputTarget::ai_prompt(7));
        let code = Some(GuiPlatformInputTarget::code_language(7));
        let table_menu = Some(GuiPlatformInputTarget::table_menu_query(7));
        let document = Some(GuiPlatformInputTarget::BlockText { block_id: 7 });

        assert!(ai_prompt_input_target_allows(prompt, 7));
        assert!(!ai_prompt_input_target_allows(None, 7));
        assert!(!ai_prompt_input_target_allows(code, 7));
        assert!(!ai_prompt_input_target_allows(document, 7));
        assert!(!ai_prompt_input_target_allows(prompt, 8));

        // Code-language editing may bootstrap before the first registration
        // frame, but any registered owner must match exactly.
        assert!(code_language_input_target_allows(None, 7));
        assert!(code_language_input_target_allows(code, 7));
        assert!(!code_language_input_target_allows(prompt, 7));
        assert!(!code_language_input_target_allows(document, 7));
        assert!(!code_language_input_target_allows(code, 8));

        assert!(table_menu_input_target_allows(table_menu, 7));
        assert!(!table_menu_input_target_allows(None, 7));
        assert!(!table_menu_input_target_allows(prompt, 7));
        assert!(!table_menu_input_target_allows(table_menu, 8));
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
        let session = cditor_session::EditorSession::new(runtime, false).into_handle();
        assert!(
            apply_platform_text_replacement(&session, expected, None, "中")
                .unwrap()
                .document_changed
        );
        assert_eq!(
            session.block_plain_text(1).unwrap().as_deref(),
            Some("a中b")
        );
        assert!(
            session
                .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                    cditor_editor_protocol::command::EditorCommand::Undo,
                    cditor_editor_protocol::command::CommandSource::Automation
                ))
                .unwrap()
                .changed()
        );
        assert_eq!(session.block_plain_text(1).unwrap().as_deref(), Some("ab"));
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
        let session = cditor_session::EditorSession::new(runtime, false).into_handle();
        assert!(
            apply_platform_text_replacement(&session, expected, None, "X")
                .unwrap()
                .document_changed
        );
        assert_eq!(session.block_plain_text(1).unwrap().as_deref(), Some("aXd"));
        assert_eq!(
            session.text_block_context(1).unwrap().unwrap().caret,
            Some(2)
        );
        assert!(
            session
                .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                    cditor_editor_protocol::command::EditorCommand::Undo,
                    cditor_editor_protocol::command::CommandSource::Automation
                ))
                .unwrap()
                .changed()
        );
        assert_eq!(
            session.block_plain_text(1).unwrap().as_deref(),
            Some("abcd")
        );
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
        let session = cditor_session::EditorSession::new(runtime, false).into_handle();
        assert!(
            apply_platform_unmark(&session, expected)
                .unwrap()
                .document_changed
        );
        assert_eq!(
            session.block_plain_text(1).unwrap().as_deref(),
            Some("a中b")
        );
        assert!(session.input_context().unwrap().composition.is_none());
        assert!(
            session
                .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                    cditor_editor_protocol::command::EditorCommand::Undo,
                    cditor_editor_protocol::command::CommandSource::Automation
                ))
                .unwrap()
                .changed()
        );
        assert_eq!(session.block_plain_text(1).unwrap().as_deref(), Some("ab"));
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
        let session = cditor_session::EditorSession::new(runtime, false).into_handle();
        assert!(
            apply_platform_text_replacement(&session, expected, Some(1..1), "")
                .unwrap()
                .document_changed
        );
        assert_eq!(session.block_plain_text(1).unwrap().as_deref(), Some("ad"));
        assert_eq!(session.document_snapshot().unwrap().block_count, 1);
        assert!(!session.input_context().unwrap().has_active_selection);
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
        let session = cditor_session::EditorSession::new(runtime, false).into_handle();
        assert!(
            apply_platform_text_replacement(&session, expected, Some(1..3), "")
                .unwrap()
                .document_changed
        );
        assert_eq!(session.block_plain_text(1).unwrap().as_deref(), Some("ad"));
        assert_eq!(
            session.text_block_context(1).unwrap().unwrap().caret,
            Some(1)
        );
        assert!(!session.input_context().unwrap().has_active_selection);
    }
}
