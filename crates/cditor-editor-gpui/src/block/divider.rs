use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgba};

use crate::theme::GuiTheme;

pub(crate) const NOTION_DIVIDER_BLOCK_HEIGHT_PX: f32 = 13.0;
pub(crate) const NOTION_DIVIDER_INSET_X_PX: f32 = 8.0;
pub(crate) const NOTION_DIVIDER_LINE_HEIGHT_PX: f32 = 1.0;
pub(crate) const NOTION_DIVIDER_ALPHA: u32 = 0x1c;

pub(crate) const fn notion_divider_color(text_color: u32) -> u32 {
    (text_color << 8) | NOTION_DIVIDER_ALPHA
}

pub(crate) fn render_notion_divider(theme: GuiTheme) -> AnyElement {
    div()
        .w_full()
        .h(px(NOTION_DIVIDER_BLOCK_HEIGHT_PX))
        .px(px(NOTION_DIVIDER_INSET_X_PX))
        .flex()
        .items_center()
        .child(
            div()
                .w_full()
                .h(px(NOTION_DIVIDER_LINE_HEIGHT_PX))
                .bg(rgba(notion_divider_color(theme.text))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notion_divider_geometry_matches_reference() {
        assert_eq!(NOTION_DIVIDER_BLOCK_HEIGHT_PX, 13.0);
        assert_eq!(NOTION_DIVIDER_INSET_X_PX, 8.0);
        assert_eq!(NOTION_DIVIDER_LINE_HEIGHT_PX, 1.0);
    }

    #[test]
    fn divider_uses_eleven_percent_theme_text_color() {
        assert_eq!(notion_divider_color(0x123456), 0x1234561c);
    }
}
