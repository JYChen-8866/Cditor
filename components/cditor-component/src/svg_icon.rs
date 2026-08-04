// Adapted from gpui-component's Icon component.
// Copyright 2024-2025 Longbridge. Licensed under Apache-2.0.

use gpui::{
    AnyElement, App, Corners, Hsla, IntoElement, Pixels, RenderImage, RenderOnce, SharedString,
    Styled, TransformationMatrix, Window, canvas, px,
};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

const COLORED_SVG_MAX_ENTRIES: usize = 128;
const COLORED_SVG_MAX_DECODED_BYTES: usize = 8 * 1024 * 1024;

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
    cx: &mut App,
) -> Option<Arc<RenderImage>> {
    if let Some(image) = COLORED_SVG_CACHE.lock().ok()?.get(key) {
        return Some(image);
    }
    let image = cx.svg_renderer().render_single_frame(bytes, 1.0).ok()?;
    let retired = COLORED_SVG_CACHE.lock().ok()?.insert(key, image.clone());
    if !retired.is_empty() {
        cx.defer(move |cx| {
            for image in retired {
                cx.drop_image(image, None);
            }
        });
    }
    Some(image)
}

struct CachedSvgImage {
    image: Arc<RenderImage>,
    decoded_bytes: usize,
    last_access: u64,
}

struct ColoredSvgCache {
    entries: HashMap<&'static str, CachedSvgImage>,
    access_clock: u64,
    decoded_bytes: usize,
    max_entries: usize,
    max_decoded_bytes: usize,
}

impl ColoredSvgCache {
    fn new(max_entries: usize, max_decoded_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            access_clock: 0,
            decoded_bytes: 0,
            max_entries: max_entries.max(1),
            max_decoded_bytes: max_decoded_bytes.max(1),
        }
    }

    fn get(&mut self, key: &'static str) -> Option<Arc<RenderImage>> {
        self.access_clock = self.access_clock.saturating_add(1);
        let cached = self.entries.get_mut(key)?;
        cached.last_access = self.access_clock;
        Some(cached.image.clone())
    }

    fn insert(&mut self, key: &'static str, image: Arc<RenderImage>) -> Vec<Arc<RenderImage>> {
        self.access_clock = self.access_clock.saturating_add(1);
        let decoded_bytes = decoded_image_bytes(&image);
        let mut retired = Vec::new();
        if let Some(previous) = self.entries.insert(
            key,
            CachedSvgImage {
                image,
                decoded_bytes,
                last_access: self.access_clock,
            },
        ) {
            self.decoded_bytes = self.decoded_bytes.saturating_sub(previous.decoded_bytes);
            self.push_if_uncached(&mut retired, previous.image);
        }
        self.decoded_bytes = self.decoded_bytes.saturating_add(decoded_bytes);
        while self.entries.len() > self.max_entries || self.decoded_bytes > self.max_decoded_bytes {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, cached)| cached.last_access)
                .map(|(key, _)| *key)
            else {
                break;
            };
            let Some(evicted) = self.entries.remove(oldest) else {
                continue;
            };
            self.decoded_bytes = self.decoded_bytes.saturating_sub(evicted.decoded_bytes);
            self.push_if_uncached(&mut retired, evicted.image);
        }
        retired
    }

    fn push_if_uncached(&self, retired: &mut Vec<Arc<RenderImage>>, image: Arc<RenderImage>) {
        if self
            .entries
            .values()
            .any(|cached| Arc::ptr_eq(&cached.image, &image))
            || retired.iter().any(|retired| Arc::ptr_eq(retired, &image))
        {
            return;
        }
        retired.push(image);
    }
}

fn decoded_image_bytes(image: &RenderImage) -> usize {
    (0..image.frame_count()).fold(0usize, |bytes, frame_index| {
        let frame = image.size(frame_index);
        let width = usize::try_from(frame.width.0.max(0)).unwrap_or(usize::MAX);
        let height = usize::try_from(frame.height.0.max(0)).unwrap_or(usize::MAX);
        bytes.saturating_add(width.saturating_mul(height).saturating_mul(4))
    })
}

static COLORED_SVG_CACHE: LazyLock<Mutex<ColoredSvgCache>> = LazyLock::new(|| {
    Mutex::new(ColoredSvgCache::new(
        COLORED_SVG_MAX_ENTRIES,
        COLORED_SVG_MAX_DECODED_BYTES,
    ))
});

impl From<SvgIcon> for AnyElement {
    fn from(icon: SvgIcon) -> Self {
        icon.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(width: u32, height: u32) -> Arc<RenderImage> {
        Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(width, height),
        )]))
    }

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

    #[test]
    fn colored_svg_cache_evicts_lru_and_returns_its_atlas_resource() {
        let mut cache = ColoredSvgCache::new(2, 64);
        let first = test_image(2, 2);
        let second = test_image(2, 2);
        let third = test_image(3, 3);

        assert!(cache.insert("first", first.clone()).is_empty());
        assert!(cache.insert("second", second.clone()).is_empty());
        assert!(Arc::ptr_eq(
            &cache.get("first").expect("first remains cached"),
            &first
        ));
        let retired = cache.insert("third", third);

        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &second));
        assert!(cache.entries.contains_key("first"));
        assert!(cache.entries.contains_key("third"));
        assert!(cache.entries.len() <= 2);
        assert!(cache.decoded_bytes <= 64);
    }
}
