use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::block::skeleton::render_block_skeleton;
use cditor_runtime::ViewBlockSnapshot;

pub fn render_placeholder(block: &ViewBlockSnapshot, theme: GuiTheme) -> AnyElement {
    render_block_skeleton(block, theme)
}

pub fn render_empty_ai_hint(theme: GuiTheme) -> AnyElement {
    div()
        .absolute()
        .left(px(4.0))
        .top(px(0.0))
        .text_size(px(13.0))
        .text_color(rgb(theme.muted))
        .child("按 space（空格）以启用 AI，或按“/”启用命令")
        .into_any_element()
}

pub fn render_loading(block: &ViewBlockSnapshot, theme: GuiTheme) -> AnyElement {
    render_block_skeleton(block, theme)
}

pub fn render_error(message: &str, theme: GuiTheme) -> AnyElement {
    div()
        .text_size(gpui::px(13.0))
        .text_color(rgb(theme.danger))
        .child(format!("Unable to load block: {message}"))
        .into_any_element()
}
