use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, rgb};

use crate::gui::GuiTheme;
use crate::gui::block::skeleton::render_block_skeleton;
use cditor_runtime::ViewBlockSnapshot;

pub fn render_placeholder(block: &ViewBlockSnapshot, theme: GuiTheme) -> AnyElement {
    render_block_skeleton(block, theme)
}

pub fn render_loading(block: &ViewBlockSnapshot, theme: GuiTheme) -> AnyElement {
    render_block_skeleton(block, theme)
}

pub fn render_error(message: &str) -> AnyElement {
    div()
        .text_color(rgb(0xcf222e))
        .child(format!("Error: {message}"))
        .into_any_element()
}
