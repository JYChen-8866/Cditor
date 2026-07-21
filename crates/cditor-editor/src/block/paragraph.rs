use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px};

pub const NOTION_PARAGRAPH_TEXT_SIZE_PX: f32 = 16.0;

pub fn render_paragraph(content: AnyElement) -> AnyElement {
    div()
        .text_size(px(NOTION_PARAGRAPH_TEXT_SIZE_PX))
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notion_paragraph_text_size_is_stable() {
        assert_eq!(NOTION_PARAGRAPH_TEXT_SIZE_PX, 16.0);
    }
}
