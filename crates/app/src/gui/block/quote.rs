use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, rgb};

use crate::gui::GuiTheme;

pub fn render_quote(content: AnyElement, theme: GuiTheme) -> AnyElement {
    div()
        .border_l_4()
        .border_color(rgb(theme.border))
        .pl_3()
        .text_color(rgb(theme.muted))
        .child(content)
        .into_any_element()
}
