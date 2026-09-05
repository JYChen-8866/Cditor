//! Scroll container with configurable top and bottom edge masks.

use gpui::{
    AnyElement, App, ElementId, IntoElement, Pixels, RenderOnce, Rgba, Styled, Window, div,
    prelude::*, px, rgb,
};

/// A reusable vertical scroll viewport with subtle edge affordances.
#[derive(IntoElement)]
pub struct ScrollableMask {
    id: ElementId,
    child: AnyElement,
    height: Option<Pixels>,
    fade: Pixels,
    mask_color: Rgba,
    mask_opacity: f32,
}

impl ScrollableMask {
    /// Creates a scrollable mask with a stable GPUI element id.
    pub fn new(id: impl Into<ElementId>, child: impl IntoElement, mask_color: u32) -> Self {
        Self {
            id: id.into(),
            child: child.into_any_element(),
            height: None,
            fade: px(18.0),
            mask_color: rgb(mask_color),
            mask_opacity: 0.62,
        }
    }

    /// Sets the viewport height.
    pub fn height(mut self, height: impl Into<Pixels>) -> Self {
        self.height = Some(height.into());
        self
    }

    /// Sets the extent of each edge mask.
    pub fn fade(mut self, fade: impl Into<Pixels>) -> Self {
        self.fade = fade.into().max(px(0.0));
        self
    }

    /// Sets mask opacity in the inclusive `0..=1` range.
    pub fn mask_opacity(mut self, opacity: f32) -> Self {
        self.mask_opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

impl RenderOnce for ScrollableMask {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mask_color = self.mask_color.opacity(self.mask_opacity);
        div()
            .relative()
            .overflow_hidden()
            .when_some(self.height, |container, height| container.h(height))
            .child(
                div()
                    .id(self.id)
                    .size_full()
                    .overflow_y_scroll()
                    .child(self.child),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(self.fade)
                    .bg(mask_color),
            )
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(self.fade)
                    .bg(mask_color),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_clamps_fade_and_opacity_without_changing_height() {
        let mask = ScrollableMask::new("test-mask", div(), 0xffffff)
            .height(px(120.0))
            .fade(px(-12.0))
            .mask_opacity(2.0);

        assert_eq!(mask.height, Some(px(120.0)));
        assert_eq!(mask.fade, px(0.0));
        assert_eq!(mask.mask_opacity, 1.0);
    }
}
