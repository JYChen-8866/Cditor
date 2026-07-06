use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px};

pub fn render_todo(checked: bool, content: AnyElement) -> AnyElement {
    render_prefixed(if checked { "☑" } else { "☐" }.to_owned(), content)
}

pub fn render_bulleted(content: AnyElement) -> AnyElement {
    render_prefixed("•".to_owned(), content)
}

pub fn render_numbered(number: usize, content: AnyElement) -> AnyElement {
    render_prefixed(format!("{number}."), content)
}

fn render_prefixed(prefix: String, content: AnyElement) -> AnyElement {
    div()
        .flex()
        .gap_2()
        .items_baseline()
        .text_size(px(16.0))
        .child(prefix)
        .child(content)
        .into_any_element()
}
