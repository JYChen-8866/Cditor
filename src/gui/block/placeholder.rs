use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, rgb};

use crate::gui::GuiTheme;

pub fn render_placeholder(theme: GuiTheme) -> AnyElement {
    div()
        .text_color(rgb(theme.muted))
        .child("Loading placeholder...")
        .into_any_element()
}

pub fn render_loading(theme: GuiTheme) -> AnyElement {
    div()
        .text_color(rgb(theme.muted))
        .child("Loading...")
        .into_any_element()
}

pub fn render_error(message: &str) -> AnyElement {
    div()
        .text_color(rgb(0xcf222e))
        .child(format!("Error: {message}"))
        .into_any_element()
}
