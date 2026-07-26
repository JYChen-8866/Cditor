use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use drafftink_core::shapes::{Image, ShapeId, ShapeTrait};
use gpui::{Image as GpuiImage, ImageFormat};
use image::{ImageEncoder, Rgba, RgbaImage};
use kurbo::{Affine, Point, Rect};

use super::plan::{ImagePaint, PaintCommand, PaintKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImageCacheKey {
    bytes_len: usize,
    source_width: u32,
    source_height: u32,
    rotation_bits: u64,
    opacity_bits: u64,
}

struct CachedImage {
    key: ImageCacheKey,
    image: Arc<GpuiImage>,
}

#[derive(Default)]
pub(crate) struct ImagePaintEngine {
    cache: HashMap<ShapeId, CachedImage>,
    touched: HashSet<ShapeId>,
}

impl ImagePaintEngine {
    pub(crate) fn clear(&mut self) {
        self.cache.clear();
        self.touched.clear();
    }

    pub(crate) fn begin_frame(&mut self) {
        self.touched.clear();
    }

    pub(crate) fn finish_frame(&mut self) {
        self.cache.retain(|id, _| self.touched.contains(id));
    }

    pub(super) fn command(&mut self, image: &Image, camera: Affine) -> Option<PaintCommand> {
        self.touched.insert(image.id());
        let key = ImageCacheKey {
            bytes_len: image.data_base64.len(),
            source_width: image.source_width,
            source_height: image.source_height,
            rotation_bits: image.rotation.to_bits(),
            opacity_bits: image.style.opacity.to_bits(),
        };
        let cached = self
            .cache
            .get(&image.id())
            .filter(|cached| cached.key == key)
            .map(|cached| cached.image.clone());
        let raster = match cached {
            Some(raster) => raster,
            None => {
                let raster = build_raster(image)?;
                self.cache.insert(
                    image.id(),
                    CachedImage {
                        key,
                        image: raster.clone(),
                    },
                );
                raster
            }
        };
        Some(PaintCommand {
            path: kurbo::BezPath::new(),
            color: 0xffffffff,
            kind: PaintKind::Image(ImagePaint {
                image: raster,
                bounds: rotated_screen_bounds(image, camera),
            }),
        })
    }
}

fn build_raster(image: &Image) -> Option<Arc<GpuiImage>> {
    let bytes = image.data()?;
    if image.rotation.abs() <= 0.001 && image.style.opacity >= 0.999 {
        let format = match image.format {
            drafftink_core::shapes::ImageFormat::Png => ImageFormat::Png,
            drafftink_core::shapes::ImageFormat::Jpeg => ImageFormat::Jpeg,
            drafftink_core::shapes::ImageFormat::WebP => ImageFormat::Webp,
        };
        return Some(Arc::new(GpuiImage::from_bytes(format, bytes)));
    }

    let source = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let transformed = rotate_with_opacity(&source, image.rotation, image.style.opacity);
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            transformed.as_raw(),
            transformed.width(),
            transformed.height(),
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(Arc::new(GpuiImage::from_bytes(ImageFormat::Png, png)))
}

fn rotate_with_opacity(source: &RgbaImage, angle: f64, opacity: f64) -> RgbaImage {
    let cosine = angle.cos();
    let sine = angle.sin();
    let source_width = source.width() as f64;
    let source_height = source.height() as f64;
    let target_width = (source_width * cosine.abs() + source_height * sine.abs() - 1e-9)
        .ceil()
        .max(1.0) as u32;
    let target_height = (source_width * sine.abs() + source_height * cosine.abs() - 1e-9)
        .ceil()
        .max(1.0) as u32;
    let source_center = ((source_width - 1.0) / 2.0, (source_height - 1.0) / 2.0);
    let target_center = (
        (target_width as f64 - 1.0) / 2.0,
        (target_height as f64 - 1.0) / 2.0,
    );
    let alpha_scale = opacity.clamp(0.0, 1.0);
    RgbaImage::from_fn(target_width, target_height, |x, y| {
        let dx = x as f64 - target_center.0;
        let dy = y as f64 - target_center.1;
        let source_x = cosine * dx + sine * dy + source_center.0;
        let source_y = -sine * dx + cosine * dy + source_center.1;
        if source_x < 0.0 || source_y < 0.0 || source_x >= source_width || source_y >= source_height
        {
            return Rgba([0, 0, 0, 0]);
        }
        let mut pixel = *source.get_pixel(source_x.round() as u32, source_y.round() as u32);
        pixel.0[3] = (f64::from(pixel.0[3]) * alpha_scale).round() as u8;
        pixel
    })
}

fn rotated_screen_bounds(image: &Image, camera: Affine) -> Rect {
    let rect = image.as_rect();
    let transform = camera * Affine::rotate_about(image.rotation, rect.center());
    [
        Point::new(rect.x0, rect.y0),
        Point::new(rect.x1, rect.y0),
        Point::new(rect.x1, rect.y1),
        Point::new(rect.x0, rect.y1),
    ]
    .map(|point| transform * point)
    .into_iter()
    .fold(None, |bounds: Option<Rect>, point| {
        Some(match bounds {
            None => Rect::new(point.x, point.y, point.x, point.y),
            Some(bounds) => Rect::new(
                bounds.x0.min(point.x),
                bounds.y0.min(point.y),
                bounds.x1.max(point.x),
                bounds.y1.max(point.y),
            ),
        })
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotated_raster_expands_bounds_and_applies_opacity() {
        let source = RgbaImage::from_pixel(4, 2, Rgba([10, 20, 30, 200]));
        let result = rotate_with_opacity(&source, std::f64::consts::FRAC_PI_2, 0.5);
        assert_eq!((result.width(), result.height()), (2, 4));
        assert!(result.pixels().any(|pixel| pixel.0[3] == 100));
    }
}
