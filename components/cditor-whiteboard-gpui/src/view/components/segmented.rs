use gpui::{
    Div, ElementId, InteractiveElement, ParentElement, Stateful, Styled, div,
    prelude::FluentBuilder, px, rgb,
};

use crate::theme::WhiteboardChrome;

pub(in crate::view) fn segment(
    id: impl Into<ElementId>,
    label: &'static str,
    selected: bool,
    c: WhiteboardChrome,
) -> Stateful<Div> {
    div()
        .id(id)
        .min_w(px(30.0))
        .h(px(24.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .cursor_pointer()
        .text_size(px(10.0))
        .text_color(if selected {
            rgb(c.on_accent)
        } else {
            rgb(c.text)
        })
        .when(selected, |button| button.bg(rgb(c.accent)))
        .when(!selected, |button| {
            button
                .bg(rgb(c.bg_strong))
                .hover(move |style| style.bg(rgb(c.hover)))
        })
        .child(label)
}
