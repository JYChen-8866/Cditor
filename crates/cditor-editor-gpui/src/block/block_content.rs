use gpui::{
    AnyElement, App, Entity, FocusHandle, IntoElement, ParentElement, ScrollHandle, Styled, div, px,
};

use crate::app::worker_admission::EditorWorkerAdmission;
use crate::block::placeholder::{
    render_empty_ai_hint, render_error, render_loading, render_placeholder,
};
use crate::document::DocumentTextViewport;
use crate::editor_view::CditorV2View;
use crate::features::code::highlight::{CodeHighlightCache, code_theme_item};
use crate::features::media::render_image_block;
use crate::features::table::render_table_block;
use crate::features::table::{
    TableAxisSelection, TableCellRangeSelection, TableCellSelection, TableReorderPreview,
    TableResizePreview,
};
use crate::features::text::collection::render_collection_block;
use crate::features::whiteboard::WhiteboardThumbnailCache;
#[cfg(feature = "whiteboard")]
use crate::features::whiteboard::render_whiteboard_thumbnail;
use crate::surfaces::TextSurfaceRenderState;
use crate::text::{
    RichTextElement, RichTextLayoutInput, SegmentedRichTextElement, SegmentedTextViewport,
};
use crate::{presentation::rich_text::render_payload_text, theme::GuiTheme};
use cditor_core::edit::SelectionRange;
use cditor_core::rich_text::{BlockPayload, BlockPayloadView};
use cditor_runtime::ViewBlockSnapshot;

const EMPTY_PAGE_TITLE_PLACEHOLDER: &str = "新页面";

#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
pub(crate) fn render_block_content(
    block: &ViewBlockSnapshot,
    text_layout_width_px: f64,
    text_viewport: DocumentTextViewport,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    image_caption_state: Option<TextSurfaceRenderState>,
    workers: &EditorWorkerAdmission,
    image_resize_preview_width_px: Option<f32>,
    table_resize_preview: Option<TableResizePreview>,
    table_reorder_preview: Option<TableReorderPreview>,
    table_range_selection: Option<TableCellRangeSelection>,
    table_cell_selection: Option<TableCellSelection>,
    suppress_text_input: bool,
    table_selection: Option<TableAxisSelection>,
    table_scroll_handle: Option<ScrollHandle>,
    code_scroll_handle: Option<ScrollHandle>,
    code_caret_reveal_after_line_break: bool,
    code_highlights: &CodeHighlightCache,
    code_highlight_theme: &'static str,
    whiteboard_thumbnails: &WhiteboardThumbnailCache,
    cx: &mut App,
) -> AnyElement {
    #[cfg(not(feature = "whiteboard"))]
    let _ = whiteboard_thumbnails;
    match &block.payload {
        BlockPayloadView::Loaded(payload) => {
            if let Some(table_view) = &block.table_view {
                return render_table_block(
                    block.block_id,
                    payload.content_version,
                    block.layout.layout_version,
                    table_view,
                    theme,
                    block.marked_range.clone(),
                    table_selection,
                    table_range_selection,
                    table_cell_selection,
                    table_resize_preview,
                    table_reorder_preview,
                    table_scroll_handle,
                    view.clone(),
                    focus.clone(),
                );
            }
            if let BlockPayload::Table(_table) = &payload.payload {
                return crate::presentation::rich_text::render_payload_text(payload, theme);
            }
            if let BlockPayload::Image(image) = &payload.payload {
                return render_image_block(
                    block.block_id,
                    payload.content_version,
                    block.layout.layout_version,
                    image,
                    theme,
                    image_caption_state,
                    workers,
                    view,
                    focus,
                    image_resize_preview_width_px,
                    cx,
                );
            }
            if let BlockPayload::Collection(collection) = &payload.payload {
                return render_collection_block(
                    block.block_id,
                    block.layout.layout_version,
                    collection,
                    theme,
                    view,
                    focus,
                    cx,
                );
            }
            #[cfg(feature = "whiteboard")]
            if matches!(payload.payload, BlockPayload::Whiteboard(_)) {
                return render_whiteboard_thumbnail(
                    block.block_id,
                    whiteboard_thumbnails,
                    theme,
                    view,
                );
            }
            if let Some(mut input) =
                RichTextLayoutInput::from_snapshot(block, text_layout_width_px, 1, 1)
            {
                if matches!(
                    block.kind,
                    cditor_core::rich_text::RichBlockKind::Code { .. }
                ) && block.marked_range.is_none()
                    && let Some(spans) =
                        code_highlights.spans(block.block_id, input.content_version)
                {
                    input.spans = spans;
                }
                let text_len = input.text_len();
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
                let segmented_viewport = Some(SegmentedTextViewport::document(
                    text_viewport.local_top_px,
                    text_viewport.height_px,
                ));
                if cditor_text::requires_segmentation(text_len)
                    && let Some(segmented_viewport) = segmented_viewport
                {
                    return SegmentedRichTextElement::new(
                        input,
                        text_theme,
                        caret_for_text_input(block.caret_offset, suppress_text_input),
                        block
                            .caret_affinity
                            .unwrap_or(cditor_core::edit::TextAffinity::Downstream),
                        block.marked_range.clone(),
                        selection_range,
                        block.attrs.color.as_deref().and_then(parse_block_hex_color),
                        segmented_viewport,
                        crate::text::element::RichTextInputHandler {
                            view,
                            focus,
                            focused: text_input_active(block.focused, suppress_text_input),
                            table_cell_position: None,
                        },
                    )
                    .render();
                }
                let text_element = RichTextElement::new(input, text_theme)
                    .with_prewarmed_layout()
                    .with_placeholder(page_title_placeholder(block, text_len))
                    .with_base_text_color(
                        block.attrs.color.as_deref().and_then(parse_block_hex_color),
                    )
                    .with_caret(caret_for_text_input(
                        block.caret_offset,
                        suppress_text_input,
                    ))
                    .with_caret_affinity(
                        block
                            .caret_affinity
                            .unwrap_or(cditor_core::edit::TextAffinity::Downstream),
                    )
                    .with_marked_range(block.marked_range.clone())
                    .with_selection_range(selection_range)
                    .with_caret_reveal_scroll_handle(
                        (code_caret_reveal_after_line_break
                            && matches!(
                                block.kind,
                                cditor_core::rich_text::RichBlockKind::Code { .. }
                            ))
                        .then_some(code_scroll_handle)
                        .flatten(),
                    )
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

pub(crate) fn parse_block_hex_color(value: &str) -> Option<u32> {
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
        && page_title_placeholder(block, text_len).is_none()
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

fn page_title_placeholder(block: &ViewBlockSnapshot, text_len: usize) -> Option<&'static str> {
    (text_len == 0
        && block.visible_index == 0
        && block.depth == 0
        && matches!(
            block.kind,
            cditor_core::rich_text::RichBlockKind::Heading { level: 1 }
        ))
    .then_some(EMPTY_PAGE_TITLE_PLACEHOLDER)
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

        assert!(RichTextLayoutInput::from_snapshot(block, 718.0, 1, 1).is_some());
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
    fn empty_first_h1_uses_page_title_placeholder_instead_of_ai_hint() {
        let runtime = DocumentRuntime::empty();
        let mut block = runtime.projection_for_window().blocks[0].clone();
        block.focused = true;

        assert_eq!(
            page_title_placeholder(&block, 0),
            Some(EMPTY_PAGE_TITLE_PLACEHOLDER)
        );
        assert!(!should_show_empty_ai_hint(&block, false, 0));
        assert_eq!(page_title_placeholder(&block, 1), None);

        let mut non_title_h1 = block;
        non_title_h1.visible_index = 1;
        assert_eq!(page_title_placeholder(&non_title_h1, 0), None);
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
