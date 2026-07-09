use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgba};

use cditor_core::ids::BlockId;
use cditor_runtime::EditorViewProjection;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionOverlayFragment {
    pub block_id: BlockId,
    pub y: f64,
    pub height: f64,
    pub full_block: bool,
}

pub fn selection_overlay_fragments(
    projection: &EditorViewProjection,
) -> Vec<SelectionOverlayFragment> {
    let mut fragments = Vec::new();
    let mut block_y = projection.before_window_height;
    for block in &projection.blocks {
        let height = block.layout.effective_height();
        if block.selected {
            fragments.push(SelectionOverlayFragment {
                block_id: block.block_id,
                y: block_y,
                height,
                full_block: true,
            });
        }
        block_y += height;
    }
    fragments
}

pub fn render_selection_overlay(fragments: &[SelectionOverlayFragment]) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .children(fragments.iter().map(|fragment| {
            div()
                .absolute()
                .left(px(0.0))
                .right_0()
                .top(px(fragment.y as f32))
                .h(px(fragment.height as f32))
                .bg(rgba(0x0969da33))
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn selection_overlay_uses_projection_fragments_not_entities() {
        let mut runtime = DocumentRuntime::demo();
        runtime.select_all_visible_blocks();
        let projection = runtime.projection_for_window();

        let fragments = selection_overlay_fragments(&projection);

        assert_eq!(fragments.len(), projection.blocks.len());
        assert!(fragments.iter().all(|fragment| fragment.full_block));
    }
}
