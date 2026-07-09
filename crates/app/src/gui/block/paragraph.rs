use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px};

pub fn render_paragraph(content: AnyElement) -> AnyElement {
    div().text_size(px(16.0)).child(content).into_any_element()
}
