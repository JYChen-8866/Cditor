use gpui::{AnyElement, App, Entity, FocusHandle};

use crate::gui::app::CditorV2View;
use crate::gui::block::media::render_image_block;
use crate::gui::block::placeholder::{render_error, render_loading, render_placeholder};
use crate::gui::block::table::render_table_block;
use crate::gui::block::table::{
    TableAxisSelection, TableCellRangeSelection, TableReorderPreview, TableResizePreview,
};
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
            if let Some(input) = RichTextLayoutInput::from_snapshot(block, 860.0, 1, 1) {
                let selection_range = match &block.selection_range {
                    Some(SelectionRange::Partial(range)) => Some(range.clone()),
                    _ => None,
                };
                RichTextElement::new(input, theme)
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
                    .render()
            } else {
                render_payload_text(payload, theme)
            }
        }
        BlockPayloadView::Placeholder { .. } => render_placeholder(block, theme),
        BlockPayloadView::Loading { .. } => render_loading(block, theme),
        BlockPayloadView::Error { message } => render_error(message),
    }
}

fn text_input_active(block_focused: bool, suppress_text_input: bool) -> bool {
    block_focused && !suppress_text_input
}

fn caret_for_text_input(caret_offset: Option<usize>, suppress_text_input: bool) -> Option<usize> {
    (!suppress_text_input).then_some(caret_offset).flatten()
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

        assert!(RichTextLayoutInput::from_snapshot(block, 860.0, 1, 1).is_some());
    }

    #[test]
    fn code_language_input_suppresses_body_text_input() {
        assert!(text_input_active(true, false));
        assert!(!text_input_active(true, true));
        assert_eq!(caret_for_text_input(Some(3), false), Some(3));
        assert_eq!(caret_for_text_input(Some(3), true), None);
    }
}
