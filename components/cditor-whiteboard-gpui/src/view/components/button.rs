use crate::theme::WhiteboardChrome;

use gpui::{
    CursorStyle, Div, ElementId, InteractiveElement, Stateful, Styled, div, prelude::FluentBuilder,
    px, rgb,
};

pub(in crate::view) fn icon_button(
    id: impl Into<ElementId>,
    selected: bool,
    disabled: bool,
    c: WhiteboardChrome,
) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .cursor(if disabled {
            CursorStyle::Arrow
        } else {
            CursorStyle::PointingHand
        })
        .when(selected, |button| button.bg(rgb(c.active)))
        .when(!selected && !disabled, |button| {
            button.hover(|style| style.bg(rgb(c.hover)))
        })
        .when(disabled, |button| button.opacity(0.32))
}

pub(in crate::view) fn tool_button(
    id: impl Into<ElementId>,
    selected: bool,
    c: WhiteboardChrome,
) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .cursor(CursorStyle::PointingHand)
        .when(selected, |button| button.bg(rgb(c.accent)))
        .when(!selected, |button| {
            button.hover(|style| style.bg(rgb(c.hover)))
        })
}
