use gpui::{Context, IntoElement, ParentElement, Render, SharedString, Styled, div, px, rgb};

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
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().pt(px(8.0)).child(
            div()
                .px(px(7.0))
                .py(px(3.0))
                .rounded(px(4.0))
                .bg(rgb(0x111827))
                .text_color(rgb(0xffffff))
                .text_size(px(11.0))
                .child(self.label.clone()),
        )
    }
}
