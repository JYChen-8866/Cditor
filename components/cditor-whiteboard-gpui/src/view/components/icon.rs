use gpui::{AnyElement, Hsla, IntoElement, SharedString, Styled, TransformationMatrix, canvas, px};

pub(in crate::view) fn svg_icon(
    key: &'static str,
    bytes: &'static [u8],
    color: Hsla,
    size: f32,
) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let _ = window.paint_svg(
                bounds,
                SharedString::from(key),
                Some(bytes),
                TransformationMatrix::default(),
                color,
                cx,
            );
        },
    )
    .size(px(size))
    .into_any_element()
}
