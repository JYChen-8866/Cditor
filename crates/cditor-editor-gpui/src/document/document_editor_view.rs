use std::collections::HashMap;

use gpui::{
    AnyElement, AnyView, App, Entity, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, ScrollHandle, Styled, div, prelude::FluentBuilder, px,
};

use crate::app::worker_admission::EditorWorkerAdmission;
use crate::block::{
    BlockActionState, BlockDragOverlaySnapshot, BlockView, render_block_drag_overlay,
};
use crate::document::{
    DocumentBlockGeometry, DocumentLayoutMetrics, DocumentSurface, DocumentTextGeometry,
    DocumentTextViewport, PageDecorationSnapshot, render_page_chrome,
};
use crate::editor_view::CditorV2View;
use crate::editor_view::TableScrollSnapshot;
use crate::features::code::highlight::CodeHighlightCache;
use crate::features::mermaid::MermaidRenderCache;
use crate::features::search::SearchDecorationState;
use crate::features::table::menu::TableMenuUiState;
use crate::features::table::{
    TableAxis, TableAxisSelection, TableCellRangeSelection, TableCellSelection,
    TableChromeOverlays, TableReorderPreview, TableResizePreview, render_table_axis_overlays,
    render_table_axis_toolbar, render_table_cell_menu, render_table_chrome_viewport,
    render_table_resize_overlays, table_axis_track_sizes, table_chrome_viewport_origins,
    table_content_editor_origin, table_content_viewport_height_px,
    table_projected_viewport_width_px, table_toolbar_editor_origin,
};
use crate::features::whiteboard::WhiteboardThumbnailCache;
use crate::input::CodeLanguageEditState;
use crate::input::platform_adapter::on_text_activation;
use crate::menu_metrics::MenuViewportBounds;
use crate::overlays::render_editor_overlays;
use crate::overlays::table::{
    TableReorderOverlayViewport, render_table_horizontal_scrollbar,
    render_table_reorder_preview_overlay,
};
use crate::surfaces::TextSurfaceRenderState;
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
        page_decorations: &PageDecorationSnapshot,
        page_chrome_extras: Option<AnyView>,
        show_page_chrome: bool,
        view: Entity<CditorV2View>,
        image_caption_states: &HashMap<BlockId, TextSurfaceRenderState>,
        workers: &EditorWorkerAdmission,
        asset_provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
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
        document_layout: DocumentLayoutMetrics,
        readonly: bool,
        image_resize_preview: Option<(BlockId, f32, f64)>,
        table_resize_preview: Option<TableResizePreview>,
        table_reorder_preview: Option<TableReorderPreview>,
        table_range_selection: Option<TableCellRangeSelection>,
        code_language_edit: Option<&CodeLanguageEditState>,
        code_theme_menu_block_id: Option<BlockId>,
        code_highlight_theme: &'static str,
        suppress_document_text_input: bool,
        table_scroll_snapshots: &HashMap<BlockId, TableScrollSnapshot>,
        code_scroll_handles: &HashMap<BlockId, ScrollHandle>,
        code_caret_reveal_after_line_break: &std::collections::HashSet<BlockId>,
        collapsed_code_blocks: &std::collections::HashSet<BlockId>,
        code_highlights: &CodeHighlightCache,
        search_decorations: &SearchDecorationState,
        mermaid_renders: &MermaidRenderCache,
        mermaid_source_blocks: &std::collections::HashSet<BlockId>,
        whiteboard_thumbnails: &WhiteboardThumbnailCache,
        page_icon_menu_open: bool,
        page_icon_menu_custom_tab: bool,
        page_icon_menu_scroll_handle: ScrollHandle,
        cx: &mut App,
    ) -> AnyElement {
        let block_view = BlockView::new(self.theme);
        let menu_viewport = document_overlay_menu_viewport(
            editor_viewport_width_px,
            editor_viewport_height_px,
            projection.scroll.global_scroll_top,
            projection.before_window_height,
            document_layout,
        );
        let mut block_y = 0.0;
        let mut table_overlay_elements = Vec::new();
        let mut block_elements = projection
            .blocks
            .iter()
            .map(|block| {
                let block_geometry = DocumentBlockGeometry::for_block(block, document_layout);
                let text_geometry =
                    DocumentTextGeometry::for_block(block, self.theme, document_layout);
                let text_layout_width_px = text_geometry.width_px;
                let top = block_y;
                let height = image_resize_preview
                    .filter(|(preview_block_id, _, _)| *preview_block_id == block.block_id)
                    .map(|(_, _, preview_height)| preview_height)
                    .unwrap_or_else(|| block.layout.effective_height());
                if image_resize_preview
                    .is_some_and(|(preview_block_id, _, _)| preview_block_id == block.block_id)
                {
                    crate::diagnostics::image_resize::trace(
                        "projection.preview",
                        format_args!(
                            "block={} visible_index={} top={top:.2} projected_height={:.2} preview_height={height:.2} before={:.2} window_height={:.2} after={:.2} range={:?}",
                            block.block_id,
                            block.visible_index,
                            block.layout.effective_height(),
                            projection.before_window_height,
                            projection.render_window.height(),
                            projection.after_window_height,
                            projection.render_window.block_range,
                        ),
                    );
                }
                block_y += height;
                if block.placeholder
                    && !projection
                        .payload_visible_block_range
                        .contains(&block.visible_index)
                {
                    // Overscan rows whose payload has not arrived reserve their
                    // geometry silently; skeletons are reserved for the visible
                    // core (cold jumps and scrollbar drags).
                    return div()
                        .absolute()
                        .left(px(block_geometry.shell_left_px))
                        .w(px(block_geometry.shell_width_px))
                        .top(px(top as f32))
                        .h(px(height as f32))
                        .into_any_element();
                }
                let text_viewport = DocumentTextViewport::for_block(
                    top,
                    text_geometry.origin_y_px,
                    projection.before_window_height,
                    projection.scroll.global_scroll_top,
                    editor_viewport_height_px,
                    document_layout.top_inset_px,
                );
                if let Some(table_view) = &block.table_view {
                    let local_top = top as f32;
                    let content_origin =
                        table_content_editor_origin(block, local_top, self.theme, document_layout);
                    let grid_origin =
                        table_toolbar_editor_origin(block, local_top, self.theme, document_layout);
                    let row_track_sizes = table_axis_track_sizes(table_view, TableAxis::Row);
                    let column_track_sizes = table_axis_track_sizes(table_view, TableAxis::Column);
                    let scroll_snapshot = table_scroll_snapshots.get(&block.block_id);
                    let offset_y = scroll_snapshot
                        .map(|snapshot| snapshot.offset_y)
                        .unwrap_or(0.0);
                    let viewport_width_px = scroll_snapshot
                        .and_then(|snapshot| snapshot.viewport_measurement)
                        .map(|measurement| measurement.viewport_width_px)
                        .unwrap_or_else(|| {
                            table_projected_viewport_width_px(block, self.theme, document_layout)
                        });
                    let viewport_height_px = scroll_snapshot
                        .and_then(|snapshot| snapshot.viewport_measurement)
                        .map(|measurement| measurement.viewport_height_px)
                        .unwrap_or_else(|| table_content_viewport_height_px(table_view.height_px));
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
                    let chrome_origins = table_chrome_viewport_origins(offset_y);
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
                        &row_track_sizes,
                        &column_track_sizes,
                        chrome_origins,
                        self.theme,
                        view.clone(),
                    );
                    table_chrome.viewport.extend(axis_chrome.viewport);
                    table_chrome.top_edge.extend(axis_chrome.top_edge);
                    table_chrome.left_edge.extend(axis_chrome.left_edge);
                    table_overlay_elements.push(render_table_chrome_viewport(
                        content_origin,
                        viewport_width_px,
                        viewport_height_px,
                        table_chrome,
                    ));
                    if let Some(selection) = table_axis_menu_selection
                        .filter(|selection| selection.block_id == block.block_id)
                    {
                        table_overlay_elements.push(render_table_axis_toolbar(
                            selection,
                            table_view,
                            table_axis_overlay_origin(grid_origin, selection.axis, offset_y),
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
                            offset_table_origin_y(content_origin, offset_y),
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
                        TableReorderOverlayViewport {
                            origin: grid_origin,
                            width_px: viewport_width_px,
                            height_px: viewport_height_px,
                            content_offset_y: offset_y,
                        },
                        table_reorder_preview,
                        self.theme,
                    ) {
                        table_overlay_elements.push(reorder_preview);
                    }
                }
                div()
                    .absolute()
                    .left(px(block_geometry.shell_left_px))
                    .w(px(block_geometry.shell_width_px))
                    .top(px(top as f32))
                    .h(px(height as f32))
                    .child({
                        let block_action =
                            block_action_state_for_projection(projection, block.block_id, action);
                        let show_hover_gutter =
                            hovered_block_id == Some(block.block_id) && !action.dragging;
                        block_view.render(
                            block,
                            text_layout_width_px,
                            text_viewport,
                            view.clone(),
                            focus.clone(),
                            code_language_focus.clone(),
                            image_caption_states.get(&block.block_id).cloned(),
                            workers,
                            asset_provider.clone(),
                            show_hover_gutter,
                            block_action,
                            table_axis_selection
                                .filter(|selection| selection.block_id == block.block_id),
                            image_resize_preview
                                .filter(|(preview_block_id, _, _)| {
                                    *preview_block_id == block.block_id
                                })
                                .map(|(_, width, _)| width),
                            table_resize_preview.filter(|(preview_block_id, _, _, _)| {
                                *preview_block_id == block.block_id
                            }),
                            table_reorder_preview.filter(|(preview_block_id, _, _, _)| {
                                *preview_block_id == block.block_id
                            }),
                            table_range_selection
                                .filter(|selection| selection.block_id == block.block_id),
                            table_cell_selection
                                .filter(|selection| selection.block_id == block.block_id),
                            code_language_edit,
                            code_theme_menu_block_id == Some(block.block_id),
                            code_highlight_theme,
                            suppress_document_text_input,
                            table_scroll_snapshots
                                .get(&block.block_id)
                                .map(|snapshot| snapshot.handle.clone()),
                            code_scroll_handles.get(&block.block_id).cloned(),
                            code_caret_reveal_after_line_break.contains(&block.block_id),
                            collapsed_code_blocks,
                            code_highlights,
                            search_decorations,
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
            .child(render_editor_overlays(
                projection,
                self.theme,
                document_layout,
            ))
            .children(table_overlay_elements)
            .when_some(drag_overlay, |this, overlay| {
                this.child(render_block_drag_overlay(overlay, self.theme))
            })
            .into_any_element();
        let retry_range = projection.payload_visible_block_range.clone();
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
        let mut surface = DocumentSurface::with_scroll_and_layout(
            projection.before_window_height,
            projection.placeholder_window_height,
            projection.after_window_height,
            projection.scroll.global_scroll_top,
            document_layout,
        );
        surface.placeholder_window_error = projection.placeholder_window_error.clone();
        surface.placeholder_window_failure = projection.placeholder_window_failure.clone();
        let (page_chrome, page_icon_menu) = if show_page_chrome {
            let (chrome, menu) = render_page_chrome(
                page_decorations,
                editor_viewport_width_px,
                editor_viewport_height_px,
                document_layout,
                projection.scroll.global_scroll_top,
                readonly,
                page_chrome_extras,
                self.theme,
                workers,
                asset_provider,
                view,
                page_icon_menu_open,
                page_icon_menu_custom_tab,
                page_icon_menu_scroll_handle,
                cx,
            );
            (Some(chrome), menu)
        } else {
            (None, None)
        };
        surface.render(
            self.theme,
            page_chrome,
            block_elements,
            Some(overlay),
            page_icon_menu,
            on_placeholder_retry,
        )
    }
}

fn offset_table_origin_y(
    origin: crate::features::table::TableToolbarEditorOrigin,
    offset_y: f32,
) -> crate::features::table::TableToolbarEditorOrigin {
    crate::features::table::TableToolbarEditorOrigin {
        x_px: origin.x_px,
        y_px: origin.y_px + offset_y,
    }
}

fn table_axis_overlay_origin(
    origin: crate::features::table::TableToolbarEditorOrigin,
    axis: TableAxis,
    offset_y: f32,
) -> crate::features::table::TableToolbarEditorOrigin {
    match axis {
        TableAxis::Row => offset_table_origin_y(origin, offset_y),
        TableAxis::Column => origin,
    }
}

fn document_overlay_menu_viewport(
    editor_width_px: f32,
    editor_height_px: f32,
    scroll_top: f64,
    window_start_global_y: f64,
    document_layout: DocumentLayoutMetrics,
) -> MenuViewportBounds {
    let page_left = ((editor_width_px - document_layout.page_width_px) / 2.0).max(0.0);
    let left = -page_left;
    let top = (scroll_top - window_start_global_y) as f32 - document_layout.top_inset_px;
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
    let placer = div()
        .id("cditor-down-placer")
        .absolute()
        .left_0()
        .right_0()
        .top(px(top as f32))
        .h(px(height as f32))
        .cursor_text();
    on_text_activation(placer, move |_event, window, cx| {
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
        let viewport = document_overlay_menu_viewport(
            1_200.0,
            800.0,
            0.0,
            0.0,
            DocumentLayoutMetrics::for_viewport(1_200.0),
        );

        assert_eq!(viewport.left, 0.0);
        assert_eq!(viewport.right, 1_200.0);
        assert_eq!(viewport.top, -96.0);
        assert_eq!(viewport.bottom, 704.0);
    }

    #[test]
    fn menu_viewport_tracks_document_scroll_and_narrow_hosts() {
        let viewport = document_overlay_menu_viewport(
            700.0,
            500.0,
            240.0,
            0.0,
            DocumentLayoutMetrics::for_viewport(700.0),
        );

        assert_eq!(viewport.left, 0.0);
        assert_eq!(viewport.right, 700.0);
        assert_eq!(viewport.top, 192.0);
        assert_eq!(viewport.bottom, 692.0);
    }

    #[test]
    fn menu_viewport_rebases_far_global_scroll_before_f32_conversion() {
        let viewport = document_overlay_menu_viewport(
            700.0,
            500.0,
            20_000_240.25,
            20_000_000.25,
            DocumentLayoutMetrics::for_viewport(700.0),
        );

        assert_eq!(viewport.top, 192.0);
        assert_eq!(viewport.bottom, 692.0);
    }

    #[test]
    fn menu_viewport_accounts_for_a_visible_readonly_notice() {
        let document_layout =
            DocumentLayoutMetrics::for_viewport(1_200.0).with_additional_top_inset_px(32.0);
        let viewport = document_overlay_menu_viewport(1_200.0, 800.0, 0.0, 0.0, document_layout);

        assert_eq!(viewport.top, -128.0);
        assert_eq!(viewport.bottom, 672.0);
    }
}
