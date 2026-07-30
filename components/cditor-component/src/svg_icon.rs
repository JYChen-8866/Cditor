// Adapted from gpui-component's Icon component.
// Copyright 2024-2025 Longbridge. Licensed under Apache-2.0.

use gpui::{
    AnyElement, App, Hsla, IntoElement, Pixels, RenderOnce, SharedString, Styled,
    TransformationMatrix, Window, canvas, px,
};

type SvgResolver = Box<dyn Fn(&App) -> (&'static str, &'static [u8])>;

/// An SVG icon backed by bytes embedded in the application binary.
#[derive(IntoElement)]
pub struct SvgIcon {
    source: SvgSource,
    color: Hsla,
    size: Pixels,
}

enum SvgSource {
    Fixed {
        key: &'static str,
        bytes: &'static [u8],
    },
    Dynamic(SvgResolver),
}

impl SvgIcon {
    pub fn new(key: &'static str, bytes: &'static [u8]) -> Self {
        Self {
            source: SvgSource::Fixed { key, bytes },
            color: gpui::black(),
            size: px(16.0),
        }
    }

    pub fn dynamic(resolve: impl Fn(&App) -> (&'static str, &'static [u8]) + 'static) -> Self {
        Self {
            source: SvgSource::Dynamic(Box::new(resolve)),
            color: gpui::black(),
            size: px(16.0),
        }
    }

    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = color.into();
        self
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for SvgIcon {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let source = self.source;
        let color = self.color;
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let (key, bytes) = match &source {
                    SvgSource::Fixed { key, bytes } => (*key, *bytes),
                    SvgSource::Dynamic(resolve) => resolve(cx),
                };
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
        .size(self.size)
    }
}

impl From<SvgIcon> for AnyElement {
    fn from(icon: SvgIcon) -> Self {
        icon.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_svg_icon_preserves_source_and_defaults_to_toolbar_size() {
        let icon = SvgIcon::new("test-icon", b"<svg/>");
        assert_eq!(icon.size, px(16.0));
        let SvgSource::Fixed { key, bytes } = icon.source else {
            panic!("new creates a fixed embedded SVG source");
        };
        assert_eq!(key, "test-icon");
        assert_eq!(bytes, b"<svg/>");
    }
}
