use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Styled, Window, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::block::chrome::{BLOCK_GUTTER_HEIGHT_PX, BLOCK_GUTTER_WIDTH_PX};

pub type GutterMouseDownHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;
pub type GutterAddHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;
pub type GutterDeleteHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;

const GUTTER_BUTTON_SIZE_PX: f32 = 24.0;
const GUTTER_CLUSTER_WIDTH_PX: f32 = GUTTER_BUTTON_SIZE_PX * 3.0;
const GUTTER_CLUSTER_LEFT_PX: f32 = -GUTTER_BUTTON_SIZE_PX * 2.0;

pub fn render_block_gutter(
    theme: GuiTheme,
    visible: bool,
    action_active: bool,
    on_add: Option<GutterAddHandler>,
    on_mouse_down: Option<GutterMouseDownHandler>,
    on_delete: Option<GutterDeleteHandler>,
) -> AnyElement {
    let color = if action_active {
        theme.action_accent
    } else {
        theme.gutter_foreground
    };
    div()
        .w(px(BLOCK_GUTTER_WIDTH_PX))
        .h(px(BLOCK_GUTTER_HEIGHT_PX))
        .relative()
        .flex_shrink_0()
        .child(if visible {
            div()
                .absolute()
                .left(px(GUTTER_CLUSTER_LEFT_PX))
                .top_0()
                .w(px(GUTTER_CLUSTER_WIDTH_PX))
                .h(px(GUTTER_BUTTON_SIZE_PX))
                .flex()
                .items_center()
                .child(render_delete_button(theme, on_delete))
                .child(render_add_button(theme, on_add))
                .child(render_drag_button(
                    theme,
                    color,
                    action_active,
                    on_mouse_down,
                ))
                .into_any_element()
        } else {
            div().into_any_element()
        })
        .into_any_element()
}

fn render_delete_button(theme: GuiTheme, on_delete: Option<GutterDeleteHandler>) -> AnyElement {
    div()
        .size(px(GUTTER_BUTTON_SIZE_PX))
        .rounded(px(3.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(14.0))
        .text_color(rgb(theme.gutter_foreground))
        .cursor_pointer()
        .hover(move |style| {
            style
                .bg(rgb(theme.hover_surface))
                .text_color(rgb(theme.danger))
        })
        .when_some(on_delete, |this, handler| {
            this.on_mouse_down(MouseButton::Left, handler)
        })
        .child("×")
        .into_any_element()
}

fn render_add_button(theme: GuiTheme, on_add: Option<GutterAddHandler>) -> AnyElement {
    div()
        .size(px(GUTTER_BUTTON_SIZE_PX))
        .rounded(px(3.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(20.0))
        .text_color(rgb(theme.gutter_foreground))
        .cursor_pointer()
        .hover(move |style| {
            style
                .bg(rgb(theme.hover_surface))
                .text_color(rgb(theme.text))
        })
        .when_some(on_add, |this, handler| {
            this.on_mouse_down(MouseButton::Left, handler)
        })
        .child("+")
        .into_any_element()
}

fn render_drag_button(
    theme: GuiTheme,
    color: u32,
    action_active: bool,
    on_mouse_down: Option<GutterMouseDownHandler>,
) -> AnyElement {
    div()
        .size(px(GUTTER_BUTTON_SIZE_PX))
        .rounded(px(3.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(if action_active {
            theme.action_background
        } else {
            theme.surface
        }))
        .cursor_pointer()
        .when_some(on_mouse_down, |this, handler| {
            this.on_mouse_down(MouseButton::Left, handler)
        })
        .hover(move |style| style.bg(rgb(theme.hover_surface)))
        .child(render_gutter_handle_icon(color))
        .into_any_element()
}

fn render_gutter_handle_icon(color: u32) -> AnyElement {
    div()
        .w(px(10.0))
        .h(px(14.0))
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
        .w(px(2.0))
        .h(px(2.0))
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
        assert_eq!(BLOCK_GUTTER_HEIGHT_PX, 24.0);
        assert_eq!(GUTTER_CLUSTER_WIDTH_PX, 72.0); // 24.0 * 3.0 buttons
        assert_eq!(GUTTER_CLUSTER_LEFT_PX, -48.0); // -24.0 * 2.0
    }
}
