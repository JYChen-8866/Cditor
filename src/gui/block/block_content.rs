use gpui::{AnyElement, App, Entity, FocusHandle};

use crate::core::edit::SelectionRange;
use crate::core::rich_text::{BlockPayload, BlockPayloadView};
use crate::gui::app::CditorV2View;
use crate::gui::block::media::render_image_block;
use crate::gui::block::placeholder::{render_error, render_loading, render_placeholder};
use crate::gui::text::{RichTextElement, RichTextLayoutInput};
use crate::gui::{GuiTheme, rich_text::render_payload_text};
use crate::runtime::ViewBlockSnapshot;

pub fn render_block_content(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    image_resize_preview_width_px: Option<f32>,
    cx: &mut App,
) -> AnyElement {
    match &block.payload {
        BlockPayloadView::Loaded(payload) => {
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
                    .with_caret(block.caret_offset)
                    .with_marked_range(block.marked_range.clone())
                    .with_selection_range(selection_range)
                    .with_input_handler(view, focus, block.focused)
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

#[cfg(test)]
mod tests {
    use crate::runtime::DocumentRuntime;

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
                    crate::core::rich_text::BlockPayloadView::Loaded(_)
                )
            })
            .unwrap();

        assert!(RichTextLayoutInput::from_snapshot(block, 860.0, 1, 1).is_some());
    }
}
