use gpui::{AnyElement, App, Entity, FocusHandle, IntoElement, Styled, div, rgb};

use crate::core::rich_text::RichBlockKind;
use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::block_content::render_block_content;
use crate::gui::block::block_shell::{BlockActionState, block_shell};
use crate::gui::block::code::render_code_block;
use crate::gui::block::heading::render_heading;
use crate::gui::block::paragraph::render_paragraph;
use crate::gui::input::{
    focus_block_from_mouse, gutter_mouse_down_from_mouse, hover_block_from_mouse,
    toggle_todo_from_mouse,
};
use crate::runtime::ViewBlockSnapshot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockView {
    pub theme: GuiTheme,
}

impl BlockView {
    pub fn new(theme: GuiTheme) -> Self {
        Self { theme }
    }

    pub fn render(
        &self,
        block: &ViewBlockSnapshot,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        hovered: bool,
        action: BlockActionState,
        image_resize_preview_width_px: Option<f32>,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.theme;
        let block_id = block.block_id;
        let content = render_kind_content(
            block,
            theme,
            view.clone(),
            focus,
            action,
            image_resize_preview_width_px,
            cx,
        );
        let focus_view = view.clone();
        let hover_view = view.clone();
        let gutter_view = view.clone();
        let on_todo_toggle = matches!(
            block.chrome.prefix,
            crate::core::block::BlockPrefixSnapshot::Todo { .. }
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
            Some(Box::new(move |event, window, cx| {
                gutter_mouse_down_from_mouse(&gutter_view, block_id, event, window, cx);
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
    action: BlockActionState,
    image_resize_preview_width_px: Option<f32>,
    cx: &mut App,
) -> AnyElement {
    let content =
        render_block_content(block, theme, view, focus, image_resize_preview_width_px, cx);
    match block.kind {
        RichBlockKind::Heading { level } => render_heading(level, content),
        RichBlockKind::Quote => content,
        RichBlockKind::Code { ref language } => {
            render_code_block(content, theme, language.as_deref(), action.action_active)
        }
        RichBlockKind::Todo { .. } | RichBlockKind::BulletedList | RichBlockKind::NumberedList => {
            render_paragraph(content)
        }
        RichBlockKind::Divider | RichBlockKind::Separator => div()
            .my_2()
            .border_t_1()
            .border_color(rgb(theme.border))
            .into_any_element(),
        _ => render_paragraph(content),
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::DocumentRuntime;

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
            crate::core::rich_text::RichBlockKind::Heading { .. }
        ));
    }
}
