use crate::gui::GuiTheme;
use gpui::InteractiveElement;
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px, rgb};

pub const V1_CODE_BLOCK_MIN_HEIGHT_PX: f32 = 92.0;
pub const V1_CODE_BLOCK_RADIUS_PX: f32 = 8.0;
pub const V1_CODE_TOOLBAR_TOP_PX: f32 = 6.0;
pub const V1_CODE_TOOLBAR_RIGHT_PX: f32 = 6.0;
pub const V1_CODE_TOOLBAR_HEIGHT_PX: f32 = 30.0;
pub const V1_CODE_TOOLBAR_RADIUS_PX: f32 = 7.0;
pub const V1_CODE_TOOLBAR_PADDING_PX: f32 = 2.0;
pub const V1_CODE_TOOLBAR_BUTTON_SIZE_PX: f32 = 26.0;
pub const V1_CODE_TOOLBAR_BUTTON_RADIUS_PX: f32 = 5.0;
pub const V1_CODE_CONTENT_PADDING_TOP_PX: f32 = 34.0;
pub const V1_CODE_CONTENT_PADDING_X_PX: f32 = 14.0;
pub const V1_CODE_CONTENT_PADDING_BOTTOM_PX: f32 = 14.0;

pub fn render_code_block(
    content: AnyElement,
    theme: GuiTheme,
    language: Option<&str>,
    action_active: bool,
) -> AnyElement {
    div()
        .relative()
        .w_full()
        .min_h(px(V1_CODE_BLOCK_MIN_HEIGHT_PX))
        .rounded(px(V1_CODE_BLOCK_RADIUS_PX))
        .bg(rgb(if action_active {
            theme.action_background
        } else {
            theme.code_background
        }))
        .overflow_hidden()
        .font_family("Menlo")
        .child(render_code_toolbar(theme, language))
        .child(
            div()
                .w_full()
                .pt(px(V1_CODE_CONTENT_PADDING_TOP_PX))
                .px(px(V1_CODE_CONTENT_PADDING_X_PX))
                .pb(px(V1_CODE_CONTENT_PADDING_BOTTOM_PX))
                .text_color(rgb(theme.code_text))
                .child(content),
        )
        .into_any_element()
}

fn render_code_toolbar(theme: GuiTheme, language: Option<&str>) -> AnyElement {
    div()
        .absolute()
        .top(px(V1_CODE_TOOLBAR_TOP_PX))
        .right(px(V1_CODE_TOOLBAR_RIGHT_PX))
        .occlude()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(4.0))
        .child(
            div()
                .h(px(V1_CODE_TOOLBAR_HEIGHT_PX))
                .flex()
                .items_center()
                .gap(px(2.0))
                .rounded(px(V1_CODE_TOOLBAR_RADIUS_PX))
                .border_1()
                .border_color(rgb(theme.code_toolbar_border))
                .bg(rgb(theme.code_toolbar_background))
                .shadow_sm()
                .p(px(V1_CODE_TOOLBAR_PADDING_PX))
                .text_size(px(12.0))
                .text_color(rgb(theme.code_toolbar_text))
                .child(render_language_selector(theme, language))
                .child(render_toolbar_icon_button(theme, "⧉"))
                .child(render_toolbar_icon_button(theme, "…")),
        )
        .into_any_element()
}

fn render_language_selector(theme: GuiTheme, language: Option<&str>) -> AnyElement {
    div()
        .h(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(V1_CODE_TOOLBAR_BUTTON_RADIUS_PX))
        .text_color(rgb(theme.code_toolbar_text))
        .hover(move |style| style.bg(rgb(theme.code_toolbar_hover)))
        .child(language.unwrap_or("plain text").to_owned())
        .child("⌄")
        .into_any_element()
}

fn render_toolbar_icon_button(theme: GuiTheme, label: &'static str) -> AnyElement {
    div()
        .w(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .h(px(V1_CODE_TOOLBAR_BUTTON_SIZE_PX))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(V1_CODE_TOOLBAR_BUTTON_RADIUS_PX))
        .text_color(rgb(theme.code_toolbar_icon))
        .hover(move |style| style.bg(rgb(theme.code_toolbar_hover)))
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_code_block_geometry_constants_match_editor2() {
        assert_eq!(V1_CODE_BLOCK_MIN_HEIGHT_PX, 92.0);
        assert_eq!(V1_CODE_BLOCK_RADIUS_PX, 8.0);
        assert_eq!(V1_CODE_TOOLBAR_TOP_PX, 6.0);
        assert_eq!(V1_CODE_TOOLBAR_RIGHT_PX, 6.0);
        assert_eq!(V1_CODE_TOOLBAR_HEIGHT_PX, 30.0);
        assert_eq!(V1_CODE_TOOLBAR_RADIUS_PX, 7.0);
        assert_eq!(V1_CODE_TOOLBAR_BUTTON_SIZE_PX, 26.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_TOP_PX, 34.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_X_PX, 14.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_BOTTOM_PX, 14.0);
    }
}
