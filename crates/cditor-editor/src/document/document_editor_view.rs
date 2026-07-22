use std::collections::HashMap;

use gpui::{
    AnyElement, App, Entity, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled, div, prelude::FluentBuilder, px,
};

use crate::app::CditorV2View;
use crate::app::cditor_v2_view::TableScrollSnapshot;
use crate::block::table::menu::TableMenuUiState;
use crate::block::{
    BlockActionState, BlockDragOverlaySnapshot, BlockView, CodeHighlightCache, MermaidRenderCache,
    TableAxis, TableAxisSelection, TableCellRangeSelection, TableCellSelection,
    TableChromeOverlays, TableReorderPreview, TableResizePreview, WhiteboardThumbnailCache,
    render_block_drag_overlay, render_table_axis_overlays, render_table_axis_toolbar,
    render_table_cell_menu, render_table_chrome_viewport, render_table_resize_overlays,
    table_axis_track_sizes, table_chrome_viewport_origins, table_content_editor_origin,
    table_toolbar_editor_origin,
};
use crate::document::DocumentSurface;
use crate::document::{DEFAULT_DOCUMENT_CONTENT_WIDTH_PX, DEFAULT_DOCUMENT_TOP_INSET_PX};
use crate::input::CodeLanguageEditState;
use crate::menu_metrics::MenuViewportBounds;
use crate::overlay::render_editor_overlays;
use crate::overlay::table::{
    render_table_horizontal_scrollbar, render_table_reorder_preview_overlay,
};
use crate::theme::GuiTheme;
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

impl DocumentEditorView {
    pub fn new(theme: GuiTheme) -> Self {
        Self { theme }
    }

    #[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
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
        table_axis_menu_selection: Option<TableAxisSelection>,
        table_cell_selection: Option<TableCellSelection>,
        table_menu_ui: &TableMenuUiState,
        editor_viewport_width_px: f32,
        editor_viewport_height_px: f32,
        readonly: bool,
        image_resize_preview: Option<(BlockId, f32)>,
        table_resize_preview: Option<TableResizePreview>,
        table_reorder_preview: Option<TableReorderPreview>,
        table_range_selection: Option<TableCellRangeSelection>,
        code_language_edit: Option<&CodeLanguageEditState>,
        code_theme_menu_block_id: Option<BlockId>,
        code_highlight_theme: &'static str,
        suppress_document_text_input: bool,
        table_scroll_snapshots: &HashMap<BlockId, TableScrollSnapshot>,
        code_highlights: &CodeHighlightCache,
        mermaid_renders: &MermaidRenderCache,
        mermaid_source_blocks: &std::collections::HashSet<BlockId>,
        whiteboard_thumbnails: &WhiteboardThumbnailCache,
        cx: &mut App,
    ) -> AnyElement {
        let block_view = BlockView::new(self.theme);
        let menu_viewport = document_overlay_menu_viewport(
            editor_viewport_width_px,
            editor_viewport_height_px,
            projection.scroll.global_scroll_top,
            projection.before_window_height,
        );
        let mut block_y = 0.0;
        let mut table_overlay_elements = Vec::new();
        let mut block_elements = projection
            .blocks
            .iter()
            .map(|block| {
                let top = block_y;
                let height = block.layout.effective_height();
                block_y += height;
                if let Some(table_view) = &block.table_view {
                    let local_top = top as f32;
                    let content_origin = table_content_editor_origin(block, local_top, self.theme);
                    let grid_origin = table_toolbar_editor_origin(block, local_top, self.theme);
                    let row_track_sizes = table_axis_track_sizes(table_view, TableAxis::Row);
                    let column_track_sizes = table_axis_track_sizes(table_view, TableAxis::Column);
                    let scroll_snapshot = table_scroll_snapshots.get(&block.block_id);
                    let viewport_width_px = scroll_snapshot
                        .and_then(|snapshot| snapshot.viewport_measurement)
                        .map(|measurement| measurement.viewport_width_px)
                        .unwrap_or(table_view.width_px);
                    if let Some(scroll_snapshot) = scroll_snapshot
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
                    let chrome_origins = table_chrome_viewport_origins();
                    let mut table_chrome = TableChromeOverlays {
                        viewport: render_table_resize_overlays(
                            block.block_id,
                            table_view,
                            chrome_origins.viewport,
                            self.theme,
                            view.clone(),
                        ),
                        ..Default::default()
                    };
                    let axis_chrome = render_table_axis_overlays(
                        block.block_id,
                        table_view,
                        table_axis_selection,
                        table_range_selection,
                        table_view.focused_cell,
                        table_cell_selection,
                        &row_track_sizes,
                        &column_track_sizes,
                        chrome_origins,
                        self.theme,
                        view.clone(),
                    );
                    table_chrome.viewport.extend(axis_chrome.viewport);
                    table_chrome.top_edge.extend(axis_chrome.top_edge);
                    table_chrome.left_edge.extend(axis_chrome.left_edge);
                    table_chrome.right_edge.extend(axis_chrome.right_edge);
                    table_overlay_elements.push(render_table_chrome_viewport(
                        content_origin,
                        viewport_width_px,
                        table_view.height_px,
                        table_chrome,
                    ));
                    if let Some(selection) = table_axis_menu_selection
                        .filter(|selection| selection.block_id == block.block_id)
                    {
                        table_overlay_elements.push(render_table_axis_toolbar(
                            selection,
                            table_view,
                            grid_origin,
                            table_menu_ui,
                            readonly,
                            self.theme,
                            view.clone(),
                            focus.clone(),
                            menu_viewport,
                        ));
                    }
                    if let Some(selection) = table_cell_selection
                        .filter(|selection| selection.block_id == block.block_id)
                        && let Some(menu) = render_table_cell_menu(
                            selection,
                            table_view,
                            content_origin,
                            menu_viewport,
                            table_menu_ui,
                            readonly,
                            self.theme,
                            view.clone(),
                        )
                    {
                        table_overlay_elements.push(menu);
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
                            code_theme_menu_block_id == Some(block.block_id),
                            code_highlight_theme,
                            suppress_document_text_input,
                            table_scroll_snapshots
                                .get(&block.block_id)
                                .map(|snapshot| snapshot.handle.clone()),
                            code_highlights,
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
        let retry_range = projection.render_window.block_range.clone();
        let retry_view = view.clone();
        let on_placeholder_retry = projection.placeholder_window_failure.as_ref().map(|_| {
            Box::new(
                move |_event: &gpui::MouseDownEvent, _window: &mut gpui::Window, cx: &mut App| {
                    retry_view.update(cx, |view, cx| {
                        view.retry_payload_window(retry_range.clone(), cx);
                    });
                    cx.stop_propagation();
                },
            ) as crate::document::skeleton_window::PlaceholderRetryHandler
        });
        let mut surface = DocumentSurface::with_scroll(
            projection.before_window_height,
            projection.placeholder_window_height,
            projection.after_window_height,
            projection.scroll.global_scroll_top,
        );
        surface.placeholder_window_error = projection.placeholder_window_error.clone();
        surface.placeholder_window_failure = projection.placeholder_window_failure.clone();
        surface.render(
            self.theme,
            block_elements,
            Some(overlay),
            on_placeholder_retry,
        )
    }
}

fn document_overlay_menu_viewport(
    editor_width_px: f32,
    editor_height_px: f32,
    scroll_top: f64,
    window_start_global_y: f64,
) -> MenuViewportBounds {
    let content_left = ((editor_width_px - DEFAULT_DOCUMENT_CONTENT_WIDTH_PX) / 2.0).max(0.0);
    let left = -content_left;
    let top = (scroll_top - window_start_global_y) as f32 - DEFAULT_DOCUMENT_TOP_INSET_PX;
    MenuViewportBounds {
        left,
        top,
        right: left + editor_width_px,
        bottom: top + editor_height_px,
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
            view.update(cx, |view, cx| {
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
    fn menu_viewport_is_expressed_in_centered_document_overlay_coordinates() {
        let viewport = document_overlay_menu_viewport(1_200.0, 800.0, 0.0, 0.0);

        assert_eq!(viewport.left, -170.0);
        assert_eq!(viewport.right, 1_030.0);
        assert_eq!(viewport.top, -32.0);
        assert_eq!(viewport.bottom, 768.0);
    }

    #[test]
    fn menu_viewport_tracks_document_scroll_and_narrow_hosts() {
        let viewport = document_overlay_menu_viewport(700.0, 500.0, 240.0, 0.0);

        assert_eq!(viewport.left, 0.0);
        assert_eq!(viewport.right, 700.0);
        assert_eq!(viewport.top, 208.0);
        assert_eq!(viewport.bottom, 708.0);
    }

    #[test]
    fn menu_viewport_rebases_far_global_scroll_before_f32_conversion() {
        let viewport = document_overlay_menu_viewport(700.0, 500.0, 20_000_240.25, 20_000_000.25);

        assert_eq!(viewport.top, 208.0);
        assert_eq!(viewport.bottom, 708.0);
    }
}
