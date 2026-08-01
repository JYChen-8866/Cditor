// Adapted from gpui-component's Icon component.
// Copyright 2024-2025 Longbridge. Licensed under Apache-2.0.

use gpui::{
    AnyElement, App, Corners, Hsla, IntoElement, Pixels, RenderImage, RenderOnce, SharedString,
    Styled, TransformationMatrix, Window, canvas, px,
};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

type SvgResolver = Box<dyn Fn(&App) -> (&'static str, &'static [u8])>;

/// An SVG icon backed by bytes embedded in the application binary.
#[derive(IntoElement)]
pub struct SvgIcon {
    source: SvgSource,
    color: Hsla,
    size: Pixels,
    colored: bool,
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
            colored: false,
        }
    }

    pub fn dynamic(resolve: impl Fn(&App) -> (&'static str, &'static [u8]) + 'static) -> Self {
        Self {
            source: SvgSource::Dynamic(Box::new(resolve)),
            color: gpui::black(),
            size: px(16.0),
            colored: false,
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

    /// Renders the SVG with its original colors instead of tinting it with a
    /// single color.
    pub fn colored(mut self) -> Self {
        self.colored = true;
        self
    }
}

impl RenderOnce for SvgIcon {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let source = self.source;
        let color = self.color;
        let colored = self.colored;
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let (key, bytes) = match &source {
                    SvgSource::Fixed { key, bytes } => (*key, *bytes),
                    SvgSource::Dynamic(resolve) => resolve(cx),
                };
                if colored {
                    match colored_svg_image(key, bytes, cx) {
                        Some(image) => {
                            let _ = window.paint_image(bounds, Corners::default(), image, 0, false);
                        }
                        None => {
                            let _ = window.paint_svg(
                                bounds,
                                SharedString::from(key),
                                Some(bytes),
                                TransformationMatrix::default(),
                                color,
                                cx,
                            );
                        }
                    }
                } else {
                    let _ = window.paint_svg(
                        bounds,
                        SharedString::from(key),
                        Some(bytes),
                        TransformationMatrix::default(),
                        color,
                        cx,
                    );
                }
            },
        )
        .size(self.size)
    }
}

fn colored_svg_image(
    key: &'static str,
    bytes: &'static [u8],
    cx: &App,
) -> Option<Arc<RenderImage>> {
    let mut cache = COLORED_SVG_CACHE.lock().ok()?;
    if let Some(image) = cache.get(key) {
        return Some(image.clone());
    }
    let image = cx.svg_renderer().render_single_frame(bytes, 1.0).ok()?;
    cache.insert(key, image.clone());
    Some(image)
}

static COLORED_SVG_CACHE: LazyLock<Mutex<HashMap<&'static str, Arc<RenderImage>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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
        assert!(!icon.colored);
        let SvgSource::Fixed { key, bytes } = icon.source else {
            panic!("new creates a fixed embedded SVG source");
        };
        assert_eq!(key, "test-icon");
        assert_eq!(bytes, b"<svg/>");
    }

    #[test]
    fn colored_mode_keeps_the_original_svg_colors() {
        let icon = SvgIcon::new("brand-icon", b"<svg/>").colored();
        assert!(icon.colored);
        assert_eq!(icon.size, px(16.0));
    }
}
