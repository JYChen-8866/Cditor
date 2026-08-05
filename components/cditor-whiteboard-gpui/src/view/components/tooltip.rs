use gpui::{Context, IntoElement, ParentElement, Render, SharedString, Styled, div, px, rgb};

use crate::theme::chrome;

pub(in crate::view) struct ToolTip {
    label: SharedString,
}

impl ToolTip {
    pub(in crate::view) fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl Render for ToolTip {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = chrome(cx);
        div().pt(px(8.0)).child(
            div()
                .px(px(7.0))
                .py(px(3.0))
                .rounded(px(4.0))
                .bg(rgb(c.text))
                .text_color(rgb(c.bg))
                .text_size(px(11.0))
                .child(self.label.clone()),
        )
    }
}
