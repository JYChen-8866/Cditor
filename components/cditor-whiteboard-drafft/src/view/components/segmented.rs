use gpui::{
    Div, ElementId, InteractiveElement, ParentElement, Stateful, Styled, div,
    prelude::FluentBuilder, px, rgb,
};

pub(in crate::view) fn segment(
    id: impl Into<ElementId>,
    label: &'static str,
    selected: bool,
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
            rgb(0xffffff)
        } else {
            rgb(0x505050)
        })
        .when(selected, |button| button.bg(rgb(0x3b82f6)))
        .when(!selected, |button| {
            button
                .bg(rgb(0xf5f5f5))
                .hover(|style| style.bg(rgb(0xebebeb)))
        })
        .child(label)
}
