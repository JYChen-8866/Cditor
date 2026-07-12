use std::collections::HashMap;

use gpui::{
    AnyElement, App, Entity, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, div, prelude::FluentBuilder, px,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::app::cditor_v2_view::TableScrollSnapshot;
use crate::gui::block::{
    BlockActionState, BlockDragOverlaySnapshot, BlockView, MermaidRenderCache, TableAxis,
    TableAxisSelection, TableCellRangeSelection, TableReorderPreview, TableResizePreview,
    WhiteboardThumbnailCache, render_block_drag_overlay, render_table_axis_overlays,
    render_table_axis_toolbar, render_table_resize_overlays, table_axis_track_sizes,
    table_content_editor_origin, table_toolbar_editor_origin,
};
use crate::gui::document::DocumentSurface;
use crate::gui::input::CodeLanguageEditState;
use crate::gui::overlay::render_editor_overlays;
use crate::gui::overlay::table::{
    render_table_horizontal_scrollbar, render_table_reorder_preview_overlay,
};
use cditor_core::ids::BlockId;
use cditor_runtime::EditorViewProjection;

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

fn document_block_top(before_window_height: f64, window_local_top: f64) -> f64 {
    before_window_height + window_local_top
}

impl DocumentEditorView {
    pub fn new(theme: GuiTheme) -> Self {
        Self { theme }
    }

    pub(crate) fn render(
        &self,
        projection: &EditorViewProjection,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        code_language_focus: FocusHandle,
        hovered_block_id: Option<BlockId>,
        drag_overlay: Option<BlockDragOverlaySnapshot>,
        action: DocumentBlockActionProjection,
        table_axis_selection: Option<TableAxisSelection>,
        image_resize_preview: Option<(BlockId, f32)>,
        table_resize_preview: Option<TableResizePreview>,
        table_reorder_preview: Option<TableReorderPreview>,
        table_range_selection: Option<TableCellRangeSelection>,
        code_language_edit: Option<&CodeLanguageEditState>,
        table_scroll_snapshots: &HashMap<BlockId, TableScrollSnapshot>,
        mermaid_renders: &MermaidRenderCache,
        mermaid_source_blocks: &std::collections::HashSet<BlockId>,
        whiteboard_thumbnails: &WhiteboardThumbnailCache,
        cx: &mut App,
    ) -> AnyElement {
        let block_view = BlockView::new(self.theme);
        let mut block_y = 0.0;
        let mut table_overlay_elements = Vec::new();
        let mut block_elements = projection
            .blocks
            .iter()
            .map(|block| {
                let top = block_y;
                let document_top = document_block_top(projection.before_window_height, top);
                let height = block.layout.effective_height();
                block_y += height;
                if let Some(table_view) = &block.table_view {
                    let content_origin =
                        table_content_editor_origin(block, document_top as f32, self.theme);
                    let grid_origin =
                        table_toolbar_editor_origin(block, document_top as f32, self.theme);
                    let row_track_sizes = table_axis_track_sizes(table_view, TableAxis::Row);
                    let column_track_sizes = table_axis_track_sizes(table_view, TableAxis::Column);
                    // Scrollbar first so that axis overlays (gutters) render on top.
                    if let Some(scroll_snapshot) = table_scroll_snapshots.get(&block.block_id)
                        && let Some(measurement) = scroll_snapshot.viewport_measurement
                        && let Some(scrollbar) = render_table_horizontal_scrollbar(
                            block.block_id,
                            table_view,
                            grid_origin,
                            measurement,
                            scroll_snapshot.offset_x,
                            0.0,
                            self.theme,
                            view.clone(),
                        )
                    {
                        table_overlay_elements.push(scrollbar);
                    }
                    table_overlay_elements.extend(render_table_axis_overlays(
                        block.block_id,
                        table_view,
                        table_axis_selection,
                        table_range_selection,
                        table_view.focused_cell,
                        &row_track_sizes,
                        &column_track_sizes,
                        content_origin,
                        self.theme,
                        view.clone(),
                    ));
                    table_overlay_elements.extend(render_table_resize_overlays(
                        block.block_id,
                        table_view,
                        content_origin,
                        self.theme,
                        view.clone(),
                    ));
                    if let Some(selection) = table_axis_selection
                        .filter(|selection| selection.block_id == block.block_id)
                    {
                        table_overlay_elements.push(render_table_axis_toolbar(
                            selection,
                            table_view,
                            grid_origin,
                            self.theme,
                            view.clone(),
                        ));
                    }
                    if let Some(reorder_preview) = render_table_reorder_preview_overlay(
                        block.block_id,
                        table_view,
                        grid_origin,
                        table_reorder_preview,
                        self.theme,
                    ) {
                        table_overlay_elements.push(reorder_preview);
                    }
                }
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(top as f32))
                    .h(px(height as f32))
                    .child({
                        let block_action =
                            block_action_state_for_projection(projection, block.block_id, action);
                        let show_hover_gutter =
                            hovered_block_id == Some(block.block_id) && !action.dragging;
                        block_view.render(
                            block,
                            view.clone(),
                            focus.clone(),
                            code_language_focus.clone(),
                            show_hover_gutter,
                            block_action,
                            table_axis_selection
                                .filter(|selection| selection.block_id == block.block_id),
                            image_resize_preview
                                .filter(|(preview_block_id, _)| *preview_block_id == block.block_id)
                                .map(|(_, width)| width),
                            table_resize_preview.filter(|(preview_block_id, _, _, _)| {
                                *preview_block_id == block.block_id
                            }),
                            table_reorder_preview.filter(|(preview_block_id, _, _, _)| {
                                *preview_block_id == block.block_id
                            }),
                            table_range_selection
                                .filter(|selection| selection.block_id == block.block_id),
                            code_language_edit,
                            table_scroll_snapshots
                                .get(&block.block_id)
                                .map(|snapshot| snapshot.handle.clone()),
                            mermaid_renders,
                            mermaid_source_blocks.contains(&block.block_id),
                            whiteboard_thumbnails,
                            cx,
                        )
                    })
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        if projection
            .blocks
            .last()
            .is_some_and(|block| block.visible_index + 1 == projection.total_visible_blocks)
        {
            block_elements.push(render_down_placer(
                block_y,
                projection.down_placer_height,
                self.theme,
                view.clone(),
            ));
            block_y += projection.down_placer_height;
        }
        block_elements.push(div().h(px(block_y as f32)).into_any_element());

        let overlay = div()
            .absolute()
            .left_0()
            .right_0()
            .top_0()
            .child(render_editor_overlays(projection, self.theme))
            .children(table_overlay_elements)
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

fn render_down_placer(
    top: f64,
    height: f64,
    _theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    div()
        .id("cditor-down-placer")
        .absolute()
        .left_0()
        .right_0()
        .top(px(top as f32))
        .h(px(height as f32))
        .cursor_text()
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.focus_down_placer_from_gui(window, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

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

    #[test]
    fn overlay_block_top_includes_virtual_window_prefix_height() {
        assert_eq!(document_block_top(8_000.0, 128.0), 8_128.0);
    }
}
