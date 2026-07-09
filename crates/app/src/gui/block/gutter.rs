use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Styled, Window, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::block::chrome::{BLOCK_GUTTER_HEIGHT_PX, BLOCK_GUTTER_WIDTH_PX};

pub type GutterMouseDownHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;

pub fn render_block_gutter(
    theme: GuiTheme,
    visible: bool,
    action_active: bool,
    on_mouse_down: Option<GutterMouseDownHandler>,
) -> AnyElement {
    let color = if action_active {
        theme.action_accent
    } else {
        theme.gutter_foreground
    };
    div()
        .w(px(BLOCK_GUTTER_WIDTH_PX))
        .h(px(BLOCK_GUTTER_HEIGHT_PX))
        .flex_shrink_0()
        .child(if visible {
            div()
                .w(px(BLOCK_GUTTER_WIDTH_PX))
                .h(px(BLOCK_GUTTER_HEIGHT_PX))
                .rounded(px(7.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(if action_active {
                    theme.action_background
                } else {
                    theme.gutter_background
                }))
                .text_color(rgb(color))
                .cursor_pointer()
                .when_some(on_mouse_down, |this, handler| {
                    this.on_mouse_down(MouseButton::Left, handler)
                })
                .hover(move |style| {
                    style
                        .bg(rgb(theme.action_hover_background))
                        .text_color(rgb(theme.action_accent))
                })
                .child(render_gutter_handle_icon(color))
                .into_any_element()
        } else {
            div().into_any_element()
        })
        .into_any_element()
}

fn render_gutter_handle_icon(color: u32) -> AnyElement {
    div()
        .w(px(12.0))
        .h(px(16.0))
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .gap(px(2.0))
        .children((0..3).map(move |_| {
            div()
                .flex()
                .gap(px(2.0))
                .children((0..2).map(move |_| render_gutter_handle_dot(color)))
        }))
        .into_any_element()
}

fn render_gutter_handle_dot(color: u32) -> AnyElement {
    div()
        .w(px(2.5))
        .h(px(2.5))
        .rounded(px(2.0))
        .bg(rgb(color))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_dimensions_match_v1_contract() {
        assert_eq!(BLOCK_GUTTER_WIDTH_PX, 24.0);
        assert_eq!(BLOCK_GUTTER_HEIGHT_PX, 22.0);
    }
}
