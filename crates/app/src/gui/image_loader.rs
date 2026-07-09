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

fn image_cache() -> &'static Mutex<HashMap<String, ImageState>> {
    static CACHE: OnceLock<Mutex<HashMap<String, ImageState>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolve a decoded image for `src`, kicking off an off-UI-thread load on first use.
///
/// Returns `Some` once the image is decoded and cached. While loading (or after a
/// failure) it returns `None`, letting the caller render a stable placeholder.
pub fn load_render_image(src: &str, cx: &mut App) -> Option<Arc<RenderImage>> {
    if src.trim().is_empty() {
        return None;
    }

    if let Some(state) = image_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(src).cloned())
    {
        return match state {
            ImageState::Ready(image) => Some(image),
            ImageState::Loading | ImageState::Failed => None,
        };
    }

    if let Ok(mut cache) = image_cache().lock() {
        cache.insert(src.to_owned(), ImageState::Loading);
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
                cache.insert(src, state);
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
}
