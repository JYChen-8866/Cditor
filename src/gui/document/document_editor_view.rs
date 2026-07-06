use gpui::{
    AnyElement, App, Entity, FocusHandle, IntoElement, ParentElement, Styled, div,
    prelude::FluentBuilder, px,
};

use crate::core::ids::BlockId;
use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::{
    BlockActionState, BlockDragOverlaySnapshot, BlockView, render_block_drag_overlay,
};
use crate::gui::document::DocumentSurface;
use crate::gui::overlay::render_editor_overlays;
use crate::runtime::EditorViewProjection;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentEditorView {
    pub theme: GuiTheme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocumentBlockActionProjection {
    pub action_block_id: Option<BlockId>,
    pub dragging: bool,
}

fn block_action_state_for_projection(
    projection: &EditorViewProjection,
    block_id: BlockId,
    action: DocumentBlockActionProjection,
) -> BlockActionState {
    let Some(action_block_id) = action.action_block_id else {
        return BlockActionState::default();
    };
    let Some(source) = projection
        .blocks
        .iter()
        .find(|block| block.block_id == action_block_id)
    else {
        return BlockActionState::default();
    };
    let Some(block) = projection
        .blocks
        .iter()
        .find(|block| block.block_id == block_id)
    else {
        return BlockActionState::default();
    };
    let source_depth = source.chrome.list_info.depth;
    let source_visible_index = source.visible_index;
    let source_subtree_end = projection
        .blocks
        .iter()
        .filter(|candidate| candidate.visible_index > source_visible_index)
        .find(|candidate| candidate.chrome.list_info.depth <= source_depth)
        .map(|candidate| candidate.visible_index)
        .unwrap_or_else(|| {
            projection
                .blocks
                .last()
                .map(|candidate| candidate.visible_index + 1)
                .unwrap_or(source_visible_index + 1)
        });
    let action_active =
        block.visible_index >= source_visible_index && block.visible_index < source_subtree_end;
    BlockActionState {
        action_active,
        action_root: block_id == action_block_id,
        dragging: action.dragging && action_active,
    }
}

impl DocumentEditorView {
    pub fn new(theme: GuiTheme) -> Self {
        Self { theme }
    }

    pub fn render(
        &self,
        projection: &EditorViewProjection,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        hovered_block_id: Option<BlockId>,
        drag_overlay: Option<BlockDragOverlaySnapshot>,
        action: DocumentBlockActionProjection,
        image_resize_preview: Option<(BlockId, f32)>,
        cx: &mut App,
    ) -> AnyElement {
        let block_view = BlockView::new(self.theme);
        let mut block_y = 0.0;
        let mut block_elements = projection
            .blocks
            .iter()
            .map(|block| {
                let top = block_y;
                let height = block.layout.effective_height();
                block_y += height;
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(top as f32))
                    .h(px(height as f32))
                    .child(
                        block_view.render(
                            block,
                            view.clone(),
                            focus.clone(),
                            hovered_block_id == Some(block.block_id),
                            block_action_state_for_projection(projection, block.block_id, action),
                            image_resize_preview
                                .filter(|(preview_block_id, _)| *preview_block_id == block.block_id)
                                .map(|(_, width)| width),
                            cx,
                        ),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        block_elements.push(div().h(px(block_y as f32)).into_any_element());

        let overlay = div()
            .absolute()
            .left_0()
            .right_0()
            .top_0()
            .child(render_editor_overlays(projection, self.theme))
            .when_some(drag_overlay, |this, overlay| {
                this.child(render_block_drag_overlay(overlay, self.theme))
            })
            .into_any_element();
        DocumentSurface::with_scroll(
            projection.before_window_height,
            projection.placeholder_window_height,
            projection.after_window_height,
            projection.scroll.global_scroll_top,
        )
        .render(self.theme, block_elements, Some(overlay))
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn document_editor_view_can_project_demo_blocks() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let editor = DocumentEditorView::new(GuiTheme::light());

        assert!(!projection.blocks.is_empty());
        assert_eq!(editor.theme, GuiTheme::light());
    }

    #[test]
    fn action_projection_marks_source_subtree_without_mutating_runtime_projection() {
        let runtime = DocumentRuntime::demo();
        let mut projection = runtime.projection_for_window();
        assert!(projection.blocks.len() >= 3);
        projection.blocks.truncate(3);
        projection.blocks[0].visible_index = 10;
        projection.blocks[0].chrome.list_info.depth = 0;
        projection.blocks[1].visible_index = 11;
        projection.blocks[1].chrome.list_info.depth = 1;
        projection.blocks[2].visible_index = 12;
        projection.blocks[2].chrome.list_info.depth = 0;
        let source = projection.blocks[0].block_id;
        let child = projection.blocks[1].block_id;
        let next_root = projection.blocks[2].block_id;
        let action = DocumentBlockActionProjection {
            action_block_id: Some(source),
            dragging: true,
        };

        let source_state = block_action_state_for_projection(&projection, source, action);
        let child_state = block_action_state_for_projection(&projection, child, action);
        let next_root_state = block_action_state_for_projection(&projection, next_root, action);

        assert!(source_state.action_active);
        assert!(source_state.action_root);
        assert!(source_state.dragging);
        assert!(child_state.action_active);
        assert!(!child_state.action_root);
        assert!(child_state.dragging);
        assert!(!next_root_state.action_active);
        assert!(!next_root_state.action_root);
        assert!(!next_root_state.dragging);
    }
}
