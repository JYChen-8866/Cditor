use std::ops::Range;

use gpui::UTF16Selection;

use cditor_core::ids::BlockId;
use cditor_runtime::DocumentRuntime;

use crate::gui::app::cditor_v2_view::GuiPlatformInputTarget;
use crate::gui::app::input_trace::trace_input;
use crate::gui::input::ime::{
    marked_preview_range_to_base_range, utf8_range_to_utf16_range, utf8_to_utf16_offset,
    utf16_range_to_utf8_range,
};

pub(super) fn apply_platform_text_replacement(
    runtime: &mut DocumentRuntime,
    range: Option<Range<usize>>,
    text: &str,
) -> Result<bool, String> {
    let has_active_selection = runtime.has_active_selection();
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
            runtime.focused_block_id(),
            runtime.has_cross_block_text_selection(),
            runtime.focused_text_selection_range(),
            runtime.input_session_selected_range(),
        ),
    );
    let result = if route == "delete_active_selection" {
        runtime.delete_active_selection()
    } else {
        runtime.replace_text_in_focused_range(range, text)
    };
    trace_input(
        "platform_text_replacement.end",
        format_args!("route={route} result={result:?}"),
    );
    result
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

pub(in crate::gui::app) fn table_menu_input_target_allows(
    registered: Option<GuiPlatformInputTarget>,
    block_id: BlockId,
) -> bool {
    registered.is_some_and(|target| target.is_table_menu_query_for(block_id))
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

pub(super) fn ime_replacement_range(
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

pub(super) fn is_empty_line_ai_platform_input(
    range_utf16: Option<&Range<usize>>,
    text: &str,
) -> bool {
    text == " " && range_utf16.is_none_or(|range| range.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{apply_platform_text_replacement, is_empty_line_ai_platform_input};
    use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
    use cditor_runtime::DocumentRuntime;

    #[test]
    fn platform_space_commit_is_recognized_without_replacing_text() {
        assert!(is_empty_line_ai_platform_input(None, " "));
        assert!(is_empty_line_ai_platform_input(Some(&(0..0)), " "));
        assert!(!is_empty_line_ai_platform_input(Some(&(0..1)), " "));
        assert!(!is_empty_line_ai_platform_input(None, "x"));
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
        runtime.set_document_text_selection(1, 1, 3, 1).unwrap();

        assert!(apply_platform_text_replacement(&mut runtime, Some(1..1), "").unwrap());
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
        runtime.set_document_text_selection(1, 1, 1, 3).unwrap();

        assert!(apply_platform_text_replacement(&mut runtime, Some(1..3), "").unwrap());
        assert_eq!(runtime.focused_text(), Some("ad"));
        assert_eq!(runtime.caret_offset_for_block(1), Some(1));
        assert!(!runtime.has_active_selection());
    }
}
