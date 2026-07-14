use gpui::{
    AnyElement, App, Entity, FocusHandle, IntoElement, ParentElement, ScrollHandle, Styled, div, px,
};

use crate::gui::app::CditorV2View;
use crate::gui::block::code::highlight::code_theme_item;
use crate::gui::block::media::render_image_block;
use crate::gui::block::placeholder::{
    render_empty_ai_hint, render_error, render_loading, render_placeholder,
};
use crate::gui::block::table::render_table_block;
use crate::gui::block::table::{
    TableAxisSelection, TableCellRangeSelection, TableReorderPreview, TableResizePreview,
};
use crate::gui::block::{
    CodeHighlightCache, WhiteboardThumbnailCache, render_whiteboard_thumbnail,
};
use crate::gui::document::DEFAULT_DOCUMENT_CONTENT_WIDTH_PX;
use crate::gui::text::{RichTextElement, RichTextLayoutInput};
use crate::gui::{GuiTheme, rich_text::render_payload_text};
use cditor_core::edit::SelectionRange;
use cditor_core::rich_text::{BlockPayload, BlockPayloadView};
use cditor_runtime::ViewBlockSnapshot;

pub(crate) fn render_block_content(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    image_resize_preview_width_px: Option<f32>,
    table_resize_preview: Option<TableResizePreview>,
    table_reorder_preview: Option<TableReorderPreview>,
    table_range_selection: Option<TableCellRangeSelection>,
    suppress_text_input: bool,
    table_selection: Option<TableAxisSelection>,
    table_scroll_handle: Option<ScrollHandle>,
    code_highlights: &CodeHighlightCache,
    code_highlight_theme: &'static str,
    whiteboard_thumbnails: &WhiteboardThumbnailCache,
    cx: &mut App,
) -> AnyElement {
    match &block.payload {
        BlockPayloadView::Loaded(payload) => {
            if let Some(table_view) = &block.table_view {
                return render_table_block(
                    block.block_id,
                    payload.content_version,
                    table_view,
                    theme,
                    block.marked_range.clone(),
                    table_selection,
                    table_range_selection,
                    table_resize_preview,
                    table_reorder_preview,
                    table_scroll_handle,
                    view.clone(),
                    focus.clone(),
                );
            }
            if let BlockPayload::Table(_table) = &payload.payload {
                return crate::gui::rich_text::render_payload_text(payload, theme);
            }
            if let BlockPayload::Image(image) = &payload.payload {
                return render_image_block(
                    block.block_id,
                    payload.content_version,
                    image,
                    theme,
                    view,
                    image_resize_preview_width_px,
                    cx,
                );
            }
            if matches!(payload.payload, BlockPayload::Whiteboard(_)) {
                return render_whiteboard_thumbnail(
                    block.block_id,
                    whiteboard_thumbnails,
                    theme,
                    view,
                );
            }
            if let Some(mut input) = RichTextLayoutInput::from_snapshot(
                block,
                f64::from(DEFAULT_DOCUMENT_CONTENT_WIDTH_PX),
                1,
                1,
            ) {
                if matches!(
                    block.kind,
                    cditor_core::rich_text::RichBlockKind::Code { .. }
                ) && let Some(spans) = code_highlights.spans(block.block_id)
                {
                    input.spans = spans.to_vec();
                }
                let text_len = input.spans.iter().map(|span| span.text.len()).sum();
                let selection_range = if block.selection_overlay {
                    None
                } else {
                    text_selection_range(&block.selection_range, text_len)
                };
                let text_theme = if matches!(
                    block.kind,
                    cditor_core::rich_text::RichBlockKind::Code { .. }
                ) {
                    GuiTheme {
                        code_text: code_theme_item(code_highlight_theme).foreground,
                        ..theme
                    }
                } else {
                    theme
                };
                let text_element = RichTextElement::new(input, text_theme)
                    .with_base_text_color(
                        block.attrs.color.as_deref().and_then(parse_block_hex_color),
                    )
                    .with_caret(caret_for_text_input(
                        block.caret_offset,
                        suppress_text_input,
                    ))
                    .with_marked_range(block.marked_range.clone())
                    .with_selection_range(selection_range)
                    .with_input_handler(
                        view,
                        focus,
                        text_input_active(block.focused, suppress_text_input),
                    )
                    .render();
                if should_show_empty_ai_hint(block, suppress_text_input, text_len) {
                    div()
                        .relative()
                        .w_full()
                        .min_h(px(24.0))
                        .child(text_element)
                        .child(render_empty_ai_hint(&block.kind, theme))
                        .into_any_element()
                } else {
                    text_element
                }
            } else {
                render_payload_text(payload, theme)
            }
        }
        BlockPayloadView::Placeholder { .. } => render_placeholder(block, theme),
        BlockPayloadView::Loading { .. } => render_loading(block, theme),
        BlockPayloadView::Error { message } => render_error(message, theme),
    }
}

fn parse_block_hex_color(value: &str) -> Option<u32> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

fn should_show_empty_ai_hint(
    block: &ViewBlockSnapshot,
    suppress_text_input: bool,
    text_len: usize,
) -> bool {
    block.focused
        && !suppress_text_input
        && text_len == 0
        && matches!(
            block.kind,
            cditor_core::rich_text::RichBlockKind::Paragraph
                | cditor_core::rich_text::RichBlockKind::Heading { .. }
                | cditor_core::rich_text::RichBlockKind::Quote
                | cditor_core::rich_text::RichBlockKind::Todo { .. }
                | cditor_core::rich_text::RichBlockKind::BulletedList
                | cditor_core::rich_text::RichBlockKind::NumberedList
                | cditor_core::rich_text::RichBlockKind::Toggle
                | cditor_core::rich_text::RichBlockKind::Callout { .. }
        )
}

fn text_input_active(block_focused: bool, suppress_text_input: bool) -> bool {
    block_focused && !suppress_text_input
}

fn caret_for_text_input(caret_offset: Option<usize>, suppress_text_input: bool) -> Option<usize> {
    (!suppress_text_input).then_some(caret_offset).flatten()
}

fn text_selection_range(
    selection: &Option<SelectionRange>,
    text_len: usize,
) -> Option<std::ops::Range<usize>> {
    match selection {
        Some(SelectionRange::Partial(range)) => Some(range.clone()),
        Some(SelectionRange::Full) => Some(0..text_len),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn block_content_accepts_loaded_demo_payload() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let block = projection
            .blocks
            .iter()
            .find(|block| {
                matches!(
                    block.payload,
                    cditor_core::rich_text::BlockPayloadView::Loaded(_)
                )
            })
            .unwrap();

        assert!(
            RichTextLayoutInput::from_snapshot(
                block,
                f64::from(DEFAULT_DOCUMENT_CONTENT_WIDTH_PX),
                1,
                1,
            )
            .is_some()
        );
    }

    #[test]
    fn ai_hint_only_appears_for_focused_empty_paragraphs() {
        let runtime = DocumentRuntime::from_payloads(
            1,
            vec![cditor_core::rich_text::BlockPayloadRecord::rich_text(
                1,
                cditor_core::rich_text::RichBlockKind::Paragraph,
                "",
            )],
            720.0,
        );
        let mut block = runtime.projection_for_window().blocks[0].clone();
        block.focused = true;
        assert!(should_show_empty_ai_hint(&block, false, 0));
        assert!(!should_show_empty_ai_hint(&block, false, 1));
        assert!(!should_show_empty_ai_hint(&block, true, 0));
        block.focused = false;
        assert!(!should_show_empty_ai_hint(&block, false, 0));
    }

    #[test]
    fn code_language_input_suppresses_body_text_input() {
        assert!(text_input_active(true, false));
        assert!(!text_input_active(true, true));
        assert_eq!(caret_for_text_input(Some(3), false), Some(3));
        assert_eq!(caret_for_text_input(Some(3), true), None);
    }

    #[test]
    fn full_document_text_fragment_selects_only_the_block_text_range() {
        assert_eq!(
            text_selection_range(&Some(SelectionRange::Full), 6),
            Some(0..6)
        );
        assert_eq!(
            text_selection_range(&Some(SelectionRange::Partial(2..4)), 6),
            Some(2..4)
        );
        assert_eq!(text_selection_range(&None, 6), None);
    }
}
