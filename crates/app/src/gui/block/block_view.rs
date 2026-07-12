use gpui::{
    AnyElement, App, Entity, FocusHandle, IntoElement, ParentElement, ScrollHandle, Styled, div,
    px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::block_content::render_block_content;
use crate::gui::block::block_shell::{BlockActionState, block_shell};
use crate::gui::block::code::render_code_block;
use crate::gui::block::heading::render_heading;
use crate::gui::block::paragraph::render_paragraph;
use crate::gui::block::table::{
    TableAxisSelection, TableCellRangeSelection, TableReorderPreview, TableResizePreview,
};
use crate::gui::block::{MermaidRenderCache, WhiteboardThumbnailCache, render_mermaid_block};
use crate::gui::input::{
    CodeLanguageEditState, focus_block_from_mouse, gutter_mouse_down_from_mouse,
    hover_block_from_mouse, toggle_todo_from_mouse,
};
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

    pub(crate) fn render(
        &self,
        block: &ViewBlockSnapshot,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        code_language_focus: FocusHandle,
        hovered: bool,
        action: BlockActionState,
        table_axis_selection: Option<TableAxisSelection>,
        image_resize_preview_width_px: Option<f32>,
        table_resize_preview: Option<TableResizePreview>,
        table_reorder_preview: Option<TableReorderPreview>,
        table_range_selection: Option<TableCellRangeSelection>,
        code_language_edit: Option<&CodeLanguageEditState>,
        table_scroll_handle: Option<ScrollHandle>,
        mermaid_renders: &MermaidRenderCache,
        mermaid_show_source: bool,
        whiteboard_thumbnails: &WhiteboardThumbnailCache,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.theme;
        let block_id = block.block_id;
        let content = render_kind_content(
            block,
            theme,
            view.clone(),
            focus,
            code_language_focus,
            action,
            table_axis_selection,
            image_resize_preview_width_px,
            table_resize_preview,
            table_reorder_preview,
            table_range_selection,
            code_language_edit,
            table_scroll_handle,
            mermaid_renders,
            mermaid_show_source,
            whiteboard_thumbnails,
            cx,
        );
        let focus_view = view.clone();
        let hover_view = view.clone();
        let add_view = view.clone();
        let gutter_view = view.clone();
        let delete_view = view.clone();
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
            ) as crate::gui::block::prefix::TodoToggleHandler
        });
        block_shell(
            block,
            theme,
            content,
            hovered,
            action,
            move |event, window, cx| {
                focus_block_from_mouse(&focus_view, block_id, event, window, cx);
                cx.stop_propagation();
            },
            Some(Box::new(move |event, _window, cx| {
                hover_block_from_mouse(&hover_view, block_id, event, cx);
            })),
            Some(Box::new(move |_event, window, cx| {
                let _ = add_view.update(cx, |view, cx| {
                    view.insert_paragraph_after_block_from_gui(block_id, window, cx);
                });
                cx.stop_propagation();
            })),
            Some(Box::new(move |event, window, cx| {
                gutter_mouse_down_from_mouse(&gutter_view, block_id, event, window, cx);
                cx.stop_propagation();
            })),
            Some(Box::new(move |_event, _window, cx| {
                let _ = delete_view.update(cx, |view, cx| {
                    view.delete_block_from_gui(block_id, cx);
                });
                cx.stop_propagation();
            })),
            on_todo_toggle,
        )
    }
}

fn render_kind_content(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
    code_language_focus: FocusHandle,
    action: BlockActionState,
    table_axis_selection: Option<TableAxisSelection>,
    image_resize_preview_width_px: Option<f32>,
    table_resize_preview: Option<TableResizePreview>,
    table_reorder_preview: Option<TableReorderPreview>,
    table_range_selection: Option<TableCellRangeSelection>,
    code_language_edit: Option<&CodeLanguageEditState>,
    table_scroll_handle: Option<ScrollHandle>,
    mermaid_renders: &MermaidRenderCache,
    mermaid_show_source: bool,
    whiteboard_thumbnails: &WhiteboardThumbnailCache,
    cx: &mut App,
) -> AnyElement {
    let content = render_block_content(
        block,
        theme,
        view.clone(),
        focus,
        image_resize_preview_width_px,
        table_resize_preview,
        table_reorder_preview,
        table_range_selection,
        code_language_edit.is_some()
            || (matches!(block.kind, RichBlockKind::Mermaid) && !mermaid_show_source),
        table_axis_selection,
        table_scroll_handle,
        whiteboard_thumbnails,
        cx,
    );
    match block.kind {
        RichBlockKind::Heading { level } => render_heading(level, content),
        RichBlockKind::Quote => content,
        RichBlockKind::Code { ref language } => {
            let language_edit = code_language_edit.filter(|edit| edit.block_id == block.block_id);
            render_code_block(
                block.block_id,
                content,
                theme,
                language.as_deref(),
                language_edit,
                action.action_active,
                view.clone(),
                code_language_focus,
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
            content,
            mermaid_show_source,
            mermaid_renders,
            theme,
            view,
        ),
        RichBlockKind::RawMarkdown => div()
            .w_full()
            .rounded(px(3.0))
            .bg(rgb(theme.code_background))
            .font_family("Menlo")
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
