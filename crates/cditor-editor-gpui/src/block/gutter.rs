use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Styled, Window, div, px, rgb,
};

use cditor_component::SvgIcon;

use crate::block::chrome::BLOCK_GUTTER_WIDTH_PX;
#[cfg(test)]
use crate::block::chrome::BLOCK_GUTTER_HEIGHT_PX;
use crate::theme::GuiTheme;

pub type GutterMouseDownHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;
pub type GutterAddHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;

const GUTTER_CONTROL_WIDTH_PX: f32 = 22.0;
const GUTTER_CONTROL_HEIGHT_PX: f32 = 28.0;
const GUTTER_CLUSTER_WIDTH_PX: f32 = GUTTER_CONTROL_WIDTH_PX;
const GUTTER_CLUSTER_LEFT_PX: f32 = BLOCK_GUTTER_WIDTH_PX - GUTTER_CLUSTER_WIDTH_PX;

pub fn render_block_gutter(
    theme: GuiTheme,
    visible: bool,
    action_active: bool,
    control_top_px: f32,
    height_px: f32,
    _on_add: Option<GutterAddHandler>,
    on_mouse_down: Option<GutterMouseDownHandler>,
) -> AnyElement {
    let color = if action_active {
        theme.action_accent
    } else {
        theme.gutter_foreground
    };
    div()
        .w(px(BLOCK_GUTTER_WIDTH_PX))
        .h(px(height_px))
        .relative()
        .flex_shrink_0()
        .child(if visible {
            div()
                .absolute()
                .left(px(GUTTER_CLUSTER_LEFT_PX))
                .top(px(control_top_px))
                .w(px(GUTTER_CLUSTER_WIDTH_PX))
                .h(px(GUTTER_CONTROL_HEIGHT_PX))
                .flex()
                .items_center()
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

fn render_drag_button(
    theme: GuiTheme,
    color: u32,
    action_active: bool,
    on_mouse_down: Option<GutterMouseDownHandler>,
) -> AnyElement {
    div()
        .w(px(GUTTER_CONTROL_WIDTH_PX))
        .h(px(GUTTER_CONTROL_HEIGHT_PX))
        .rounded(px(4.0))
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
    const GUTTER: &[u8] = include_bytes!("../../../../assets/icons/gutter.svg");
    SvgIcon::new("block-gutter", GUTTER)
        .color(rgb(color))
        .size(px(16.0))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_dimensions_match_block_prototype_without_changing_outer_geometry() {
        assert_eq!(BLOCK_GUTTER_WIDTH_PX, 22.0);
        assert_eq!(BLOCK_GUTTER_HEIGHT_PX, 24.0);
        assert_eq!(GUTTER_CLUSTER_WIDTH_PX, 22.0);
        assert_eq!(GUTTER_CLUSTER_LEFT_PX, 0.0);
        assert_eq!(GUTTER_CONTROL_WIDTH_PX, 22.0);
        assert_eq!(GUTTER_CONTROL_HEIGHT_PX, 28.0);
        assert_eq!(GUTTER_CLUSTER_LEFT_PX + GUTTER_CLUSTER_WIDTH_PX, 22.0);
    }
}
