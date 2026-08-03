use gpui::{
    AnyElement, App, Entity, FocusHandle, IntoElement, ParentElement, ScrollHandle, Styled, div,
    px, rgb,
};

use crate::app::worker_admission::EditorWorkerAdmission;
use crate::block::block_content::render_block_content;
use crate::block::block_shell::{BlockActionState, block_shell};
use crate::document::DocumentTextViewport;
use crate::editor_view::CditorV2View;
use crate::features::code::highlight::CodeHighlightCache;
use crate::features::code::render_code_block;
use crate::features::mermaid::{MermaidRenderCache, render_mermaid_block};
use crate::features::search::SearchDecorationState;
use crate::features::table::{
    TableAxisSelection, TableCellRangeSelection, TableCellSelection, TableReorderPreview,
    TableResizePreview,
};
use crate::features::text::heading::render_heading;
use crate::features::text::paragraph::render_paragraph;
use crate::features::whiteboard::WhiteboardThumbnailCache;
use crate::input::{
    CodeLanguageEditState, focus_block_from_mouse, gutter_mouse_down_from_mouse,
    hover_block_from_mouse, toggle_block_fold_from_mouse, toggle_todo_from_mouse,
};
use crate::platform::EDITOR_MONO_FONT_FAMILY;
use crate::surfaces::TextSurfaceRenderState;
use crate::theme::GuiTheme;
use cditor_core::rich_text::RichBlockKind;
use cditor_runtime::ViewBlockSnapshot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockView {
    pub theme: GuiTheme,
}

impl BlockView {
    pub fn new(theme: GuiTheme) -> Self {
        Self { theme }
    }

    #[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
    pub(crate) fn render(
        &self,
        block: &ViewBlockSnapshot,
        text_layout_width_px: f64,
        text_viewport: DocumentTextViewport,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        code_language_focus: FocusHandle,
        image_caption_state: Option<TextSurfaceRenderState>,
        workers: &EditorWorkerAdmission,
        hovered: bool,
        action: BlockActionState,
        table_axis_selection: Option<TableAxisSelection>,
        image_resize_preview_width_px: Option<f32>,
        table_resize_preview: Option<TableResizePreview>,
        table_reorder_preview: Option<TableReorderPreview>,
        table_range_selection: Option<TableCellRangeSelection>,
        table_cell_selection: Option<TableCellSelection>,
        code_language_edit: Option<&CodeLanguageEditState>,
        code_theme_menu_open: bool,
        code_highlight_theme: &'static str,
        suppress_document_text_input: bool,
        table_scroll_handle: Option<ScrollHandle>,
        code_scroll_handle: Option<ScrollHandle>,
        code_caret_reveal_after_line_break: bool,
        collapsed_code_blocks: &std::collections::HashSet<cditor_core::ids::BlockId>,
        code_highlights: &CodeHighlightCache,
        search_decorations: &SearchDecorationState,
        mermaid_renders: &MermaidRenderCache,
        mermaid_show_source: bool,
        whiteboard_thumbnails: &WhiteboardThumbnailCache,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.theme;
        let block_id = block.block_id;
        let content = render_kind_content(
            block,
            text_layout_width_px,
            text_viewport,
            theme,
            view.clone(),
            focus,
            code_language_focus,
            image_caption_state,
            workers,
            action,
            table_axis_selection,
            image_resize_preview_width_px,
            table_resize_preview,
            table_reorder_preview,
            table_range_selection,
            table_cell_selection,
            code_language_edit,
            code_theme_menu_open,
            code_highlight_theme,
            suppress_document_text_input,
            table_scroll_handle,
            code_scroll_handle,
            code_caret_reveal_after_line_break,
            collapsed_code_blocks,
            code_highlights,
            search_decorations,
            mermaid_renders,
            mermaid_show_source,
            whiteboard_thumbnails,
            cx,
        );
        let focus_view = view.clone();
        let hover_view = view.clone();
        let add_view = view.clone();
        let gutter_view = view.clone();
        let on_todo_toggle = matches!(
            block.chrome.prefix,
            cditor_core::block::BlockPrefixSnapshot::Todo { .. }
        )
        .then(|| {
            let todo_view = view.clone();
            Box::new(
                move |event: &gpui::MouseDownEvent,
                      window: &mut gpui::Window,
                      cx: &mut gpui::App| {
                    toggle_todo_from_mouse(&todo_view, block_id, event, window, cx);
                    cx.stop_propagation();
                },
            ) as crate::block::prefix::TodoToggleHandler
        });
        let collapsed_code_block = matches!(block.kind, RichBlockKind::Code { .. })
            && collapsed_code_blocks.contains(&block.block_id);
        let on_fold_toggle = matches!(
            block.chrome.prefix,
            cditor_core::block::BlockPrefixSnapshot::Heading { .. }
                | cditor_core::block::BlockPrefixSnapshot::Toggle { .. }
        )
        .then(|| {
            let fold_view = view.clone();
            Box::new(
                move |event: &gpui::MouseDownEvent,
                      window: &mut gpui::Window,
                      cx: &mut gpui::App| {
                    toggle_block_fold_from_mouse(&fold_view, block_id, event, window, cx);
                    cx.stop_propagation();
                },
            ) as crate::block::prefix::FoldToggleHandler
        });
        block_shell(
            block,
            theme,
            content,
            hovered,
            action,
            collapsed_code_block,
            move |event, window, cx| {
                focus_block_from_mouse(&focus_view, block_id, event, window, cx);
                cx.stop_propagation();
            },
            Some(Box::new(move |event, _window, cx| {
                hover_block_from_mouse(&hover_view, block_id, event, cx);
            })),
            Some(Box::new(move |_event, window, cx| {
                add_view.update(cx, |view, cx| {
                    view.insert_paragraph_after_block_from_gui(block_id, window, cx);
                });
                cx.stop_propagation();
            })),
            Some(Box::new(move |event, window, cx| {
                gutter_mouse_down_from_mouse(&gutter_view, block_id, event, window, cx);
                cx.stop_propagation();
            })),
            on_todo_toggle,
            on_fold_toggle,
        )
    }
}

#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
fn render_kind_content(
    block: &ViewBlockSnapshot,
    text_layout_width_px: f64,
    text_viewport: DocumentTextViewport,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    code_language_focus: FocusHandle,
    image_caption_state: Option<TextSurfaceRenderState>,
    workers: &EditorWorkerAdmission,
    action: BlockActionState,
    table_axis_selection: Option<TableAxisSelection>,
    image_resize_preview_width_px: Option<f32>,
    table_resize_preview: Option<TableResizePreview>,
    table_reorder_preview: Option<TableReorderPreview>,
    table_range_selection: Option<TableCellRangeSelection>,
    table_cell_selection: Option<TableCellSelection>,
    code_language_edit: Option<&CodeLanguageEditState>,
    code_theme_menu_open: bool,
    code_highlight_theme: &'static str,
    suppress_document_text_input: bool,
    table_scroll_handle: Option<ScrollHandle>,
    code_scroll_handle: Option<ScrollHandle>,
    code_caret_reveal_after_line_break: bool,
    collapsed_code_blocks: &std::collections::HashSet<cditor_core::ids::BlockId>,
    code_highlights: &CodeHighlightCache,
    search_decorations: &SearchDecorationState,
    mermaid_renders: &MermaidRenderCache,
    mermaid_show_source: bool,
    whiteboard_thumbnails: &WhiteboardThumbnailCache,
    cx: &mut App,
) -> AnyElement {
    let content = render_block_content(
        block,
        text_layout_width_px,
        text_viewport,
        theme,
        view.clone(),
        focus,
        image_caption_state,
        workers,
        image_resize_preview_width_px,
        table_resize_preview,
        table_reorder_preview,
        table_range_selection,
        table_cell_selection,
        suppress_document_text_input
            || code_language_edit.is_some()
            || (cfg!(feature = "mermaid")
                && matches!(block.kind, RichBlockKind::Mermaid)
                && !mermaid_show_source),
        table_axis_selection,
        table_scroll_handle,
        code_scroll_handle.clone(),
        code_caret_reveal_after_line_break,
        code_highlights,
        search_decorations,
        code_highlight_theme,
        whiteboard_thumbnails,
        cx,
    );
    match block.kind {
        RichBlockKind::Heading { level } => render_heading(level, content),
        RichBlockKind::Quote => content,
        RichBlockKind::Code { ref language } => {
            let language_edit = code_language_edit.filter(|edit| edit.block_id == block.block_id);
            let code_collapsed = collapsed_code_blocks.contains(&block.block_id);
            render_code_block(
                block.block_id,
                content,
                theme,
                language.as_deref(),
                language_edit,
                code_theme_menu_open,
                code_highlight_theme,
                action.action_active,
                view.clone(),
                code_language_focus,
                code_collapsed,
            )
        }
        RichBlockKind::Todo { .. } | RichBlockKind::BulletedList | RichBlockKind::NumberedList => {
            render_paragraph(content)
        }
        RichBlockKind::Table => content,
        RichBlockKind::Math => div()
            .w_full()
            .text_center()
            .text_size(px(20.0))
            .child(content)
            .into_any_element(),
        RichBlockKind::Mermaid => render_mermaid_block(
            block.block_id,
            match &block.payload {
                cditor_core::rich_text::BlockPayloadView::Loaded(payload) => {
                    payload.content_version
                }
                _ => 0,
            },
            content,
            mermaid_show_source,
            mermaid_renders,
            theme,
            view,
            cx,
        ),
        RichBlockKind::RawMarkdown => div()
            .w_full()
            .font_family(EDITOR_MONO_FONT_FAMILY)
            .text_size(px(13.0))
            .child(content)
            .into_any_element(),
        RichBlockKind::Divider | RichBlockKind::Separator => div()
            .w_full()
            .my(px(11.0))
            .h(px(1.0))
            .bg(rgb(theme.border))
            .into_any_element(),
        _ => render_paragraph(content),
    }
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn block_view_can_classify_demo_block_kind() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let block = &projection.blocks[0];
        let view = BlockView::new(GuiTheme::light());

        assert_eq!(view.theme, GuiTheme::light());
        assert!(matches!(
            block.kind,
            cditor_core::rich_text::RichBlockKind::Heading { .. }
        ));
    }
}
