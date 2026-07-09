use gpui::{AnyElement, FontWeight, IntoElement, ParentElement, Styled, div, px};

pub fn render_heading(level: u8, content: AnyElement) -> AnyElement {
    div()
        .text_size(px(match level {
            1 => 28.0,
            2 => 24.0,
            _ => 20.0,
        }))
        .font_weight(FontWeight::BOLD)
        .child(content)
        .into_any_element()
}
