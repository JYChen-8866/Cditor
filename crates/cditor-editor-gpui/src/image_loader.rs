//! Async image loading and rendering for image blocks and cover-style media.
//!
//! Ported from V1 CoverImages: sources are decoded into `gpui::RenderImage`, cached
//! by source string, and painted with a custom element so `ObjectFit::Cover` can
//! use the same vertical crop positioning semantics as V1.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::{
    App, Bounds, Corners, DevicePixels, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, ObjectFit, Pixels, RenderImage, Size, Style, Window, point, px,
    relative, size,
};

#[derive(Clone)]
enum ImageState {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

const IMAGE_CACHE_MAX_ENTRIES: usize = 256;
const IMAGE_CACHE_MAX_DECODED_BYTES: usize = 128 * 1024 * 1024;

struct CachedImage {
    state: ImageState,
    decoded_bytes: usize,
    last_access: u64,
}

struct ImageCache {
    entries: HashMap<String, CachedImage>,
    access_clock: u64,
    decoded_bytes: usize,
    max_entries: usize,
    max_decoded_bytes: usize,
}

enum ImageCacheLookup {
    Existing(ImageState),
    StartLoad,
}

impl ImageCache {
    fn new(max_entries: usize, max_decoded_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            access_clock: 0,
            decoded_bytes: 0,
            max_entries: max_entries.max(1),
            max_decoded_bytes: max_decoded_bytes.max(1),
        }
    }

    fn lookup_or_start(&mut self, src: &str) -> ImageCacheLookup {
        self.access_clock = self.access_clock.saturating_add(1);
        if let Some(entry) = self.entries.get_mut(src) {
            entry.last_access = self.access_clock;
            return ImageCacheLookup::Existing(entry.state.clone());
        }
        self.entries.insert(
            src.to_owned(),
            CachedImage {
                state: ImageState::Loading,
                decoded_bytes: 0,
                last_access: self.access_clock,
            },
        );
        ImageCacheLookup::StartLoad
    }

    fn finish(&mut self, src: String, state: ImageState) {
        self.access_clock = self.access_clock.saturating_add(1);
        let decoded_bytes = match &state {
            ImageState::Ready(image) => decoded_image_bytes(image),
            ImageState::Loading | ImageState::Failed => 0,
        };
        if let Some(previous) = self.entries.insert(
            src,
            CachedImage {
                state,
                decoded_bytes,
                last_access: self.access_clock,
            },
        ) {
            self.decoded_bytes = self.decoded_bytes.saturating_sub(previous.decoded_bytes);
        }
        self.decoded_bytes = self.decoded_bytes.saturating_add(decoded_bytes);
        self.trim();
    }

    fn trim(&mut self) {
        while self.entries.len() > self.max_entries || self.decoded_bytes > self.max_decoded_bytes {
            let candidate = self
                .entries
                .iter()
                .filter(|(_, entry)| !matches!(entry.state, ImageState::Loading))
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|(src, _)| src.clone());
            let Some(candidate) = candidate else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&candidate) {
                self.decoded_bytes = self.decoded_bytes.saturating_sub(evicted.decoded_bytes);
            }
        }
    }
}

fn image_cache() -> &'static Mutex<ImageCache> {
    static CACHE: OnceLock<Mutex<ImageCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(ImageCache::new(
            IMAGE_CACHE_MAX_ENTRIES,
            IMAGE_CACHE_MAX_DECODED_BYTES,
        ))
    })
}

/// Resolve a decoded image for `src`, kicking off an off-UI-thread load on first use.
///
/// Returns `Some` once the image is decoded and cached. While loading (or after a
/// failure) it returns `None`, letting the caller render a stable placeholder.
pub fn load_render_image(src: &str, cx: &mut App) -> Option<Arc<RenderImage>> {
    if src.trim().is_empty() {
        return None;
    }

    let lookup = image_cache()
        .lock()
        .ok()
        .map(|mut cache| cache.lookup_or_start(src))?;
    if let ImageCacheLookup::Existing(state) = lookup {
        return match state {
            ImageState::Ready(image) => Some(image),
            ImageState::Loading | ImageState::Failed => None,
        };
    }

    let src = src.to_owned();
    let async_cx = cx.to_async();
    let executor = cx.background_executor().clone();
    cx.foreground_executor()
        .spawn(async move {
            let fetch_src = src.clone();
            let state = executor
                .spawn(async move {
                    fetch_image_bytes(&fetch_src)
                        .as_deref()
                        .and_then(decode_render_image)
                })
                .await
                .map_or(ImageState::Failed, ImageState::Ready);
            if let Ok(mut cache) = image_cache().lock() {
                cache.finish(src, state);
            }
            async_cx.update(App::refresh_windows);
        })
        .detach();

    None
}

fn fetch_image_bytes(src: &str) -> Option<Vec<u8>> {
    if src.starts_with("http://") || src.starts_with("https://") {
        let response = reqwest::blocking::get(src).ok()?;
        let bytes = response.bytes().ok()?;
        Some(bytes.to_vec())
    } else {
        std::fs::read(parse_local_path(src)).ok()
    }
}

fn parse_local_path(src: &str) -> PathBuf {
    let raw = src.strip_prefix("file://").unwrap_or(src);
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(raw)
}

fn decode_render_image(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    let format = image::guess_format(bytes).ok()?;
    let mut data = image::load_from_memory_with_format(bytes, format)
        .ok()?
        .into_rgba8();
    // gpui paints premultiplied BGRA; V1 swaps R/B after decoding to RGBA.
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new([image::Frame::new(data)])))
}

fn decoded_image_bytes(image: &RenderImage) -> usize {
    (0..image.frame_count()).fold(0usize, |bytes, frame_index| {
        let frame = image.size(frame_index);
        let width = usize::try_from(frame.width.0.max(0)).unwrap_or(usize::MAX);
        let height = usize::try_from(frame.height.0.max(0)).unwrap_or(usize::MAX);
        bytes.saturating_add(width.saturating_mul(height).saturating_mul(4))
    })
}

pub struct RasterImageElement {
    image: Arc<RenderImage>,
    fit: ObjectFit,
    radius: Pixels,
    cover_position_y: Option<f32>,
}

impl RasterImageElement {
    #[must_use]
    pub fn new(image: Arc<RenderImage>, fit: ObjectFit, radius: Pixels) -> Self {
        Self {
            image,
            fit,
            radius,
            cover_position_y: None,
        }
    }

    #[must_use]
    pub fn cover(image: Arc<RenderImage>, radius: Pixels, position_y: f32) -> Self {
        Self {
            image,
            fit: ObjectFit::Cover,
            radius,
            cover_position_y: Some(position_y.clamp(0.0, 1.0)),
        }
    }
}

fn positioned_cover_bounds(
    container: Bounds<Pixels>,
    image_size: Size<DevicePixels>,
    position_y: f32,
) -> Bounds<Pixels> {
    let image_width = (image_size.width.0 as f32).max(1.0);
    let image_height = (image_size.height.0 as f32).max(1.0);
    let container_width = f32::from(container.size.width).max(1.0);
    let container_height = f32::from(container.size.height).max(1.0);
    let scale = (container_width / image_width).max(container_height / image_height);
    let scaled_width = px(image_width * scale);
    let scaled_height = px(image_height * scale);
    let overflow_x = (scaled_width - container.size.width).max(px(0.0));
    let overflow_y = (scaled_height - container.size.height).max(px(0.0));

    Bounds {
        origin: point(
            container.origin.x - overflow_x / 2.0,
            container.origin.y - overflow_y * position_y.clamp(0.0, 1.0),
        ),
        size: size(scaled_width, scaled_height),
    }
}

impl IntoElement for RasterImageElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for RasterImageElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        (): &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        (): &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) {
        if self.image.frame_count() == 0 {
            return;
        }
        let image_bounds = self.cover_position_y.map_or_else(
            || self.fit.get_bounds(bounds, self.image.size(0)),
            |position_y| positioned_cover_bounds(bounds, self.image.size(0), position_y),
        );
        let corner_radii = Corners::all(self.radius).clamp_radii_for_quad_size(image_bounds.size);
        let _ = window.paint_image(image_bounds, corner_radii, self.image.clone(), 0, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_path_parser_strips_file_scheme() {
        assert_eq!(
            parse_local_path("file:///tmp/a.png"),
            PathBuf::from("/tmp/a.png")
        );
        assert_eq!(parse_local_path("/tmp/a.png"), PathBuf::from("/tmp/a.png"));
    }

    #[test]
    fn positioned_cover_uses_vertical_position() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(100.0)));
        let image_size = size(DevicePixels(100), DevicePixels(200));

        let top = positioned_cover_bounds(bounds, image_size, 0.0);
        let bottom = positioned_cover_bounds(bounds, image_size, 1.0);

        assert!(bottom.origin.y < top.origin.y);
    }

    fn test_image(width: u32, height: u32) -> Arc<RenderImage> {
        Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(width, height),
        )]))
    }

    #[test]
    fn decoded_image_cache_evicts_lru_by_bytes_and_entries() {
        let mut cache = ImageCache::new(2, 64);
        assert!(matches!(
            cache.lookup_or_start("a"),
            ImageCacheLookup::StartLoad
        ));
        cache.finish("a".to_owned(), ImageState::Ready(test_image(2, 2)));
        assert!(matches!(
            cache.lookup_or_start("b"),
            ImageCacheLookup::StartLoad
        ));
        cache.finish("b".to_owned(), ImageState::Ready(test_image(2, 2)));
        let _ = cache.lookup_or_start("a");
        assert!(matches!(
            cache.lookup_or_start("c"),
            ImageCacheLookup::StartLoad
        ));
        cache.finish("c".to_owned(), ImageState::Ready(test_image(3, 3)));

        assert!(cache.entries.contains_key("a"));
        assert!(cache.entries.contains_key("c"));
        assert!(!cache.entries.contains_key("b"));
        assert!(cache.entries.len() <= 2);
        assert!(cache.decoded_bytes <= 64);
    }

    #[test]
    fn in_flight_decode_is_never_evicted_or_dispatched_twice() {
        let mut cache = ImageCache::new(1, 1);
        assert!(matches!(
            cache.lookup_or_start("loading"),
            ImageCacheLookup::StartLoad
        ));
        assert!(matches!(
            cache.lookup_or_start("loading"),
            ImageCacheLookup::Existing(ImageState::Loading)
        ));
        assert!(matches!(
            cache.lookup_or_start("second"),
            ImageCacheLookup::StartLoad
        ));
        cache.trim();

        assert!(cache.entries.contains_key("loading"));
        assert!(cache.entries.contains_key("second"));
    }
}
