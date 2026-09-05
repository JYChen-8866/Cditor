use gpui::{
    AnyElement, AnyView, App, Entity, FocusHandle, IntoElement, ParentElement, ScrollHandle,
    Styled, div, px,
};

use crate::app::worker_admission::EditorWorkerAdmission;
use crate::block::block_content::render_block_content;
use crate::block::block_shell::{BlockActionState, block_shell};
use crate::block::divider::render_notion_divider;
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
use crate::features::text::heading::{render_document_title, render_heading};
use crate::features::text::paragraph::render_paragraph;
use crate::features::video::VideoPlaybackCache;
use crate::features::whiteboard::WhiteboardThumbnailCache;
use crate::input::{
    CodeLanguageEditState, focus_block_from_mouse, gutter_mouse_down_from_mouse,
    hover_block_from_mouse, open_link_from_mouse, toggle_block_fold_from_mouse,
    toggle_todo_from_mouse,
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
        document_title_footer: Option<AnyView>,
        focus: FocusHandle,
        code_language_focus: FocusHandle,
        image_caption_state: Option<TextSurfaceRenderState>,
        collection_title_state: Option<TextSurfaceRenderState>,
        workers: &EditorWorkerAdmission,
        asset_provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
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
        mermaid_source_scroll_handle: Option<ScrollHandle>,
        mermaid_source_caret_reveal_after_line_break: bool,
        collapsed_code_blocks: &std::collections::HashSet<cditor_core::ids::BlockId>,
        code_copy_feedback_block_id: Option<cditor_core::ids::BlockId>,
        // 语言为 mermaid 且当前在看图的代码块。
        mermaid_preview_code_blocks: &std::collections::HashSet<cditor_core::ids::BlockId>,
        code_collapse_tweens: &std::collections::HashMap<
            cditor_core::ids::BlockId,
            crate::features::code::CodeCollapseTween,
        >,
        code_highlights: &CodeHighlightCache,
        search_decorations: &SearchDecorationState,
        mermaid_renders: &MermaidRenderCache,
        mermaid_show_source: bool,
        mermaid_source_tweens: &std::collections::HashMap<
            cditor_core::ids::BlockId,
            crate::features::code::CodeCollapseTween,
        >,
        video_playbacks: &VideoPlaybackCache,
        whiteboard_thumbnails: &WhiteboardThumbnailCache,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.theme;
        let block_id = block.block_id;
        let is_document_title = block.kind.is_document_title();
        let content = render_kind_content(
            block,
            text_layout_width_px,
            text_viewport,
            theme,
            view.clone(),
            focus,
            code_language_focus,
            image_caption_state,
            collection_title_state,
            workers,
            asset_provider,
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
            mermaid_source_scroll_handle,
            mermaid_source_caret_reveal_after_line_break,
            collapsed_code_blocks,
            code_copy_feedback_block_id,
            mermaid_preview_code_blocks,
            code_collapse_tweens,
            code_highlights,
            search_decorations,
            mermaid_renders,
            mermaid_show_source,
            mermaid_source_tweens,
            video_playbacks,
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
        let code_block_kind = matches!(block.kind, RichBlockKind::Code { .. });
        // While a tween is active we keep the content surface mounted (clipped)
        // so the height change continuously slides the rest of the document.
        let has_active_tween =
            code_block_kind && code_collapse_tweens.contains_key(&block.block_id);

        // For the outer block shell: release the inner content min_h reservation
        // both when stably collapsed *and* while animating. The live tween height
        // (fed to layout + used for the absolute shell container) is authoritative.
        let collapsed_code_block = code_block_kind
            && (collapsed_code_blocks.contains(&block.block_id) || has_active_tween);
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
        let action = if is_document_title {
            BlockActionState::default()
        } else {
            action
        };
        let on_mouse_move: Option<crate::block::block_shell::BlockMouseMoveHandler> =
            (!is_document_title).then_some(Box::new(move |event, _window, cx| {
                hover_block_from_mouse(&hover_view, block_id, event, cx);
            }));
        let on_gutter_add: Option<crate::block::gutter::GutterAddHandler> = (!is_document_title)
            .then_some(Box::new(move |_event, window, cx| {
                add_view.update(cx, |view, cx| {
                    view.insert_paragraph_after_block_from_gui(block_id, window, cx);
                });
                cx.stop_propagation();
            }));
        let on_gutter_mouse_down: Option<crate::block::gutter::GutterMouseDownHandler> =
            (!is_document_title).then_some(Box::new(move |event, window, cx| {
                gutter_mouse_down_from_mouse(&gutter_view, block_id, event, window, cx);
                cx.stop_propagation();
            }));
        block_shell(
            block,
            theme,
            content,
            document_title_footer,
            hovered,
            action,
            collapsed_code_block,
            move |event, window, cx| {
                if open_link_from_mouse(&focus_view, block_id, event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                focus_block_from_mouse(&focus_view, block_id, event, window, cx);
                cx.stop_propagation();
            },
            on_mouse_move,
            on_gutter_add,
            on_gutter_mouse_down,
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
    collection_title_state: Option<TextSurfaceRenderState>,
    workers: &EditorWorkerAdmission,
    asset_provider: Option<std::sync::Arc<dyn cditor_sdk::providers::AssetProvider>>,
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
    mermaid_source_scroll_handle: Option<ScrollHandle>,
    mermaid_source_caret_reveal_after_line_break: bool,
    collapsed_code_blocks: &std::collections::HashSet<cditor_core::ids::BlockId>,
    code_copy_feedback_block_id: Option<cditor_core::ids::BlockId>,
    // 语言为 mermaid 且当前在看图的代码块。
    mermaid_preview_code_blocks: &std::collections::HashSet<cditor_core::ids::BlockId>,
    code_collapse_tweens: &std::collections::HashMap<
        cditor_core::ids::BlockId,
        crate::features::code::CodeCollapseTween,
    >,
    code_highlights: &CodeHighlightCache,
    search_decorations: &SearchDecorationState,
    mermaid_renders: &MermaidRenderCache,
    mermaid_show_source: bool,
    mermaid_source_tweens: &std::collections::HashMap<
        cditor_core::ids::BlockId,
        crate::features::code::CodeCollapseTween,
    >,
    video_playbacks: &VideoPlaybackCache,
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
        collection_title_state,
        workers,
        asset_provider,
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
        code_scroll_handle,
        code_caret_reveal_after_line_break,
        mermaid_source_scroll_handle,
        mermaid_source_caret_reveal_after_line_break,
        code_highlights,
        search_decorations,
        code_highlight_theme,
        video_playbacks,
        whiteboard_thumbnails,
        cx,
    );
    match block.kind {
        RichBlockKind::DocumentTitle => render_document_title(content),
        RichBlockKind::Heading { level } => render_heading(level, content),
        RichBlockKind::Quote => content,
        RichBlockKind::Code { ref language } => {
            let language_edit = code_language_edit.filter(|edit| edit.block_id == block.block_id);
            // Stable collapsed (no active animation) hides the content subtree entirely.
            let code_collapsed = collapsed_code_blocks.contains(&block.block_id)
                && !code_collapse_tweens.contains_key(&block.block_id);

            // If a tween is active, compute how much of the *code content surface*
            // (below the toolbar header) should be visible right now.
            // The tween itself stores the *total* block height; subtract the collapsed
            // header geometry to get the visible content area for clipping.
            let animated_content_height = code_collapse_tweens.get(&block.block_id).map(|tween| {
                (tween.frame_height - crate::features::code::V1_CODE_COLLAPSED_BLOCK_HEIGHT_PX)
                    .max(0.0)
            });

            // 语言是 mermaid 且开了预览：内容区换成渲染图。块类型仍是 Code，
            // 所以工具栏、折叠、语言选择器全都不变，只有内容区换了东西。
            let content = if crate::features::code::language_is_mermaid(language.as_deref())
                && mermaid_preview_code_blocks.contains(&block.block_id)
            {
                let content_version = match &block.payload {
                    cditor_core::rich_text::BlockPayloadView::Loaded(payload) => {
                        payload.content_version
                    }
                    _ => 0,
                };
                crate::features::mermaid::render_code_block_mermaid_preview(
                    block.block_id,
                    content_version,
                    content,
                    mermaid_renders,
                    theme,
                    view.clone(),
                    cx,
                )
            } else {
                content
            };

            render_code_block(
                block.block_id,
                content,
                theme,
                language.as_deref(),
                language_edit,
                code_theme_menu_open,
                code_highlight_theme,
                action.action_active,
                view,
                code_language_focus,
                code_collapsed,
                animated_content_height,
                // chevron 要跟"意图"而不是"稳定折叠态"：动画途中也得立刻翻向。
                collapsed_code_blocks.contains(&block.block_id),
                code_copy_feedback_block_id == Some(block.block_id),
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
        RichBlockKind::Mermaid => {
            let animated_block_height = mermaid_source_tweens
                .get(&block.block_id)
                .map(|tween| tween.frame_height);
            render_mermaid_block(
                block.block_id,
                match &block.payload {
                    cditor_core::rich_text::BlockPayloadView::Loaded(payload) => {
                        payload.content_version
                    }
                    _ => 0,
                },
                block.layout.effective_height(),
                mermaid_source_block_height_px(block, text_layout_width_px),
                content,
                mermaid_show_source,
                mermaid_renders,
                theme,
                view,
                animated_block_height,
                cx,
            )
        }
        RichBlockKind::RawMarkdown => div()
            .w_full()
            .font_family(EDITOR_MONO_FONT_FAMILY)
            .text_size(px(13.0))
            .child(content)
            .into_any_element(),
        RichBlockKind::Divider | RichBlockKind::Separator => render_notion_divider(theme),
        _ => render_paragraph(content),
    }
}

/// Mermaid 源码模式的块高度：与代码块同一套行度量，随源码行数增长。
fn mermaid_source_block_height_px(block: &ViewBlockSnapshot, text_layout_width_px: f64) -> f64 {
    let source = match &block.payload {
        cditor_core::rich_text::BlockPayloadView::Loaded(payload) => payload.plain_text(),
        _ => String::new(),
    };
    cditor_core::layout::estimate_mermaid_source_block_height_px(&source, text_layout_width_px)
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
