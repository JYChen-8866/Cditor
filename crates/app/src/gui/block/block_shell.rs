use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, ParentElement,
    Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::block::chrome::BlockChromeStyle;
use crate::gui::block::gutter::{GutterMouseDownHandler, render_block_gutter};
use crate::gui::block::prefix::{TodoToggleHandler, render_block_prefix};
use cditor_runtime::ViewBlockSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockActionState {
    pub action_active: bool,
    pub action_root: bool,
    pub dragging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockShellStyle {
    pub indent_px: f32,
    pub border_color: u32,
    pub background_color: u32,
}

impl BlockShellStyle {
    pub fn from_snapshot(block: &ViewBlockSnapshot, theme: GuiTheme) -> Self {
        let chrome = BlockChromeStyle::from_snapshot(block, theme);
        Self {
            indent_px: chrome.indent_px,
            border_color: chrome.content_border,
            background_color: chrome.outer_background,
        }
    }
}

pub type BlockMouseMoveHandler =
    Box<dyn Fn(&MouseMoveEvent, &mut gpui::Window, &mut App) + 'static>;

pub fn block_shell(
    block: &ViewBlockSnapshot,
    theme: GuiTheme,
    content: AnyElement,
    hovered: bool,
    action: BlockActionState,
    on_mouse_down: impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut App) + 'static,
    on_mouse_move: Option<BlockMouseMoveHandler>,
    on_gutter_mouse_down: Option<GutterMouseDownHandler>,
    on_todo_toggle: Option<TodoToggleHandler>,
) -> AnyElement {
    let chrome = BlockChromeStyle::from_snapshot(block, theme);
    let gutter_visible = should_show_gutter(hovered, action.action_root);
    let outer_background = outer_background_for_action(chrome.outer_background, theme, action);
    let content_background =
        content_background_for_action(chrome.content_background, theme, action);
    let shell_border = border_for_action(theme.surface, theme, action);
    let content_border = border_for_action(
        chrome.quote_bar.unwrap_or(chrome.content_border),
        theme,
        action,
    );
    div()
        .id(("v2-block", block.block_id))
        .w_full()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(shell_border))
        .bg(rgb(outer_background))
        .text_color(rgb(chrome.text_color))
        .px_2()
        .py(px(4.0))
        .on_mouse_down(MouseButton::Left, on_mouse_down)
        .when_some(on_mouse_move, |this, handler| this.on_mouse_move(handler))
        .child(
            div().pl(px(chrome.indent_px)).child(
                div()
                    .id(("v2-block-row", block.block_id))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(render_block_gutter(
                        theme,
                        gutter_visible,
                        action.action_active,
                        on_gutter_mouse_down,
                    ))
                    .child(
                        div()
                            .relative()
                            .min_w(px(0.0))
                            .w_full()
                            .min_h(px(chrome.content_min_height_px))
                            .rounded(px(chrome.content_radius_px))
                            .bg(rgb(content_background))
                            .when(chrome.quote_bar.is_none(), |this| this.border_1())
                            .border_color(rgb(content_border))
                            .border_l(px(if chrome.quote_bar.is_some() { 4.0 } else { 1.0 }))
                            .pl(px(chrome.content_padding_left_px))
                            .pr(px(chrome.content_padding_right_px))
                            .py(px(chrome.content_padding_y_px))
                            .flex()
                            .items_center()
                            .when(chrome.quote_bar.is_some(), |this| this.items_start())
                            .child(render_block_prefix(
                                &block.chrome.prefix,
                                theme,
                                true,
                                on_todo_toggle,
                            ))
                            .child(div().min_w(px(0.0)).w_full().child(content)),
                    ),
            ),
        )
        .into_any_element()
}

pub fn should_show_gutter(hovered: bool, action_root: bool) -> bool {
    action_root || hovered
}

pub fn content_background_for_action(
    default_content_background: u32,
    theme: GuiTheme,
    action: BlockActionState,
) -> u32 {
    if action.action_active {
        theme.action_background
    } else {
        default_content_background
    }
}

pub fn outer_background_for_action(
    default_outer_background: u32,
    theme: GuiTheme,
    action: BlockActionState,
) -> u32 {
    if action.action_active {
        theme.action_background
    } else {
        default_outer_background
    }
}

pub fn border_for_action(default_border: u32, theme: GuiTheme, action: BlockActionState) -> u32 {
    if action.action_active {
        theme.action_background
    } else {
        default_border
    }
}

pub fn placeholder_shell(theme: GuiTheme, content: AnyElement) -> AnyElement {
    div()
        .w_full()
        .px_2()
        .py_1()
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.page))
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;

    #[test]
    fn gutter_visibility_prefers_action_root_or_hover_only() {
        assert!(!should_show_gutter(false, false));
        assert!(should_show_gutter(true, false));
        assert!(should_show_gutter(false, true));
    }

    #[test]
    fn action_active_uses_v1_action_background_without_height_change() {
        let theme = GuiTheme::light();
        let action = BlockActionState {
            action_active: true,
            action_root: false,
            dragging: true,
        };
        assert_eq!(
            content_background_for_action(0x123456, theme, action),
            theme.action_background
        );
        assert_eq!(
            outer_background_for_action(0xabcdef, theme, action),
            theme.action_background
        );
        assert_eq!(
            border_for_action(theme.surface, theme, action),
            theme.action_background
        );
        assert_eq!(
            content_background_for_action(0x123456, theme, BlockActionState::default()),
            0x123456
        );
        assert_eq!(
            outer_background_for_action(0xabcdef, theme, BlockActionState::default()),
            0xabcdef
        );
        assert_eq!(
            border_for_action(theme.surface, theme, BlockActionState::default()),
            theme.surface
        );
    }

    #[test]
    fn block_shell_style_uses_chrome_depth_focus_and_selection() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection_for_window();
        let mut block = projection.blocks[0].clone();
        block.depth = 2;
        block.chrome.list_info.depth = 2;
        block.focused = true;
        block.selected = false;

        let style = BlockShellStyle::from_snapshot(&block, GuiTheme::light());

        assert_eq!(style.indent_px, 48.0);
        assert_eq!(style.background_color, GuiTheme::light().surface);
    }
}
