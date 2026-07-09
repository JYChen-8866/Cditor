use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::text::{RichTextElement, RichTextLayoutInput};
use cditor_core::ids::BlockId;
use cditor_runtime::{EditorViewProjection, ViewBlockSnapshot};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretOverlayRect {
    pub block_id: BlockId,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn caret_overlay_rects(
    projection: &EditorViewProjection,
    theme: GuiTheme,
) -> Vec<CaretOverlayRect> {
    let mut rects = Vec::new();
    let mut block_y = projection.before_window_height;
    for block in &projection.blocks {
        if block.focused
            && let Some(rect) = caret_rect_for_block(block, theme, block_y)
        {
            rects.push(rect);
        }
        block_y += block.layout.effective_height();
    }
    rects
}

pub fn render_caret_overlay(rects: &[CaretOverlayRect], theme: GuiTheme) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .children(rects.iter().map(|rect| {
            div()
                .absolute()
                .left(px(rect.x as f32))
                .top(px(rect.y as f32))
                .w(px(rect.width as f32))
                .h(px(rect.height as f32))
                .bg(rgb(theme.focused))
        }))
        .into_any_element()
}

fn caret_rect_for_block(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    block_y: f64,
) -> Option<CaretOverlayRect> {
    let caret_offset = block.caret_offset?;
    if let Some(input) = RichTextLayoutInput::from_snapshot(block, 860.0, 1, 1) {
        let element = RichTextElement::new(input, theme).with_caret(Some(caret_offset));
        let rect = element.candidate_rect_for_caret()?;
        return Some(CaretOverlayRect {
            block_id: block.block_id,
            x: rect.x,
            y: block_y + rect.y,
            width: rect.width,
            height: rect.height,
        });
    }

    Some(CaretOverlayRect {
        block_id: block.block_id,
        x: 0.0,
        y: block_y,
        width: 1.0,
        height: 24.0,
    })
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn caret_overlay_projects_focused_block_without_ui_entity() {
        let mut runtime = DocumentRuntime::demo();
        let block_id = runtime.projection_for_window().blocks[0].block_id;
        runtime.focus_block_at_offset(block_id, 1).unwrap();
        let projection = runtime.projection_for_window();

        let rects = caret_overlay_rects(&projection, GuiTheme::light());

        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].block_id, block_id);
        assert!(rects[0].height > 0.0);
    }
}
