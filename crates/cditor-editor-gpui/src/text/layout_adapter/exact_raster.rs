use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fmt,
    sync::Arc,
};

use cditor_text::{FontInstanceKey, TextPaintFont};
use gpui::{App, Bounds, Corners, Pixels, Point, RenderImage, Window, point, px, size};
use image::{Frame, RgbaImage};
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, StrikeWith, image::Content},
    zeno::{Angle, Format, Transform, Vector},
};

const EXACT_RASTER_POLICY_VERSION: u16 = 2;
// Every entry owns a standalone RenderImage. The pixel buffer is usually tiny,
// but graphics backends can reserve a page-sized texture allocation per image;
// a 4K-entry pixel-only budget therefore reached hundreds of MiB on Windows.
// Keep enough glyphs for the visible working set while bounding GPU resources.
const EXACT_RASTER_MAX_ENTRIES: usize = 512;
const EXACT_RASTER_MAX_BYTES: usize = 16 * 1024 * 1024;
const SUBPIXEL_VARIANTS_X: f32 = 4.0;
const SUBPIXEL_VARIANTS_Y: f32 = 1.0;

thread_local! {
    static SCALE_CONTEXT: RefCell<ScaleContext> = RefCell::new(ScaleContext::new());
    static RASTER_CACHE: RefCell<ExactRasterCache> =
        RefCell::new(ExactRasterCache::new(EXACT_RASTER_MAX_ENTRIES, EXACT_RASTER_MAX_BYTES));
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ExactRasterPaintResult {
    pub painted: bool,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ExactRasterError {
    InvalidFaceIndex(u32),
    InvalidGlyphId(u32),
    RasterizationFailed(u32),
    InvalidImageBuffer { expected: usize, actual: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactRasterErrorKind {
    InvalidFaceIndex,
    InvalidGlyphId,
    RasterizationFailed,
    InvalidImageBuffer,
}

impl ExactRasterError {
    pub(crate) fn kind(&self) -> ExactRasterErrorKind {
        match self {
            Self::InvalidFaceIndex(_) => ExactRasterErrorKind::InvalidFaceIndex,
            Self::InvalidGlyphId(_) => ExactRasterErrorKind::InvalidGlyphId,
            Self::RasterizationFailed(_) => ExactRasterErrorKind::RasterizationFailed,
            Self::InvalidImageBuffer { .. } => ExactRasterErrorKind::InvalidImageBuffer,
        }
    }
}

impl fmt::Display for ExactRasterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFaceIndex(index) => write!(formatter, "invalid font face index {index}"),
            Self::InvalidGlyphId(id) => write!(formatter, "glyph id {id} exceeds u16"),
            Self::RasterizationFailed(id) => write!(formatter, "failed to rasterize glyph {id}"),
            Self::InvalidImageBuffer { expected, actual } => write!(
                formatter,
                "invalid raster buffer length: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for ExactRasterError {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExactRasterCacheStats {
    pub entries: usize,
    pub estimated_bytes: usize,
    pub max_entries: usize,
    pub max_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

#[derive(Default)]
pub(crate) struct ExactRasterCacheTrimResult {
    pub(crate) evicted_entries: usize,
    pub(crate) evicted_estimated_bytes: usize,
    pub(crate) remaining_entries: usize,
    pub(crate) remaining_estimated_bytes: usize,
    pub(crate) retired_images: Vec<Arc<RenderImage>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ExactRasterKey {
    font: FontInstanceKey,
    glyph_id: u32,
    device_font_size_bits: u32,
    subpixel_x: u8,
    subpixel_y: u8,
    foreground: u32,
    color: bool,
    policy_version: u16,
}

#[derive(Clone)]
enum ExactRasterCacheValue {
    Glyph(Arc<ExactRasterGlyph>),
    Empty,
}

impl ExactRasterCacheValue {
    fn estimated_bytes(&self) -> usize {
        match self {
            Self::Glyph(glyph) => glyph.estimated_bytes,
            Self::Empty => 1,
        }
    }

    fn image(&self) -> Option<&Arc<RenderImage>> {
        match self {
            Self::Glyph(glyph) => Some(&glyph.image),
            Self::Empty => None,
        }
    }
}

struct ExactRasterGlyph {
    image: Arc<RenderImage>,
    placement: RasterPlacement,
    estimated_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RasterPlacement {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

struct ExactRasterCache {
    entries: HashMap<ExactRasterKey, ExactRasterCacheValue>,
    order: VecDeque<ExactRasterKey>,
    max_entries: usize,
    max_bytes: usize,
    estimated_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl ExactRasterCache {
    fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            max_entries: max_entries.max(1),
            max_bytes: max_bytes.max(1),
            estimated_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    fn get(&mut self, key: &ExactRasterKey) -> Option<ExactRasterCacheValue> {
        let value = self.entries.get(key)?.clone();
        self.hits = self.hits.saturating_add(1);
        self.touch(key);
        Some(value)
    }

    fn record_miss(&mut self) {
        self.misses = self.misses.saturating_add(1);
    }

    fn insert(
        &mut self,
        key: ExactRasterKey,
        value: ExactRasterCacheValue,
    ) -> Vec<Arc<RenderImage>> {
        let mut retired = Vec::new();
        let value_bytes = value.estimated_bytes();
        if let Some(previous) = self.entries.insert(key.clone(), value) {
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_sub(previous.estimated_bytes());
            self.push_if_uncached(&mut retired, previous.image());
        }
        self.estimated_bytes = self.estimated_bytes.saturating_add(value_bytes);
        self.touch(&key);
        while self.entries.len() > self.max_entries || self.estimated_bytes > self.max_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            let Some(evicted) = self.entries.remove(&oldest) else {
                continue;
            };
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_sub(evicted.estimated_bytes());
            self.evictions = self.evictions.saturating_add(1);
            self.push_if_uncached(&mut retired, evicted.image());
        }
        retired
    }

    fn touch(&mut self, key: &ExactRasterKey) {
        if let Some(index) = self.order.iter().position(|candidate| candidate == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.clone());
    }

    fn stats(&self) -> ExactRasterCacheStats {
        ExactRasterCacheStats {
            entries: self.entries.len(),
            estimated_bytes: self.estimated_bytes,
            max_entries: self.max_entries,
            max_bytes: self.max_bytes,
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
        }
    }

    fn apply_memory_pressure(
        &mut self,
        pressure: crate::memory_pressure::CditorMemoryPressure,
    ) -> ExactRasterCacheTrimResult {
        let (target_entries, target_bytes) = match pressure {
            crate::memory_pressure::CditorMemoryPressure::Normal => {
                (self.max_entries, self.max_bytes)
            }
            crate::memory_pressure::CditorMemoryPressure::Warning => {
                (self.max_entries / 2, self.max_bytes / 2)
            }
            crate::memory_pressure::CditorMemoryPressure::Critical => (0, 0),
        };
        self.trim_to(target_entries, target_bytes)
    }

    fn trim_to(
        &mut self,
        target_entries: usize,
        target_bytes: usize,
    ) -> ExactRasterCacheTrimResult {
        let before_entries = self.entries.len();
        let before_bytes = self.estimated_bytes;
        let mut retired_images = Vec::new();
        while self.entries.len() > target_entries || self.estimated_bytes > target_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            let Some(evicted) = self.entries.remove(&oldest) else {
                continue;
            };
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_sub(evicted.estimated_bytes());
            self.evictions = self.evictions.saturating_add(1);
            self.push_if_uncached(&mut retired_images, evicted.image());
        }
        ExactRasterCacheTrimResult {
            evicted_entries: before_entries.saturating_sub(self.entries.len()),
            evicted_estimated_bytes: before_bytes.saturating_sub(self.estimated_bytes),
            remaining_entries: self.entries.len(),
            remaining_estimated_bytes: self.estimated_bytes,
            retired_images,
        }
    }

    fn push_if_uncached(
        &self,
        retired: &mut Vec<Arc<RenderImage>>,
        candidate: Option<&Arc<RenderImage>>,
    ) {
        let Some(candidate) = candidate else {
            return;
        };
        if self.entries.values().any(|value| {
            value
                .image()
                .is_some_and(|cached| Arc::ptr_eq(cached, candidate))
        }) || retired
            .iter()
            .any(|retired| Arc::ptr_eq(retired, candidate))
        {
            return;
        }
        retired.push(candidate.clone());
    }
}

pub(super) fn paint_exact_glyph(
    font: &TextPaintFont,
    glyph_id: u32,
    color_glyph: bool,
    font_size: f32,
    foreground: u32,
    baseline_origin: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> Result<ExactRasterPaintResult, ExactRasterError> {
    let scale = window.scale_factor().max(f32::EPSILON);
    let device_origin_x = f32::from(baseline_origin.x) * scale;
    let device_origin_y = f32::from(baseline_origin.y) * scale;
    let quantized_x = quantize_device_origin(device_origin_x, SUBPIXEL_VARIANTS_X);
    let quantized_y = quantize_device_origin(device_origin_y, SUBPIXEL_VARIANTS_Y);
    let key = ExactRasterKey {
        font: font.instance_key().clone(),
        glyph_id,
        device_font_size_bits: (font_size * scale).to_bits(),
        subpixel_x: quantized_x.subpixel,
        subpixel_y: quantized_y.subpixel,
        foreground,
        color: color_glyph,
        policy_version: EXACT_RASTER_POLICY_VERSION,
    };
    let (value, cache_hit, retired) = cached_or_rasterize(
        key,
        RasterFontSource {
            data: font.data(),
            blob_id: font.blob_id(),
            face_index: font.face_index(),
            instance: font.instance_key(),
        },
    )?;
    retire_images_after_paint(retired, cx);
    let ExactRasterCacheValue::Glyph(glyph) = value else {
        return Ok(ExactRasterPaintResult {
            painted: false,
            cache_hit,
        });
    };

    let bounds = Bounds::new(
        point(
            px((quantized_x.integer + glyph.placement.left) as f32 / scale),
            px((quantized_y.integer - glyph.placement.top) as f32 / scale),
        ),
        size(
            px(glyph.placement.width as f32 / scale),
            px(glyph.placement.height as f32 / scale),
        ),
    );
    window
        .paint_image(
            bounds,
            bounds,
            Corners::default(),
            glyph.image.clone(),
            0,
            false,
        )
        .map_err(|_| ExactRasterError::RasterizationFailed(glyph_id))?;
    Ok(ExactRasterPaintResult {
        painted: true,
        cache_hit,
    })
}

fn cached_or_rasterize(
    key: ExactRasterKey,
    source: RasterFontSource<'_>,
) -> Result<(ExactRasterCacheValue, bool, Vec<Arc<RenderImage>>), ExactRasterError> {
    if let Some(value) = RASTER_CACHE.with(|cache| cache.borrow_mut().get(&key)) {
        return Ok((value, true, Vec::new()));
    }
    RASTER_CACHE.with(|cache| cache.borrow_mut().record_miss());

    let value = SCALE_CONTEXT
        .with(|context| rasterize_uncached(&key, source, &mut context.borrow_mut()))?;
    let retired = RASTER_CACHE.with(|cache| cache.borrow_mut().insert(key, value.clone()));
    Ok((value, false, retired))
}

fn retire_images_after_paint(images: Vec<Arc<RenderImage>>, cx: &mut App) {
    if images.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for image in images {
            cx.drop_image(image, None);
        }
    });
}

#[derive(Clone, Copy)]
struct RasterFontSource<'a> {
    data: &'a [u8],
    blob_id: u64,
    face_index: u32,
    instance: &'a FontInstanceKey,
}

fn rasterize_uncached(
    key: &ExactRasterKey,
    source: RasterFontSource<'_>,
    context: &mut ScaleContext,
) -> Result<ExactRasterCacheValue, ExactRasterError> {
    let font = FontRef::from_index(source.data, source.face_index as usize)
        .ok_or(ExactRasterError::InvalidFaceIndex(source.face_index))?;
    let glyph_id =
        u16::try_from(key.glyph_id).map_err(|_| ExactRasterError::InvalidGlyphId(key.glyph_id))?;

    let mut scaler = context
        .builder_with_id(font, [source.blob_id, u64::from(source.face_index)])
        .size(f32::from_bits(key.device_font_size_bits))
        .hint(true)
        .normalized_coords(source.instance.normalized_coords().iter())
        .build();
    let sources = if key.color {
        [
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Bitmap(StrikeWith::BestFit),
            Source::Outline,
        ]
    } else {
        [
            Source::Bitmap(StrikeWith::ExactSize),
            Source::Outline,
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
        ]
    };
    let mut renderer = Render::new(&sources);
    renderer
        .format(Format::Alpha)
        .offset(Vector::new(
            f32::from(key.subpixel_x) / SUBPIXEL_VARIANTS_X,
            f32::from(key.subpixel_y) / SUBPIXEL_VARIANTS_Y,
        ))
        .default_color(foreground_rgba(key.foreground));
    let embolden_strength = (f32::from_bits(key.device_font_size_bits) * 0.025).max(0.5);
    if source.instance.synthesis().embolden() {
        renderer.embolden(embolden_strength);
    }
    if let Some(skew) = source.instance.synthesis().skew() {
        renderer.transform(Some(Transform::skew(
            Angle::from_degrees(skew),
            Angle::ZERO,
        )));
    }

    let Some(mut image) = renderer.render(&mut scaler, glyph_id) else {
        return if glyph_is_known_blank(font, glyph_id) {
            Ok(ExactRasterCacheValue::Empty)
        } else {
            Err(ExactRasterError::RasterizationFailed(key.glyph_id))
        };
    };
    if source.instance.synthesis().embolden() && image.content == Content::Color {
        embolden_color_image(&mut image, embolden_strength);
    }
    swash_image_to_cache_value(image, key.foreground)
}

fn embolden_color_image(image: &mut swash::scale::image::Image, strength: f32) {
    let radius = strength.ceil().clamp(1.0, 8.0) as u32;
    let width = image.placement.width;
    let height = image.placement.height;
    if width == 0 || height == 0 || image.data.len() != width as usize * height as usize * 4 {
        return;
    }

    let target_width = width + radius;
    let target_height = height + radius;
    let mut target = vec![0; target_width as usize * target_height as usize * 4];
    for source_y in 0..height {
        for source_x in 0..width {
            let source_index = ((source_y * width + source_x) * 4) as usize;
            let source_pixel = &image.data[source_index..source_index + 4];
            if source_pixel[3] == 0 {
                continue;
            }
            for offset_y in 0..=radius {
                for offset_x in 0..=radius {
                    let target_x = source_x + offset_x;
                    let target_y = source_y + radius - offset_y;
                    let target_index = ((target_y * target_width + target_x) * 4) as usize;
                    if source_pixel[3] > target[target_index + 3] {
                        target[target_index..target_index + 4].copy_from_slice(source_pixel);
                    }
                }
            }
        }
    }
    image.data = target;
    image.placement.top += radius as i32;
    image.placement.width = target_width;
    image.placement.height = target_height;
}

fn glyph_is_known_blank(font: FontRef<'_>, glyph_id: u16) -> bool {
    const BLANK_CHARACTERS: [char; 22] = [
        ' ', '\u{00a0}', '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}',
        '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}', '\u{200a}', '\u{200b}',
        '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{2060}', '\u{3000}', '\u{feff}',
    ];
    let charmap = font.charmap();
    BLANK_CHARACTERS
        .into_iter()
        .any(|character| charmap.map(character) == glyph_id)
}

fn swash_image_to_cache_value(
    image: swash::scale::image::Image,
    foreground: u32,
) -> Result<ExactRasterCacheValue, ExactRasterError> {
    let width = image.placement.width;
    let height = image.placement.height;
    if width == 0 || height == 0 {
        return Ok(ExactRasterCacheValue::Empty);
    }
    let pixel_count = width as usize * height as usize;
    let bgra = match image.content {
        Content::Mask => {
            if image.data.len() != pixel_count {
                return Err(ExactRasterError::InvalidImageBuffer {
                    expected: pixel_count,
                    actual: image.data.len(),
                });
            }
            mask_to_premultiplied_bgra(&image.data, foreground)
        }
        Content::Color | Content::SubpixelMask => {
            let expected = pixel_count * 4;
            if image.data.len() != expected {
                return Err(ExactRasterError::InvalidImageBuffer {
                    expected,
                    actual: image.data.len(),
                });
            }
            rgba_to_premultiplied_bgra(image.data)
        }
    };
    let buffer =
        RgbaImage::from_raw(width, height, bgra).ok_or(ExactRasterError::InvalidImageBuffer {
            expected: pixel_count * 4,
            actual: pixel_count * 4,
        })?;
    let estimated_bytes = buffer.len();
    Ok(ExactRasterCacheValue::Glyph(Arc::new(ExactRasterGlyph {
        image: Arc::new(RenderImage::new([Frame::new(buffer)])),
        placement: RasterPlacement {
            left: image.placement.left,
            top: image.placement.top,
            width,
            height,
        },
        estimated_bytes,
    })))
}

fn mask_to_premultiplied_bgra(mask: &[u8], foreground: u32) -> Vec<u8> {
    let [red, green, blue] = foreground_rgb(foreground);
    mask.iter()
        .flat_map(|alpha| {
            let alpha = *alpha;
            [
                premultiply(blue, alpha),
                premultiply(green, alpha),
                premultiply(red, alpha),
                alpha,
            ]
        })
        .collect()
}

fn rgba_to_premultiplied_bgra(mut rgba: Vec<u8>) -> Vec<u8> {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = pixel[3];
        let red = premultiply(pixel[0], alpha);
        let green = premultiply(pixel[1], alpha);
        let blue = premultiply(pixel[2], alpha);
        pixel.copy_from_slice(&[blue, green, red, alpha]);
    }
    rgba
}

fn premultiply(channel: u8, alpha: u8) -> u8 {
    ((u16::from(channel) * u16::from(alpha) + 127) / 255) as u8
}

fn foreground_rgba(foreground: u32) -> [u8; 4] {
    let [red, green, blue] = foreground_rgb(foreground);
    [red, green, blue, 255]
}

fn foreground_rgb(foreground: u32) -> [u8; 3] {
    [
        ((foreground >> 16) & 0xff) as u8,
        ((foreground >> 8) & 0xff) as u8,
        (foreground & 0xff) as u8,
    ]
}

#[derive(Debug, Clone, Copy)]
struct QuantizedDeviceOrigin {
    integer: i32,
    subpixel: u8,
}

fn quantize_device_origin(value: f32, variants: f32) -> QuantizedDeviceOrigin {
    let quantized = (value * variants).round() / variants;
    let integer = quantized.trunc() as i32;
    let subpixel = ((quantized - integer as f32).abs() * variants).round() as u8;
    QuantizedDeviceOrigin { integer, subpixel }
}

pub(crate) fn exact_raster_cache_stats() -> ExactRasterCacheStats {
    RASTER_CACHE.with(|cache| cache.borrow().stats())
}

pub(crate) fn trim_exact_raster_cache(
    pressure: crate::memory_pressure::CditorMemoryPressure,
) -> ExactRasterCacheTrimResult {
    RASTER_CACHE.with(|cache| cache.borrow_mut().apply_memory_pressure(pressure))
}

#[cfg(test)]
fn clear_exact_raster_cache() {
    RASTER_CACHE.with(|cache| {
        *cache.borrow_mut() =
            ExactRasterCache::new(EXACT_RASTER_MAX_ENTRIES, EXACT_RASTER_MAX_BYTES);
    });
}

#[cfg(test)]
mod tests;
