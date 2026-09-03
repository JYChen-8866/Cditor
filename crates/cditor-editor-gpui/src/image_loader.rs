//! Async image loading and rendering for image blocks and cover-style media.
//!
//! Ported from V1 CoverImages: sources are decoded into `gpui::RenderImage`, cached
//! by source string, and painted with a custom element so `ObjectFit::Cover` can
//! use the same vertical crop positioning semantics as V1.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use gpui::{
    App, Bounds, Corners, DevicePixels, Element, ElementId, Entity, Global, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ObjectFit, Pixels, RenderImage, Size, Style, Window,
    point, px, relative, size,
};

use cditor_core::ids::BlockId;
use cditor_runtime::{MainThreadWorkKind, WorkCost, WorkerTaskKind};

use crate::app::main_thread_scheduler::MainThreadApplyRequest;
use crate::app::worker_admission::EditorWorkerAdmission;
use crate::editor_view::CditorV2View;

pub trait RemoteImageDataSource: Send + Sync + 'static {
    fn load(&self, url: &str) -> Result<Vec<u8>, String>;
}

const REMOTE_IMAGE_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_IMAGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const REMOTE_IMAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;
// Keep attacker-controlled or accidentally malformed URLs/paths from turning
// the process-wide cache key map into a large string store. Normal S3/object
// keys are far below this limit; oversized sources are rejected before any
// allocation or worker admission.
const MAX_IMAGE_SOURCE_BYTES: usize = 16 * 1024;
// Compressed size is not a safe proxy for decoded memory: a tiny PNG can
// expand into a multi-gigabyte bitmap. Reject pathological dimensions and
// decoder allocations before `image` creates the pixel buffer.
const MAX_DECODED_IMAGE_WIDTH: u32 = 8192;
const MAX_DECODED_IMAGE_HEIGHT: u32 = 8192;
const MAX_DECODED_IMAGE_ALLOC_BYTES: u64 = 64 * 1024 * 1024;
const DISPLAY_IMAGE_MAX_EDGE_PX: u32 = 2048;

struct BuiltinRemoteImageDataSource {
    client: reqwest::blocking::Client,
}

impl BuiltinRemoteImageDataSource {
    fn new() -> Result<Self, reqwest::Error> {
        reqwest::blocking::Client::builder()
            .connect_timeout(REMOTE_IMAGE_CONNECT_TIMEOUT)
            .timeout(REMOTE_IMAGE_REQUEST_TIMEOUT)
            .build()
            .map(|client| Self { client })
    }
}

impl RemoteImageDataSource for BuiltinRemoteImageDataSource {
    fn load(&self, url: &str) -> Result<Vec<u8>, String> {
        let response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| error.to_string())?;
        let content_length = response.content_length();
        read_remote_image_body(response, content_length)
    }
}

fn builtin_remote_image_data_source() -> Option<&'static dyn RemoteImageDataSource> {
    static SOURCE: OnceLock<Option<BuiltinRemoteImageDataSource>> = OnceLock::new();
    SOURCE
        .get_or_init(|| BuiltinRemoteImageDataSource::new().ok())
        .as_ref()
        .map(|source| source as &dyn RemoteImageDataSource)
}

struct RemoteImageDataSourceGlobal(Arc<dyn RemoteImageDataSource>);

impl Global for RemoteImageDataSourceGlobal {}

pub fn configure_remote_image_data_source(cx: &mut App, source: Arc<dyn RemoteImageDataSource>) {
    cx.set_global(RemoteImageDataSourceGlobal(source));
}

/// Captures all non-UI state needed to decode a source image for the preview
/// canvas. The source is intentionally retained instead of the original
/// decoded bitmap; preview decoding is an explicit, short-lived operation.
pub(crate) struct PreviewImageLoad {
    source: String,
    remote_source: Option<Arc<dyn RemoteImageDataSource>>,
    asset_provider: Option<Arc<dyn cditor_sdk::providers::AssetProvider>>,
}

impl PreviewImageLoad {
    pub(crate) fn capture(
        source: &str,
        asset_provider: Option<Arc<dyn cditor_sdk::providers::AssetProvider>>,
        cx: &App,
    ) -> Option<Self> {
        image_source_allowed(source).then(|| Self {
            source: source.to_owned(),
            remote_source: cx
                .try_global::<RemoteImageDataSourceGlobal>()
                .map(|source| source.0.clone()),
            asset_provider,
        })
    }

    pub(crate) async fn decode(self) -> Option<Arc<RenderImage>> {
        let bytes = fetch_image_bytes(
            &self.source,
            self.remote_source.as_deref(),
            self.asset_provider,
        )
        .await?;
        decode_preview_render_image(&bytes)
    }
}

#[derive(Clone)]
enum ImageState {
    Loading { generation: u64 },
    Ready(Arc<RenderImage>),
    Failed,
}

// RenderImage retains CPU pixels and backend texture state. Bound both the
// decoded-byte working set and the number of graphics resources process-wide.
const IMAGE_CACHE_MAX_ENTRIES: usize = 96;
const IMAGE_CACHE_MAX_DECODED_BYTES: usize = 24 * 1024 * 1024;

struct CachedImage {
    state: ImageState,
    decoded_bytes: usize,
    last_access: u64,
}

struct ImageCache {
    entries: HashMap<String, CachedImage>,
    access_clock: u64,
    load_generation: u64,
    decoded_bytes: usize,
    max_entries: usize,
    max_decoded_bytes: usize,
}

enum ImageCacheLookup {
    Existing(ImageState),
    StartLoad {
        generation: u64,
    },
    /// Every slot is still in flight. Do not insert another key until one of
    /// those requests completes; evicting a loading entry would let its late
    /// callback repopulate the cache and retain the source indefinitely.
    AtCapacity,
}

#[derive(Default)]
pub(crate) struct ImageCacheTrimResult {
    pub(crate) evicted_entries: usize,
    pub(crate) evicted_decoded_bytes: usize,
    pub(crate) invalidated_loads: usize,
    pub(crate) remaining_entries: usize,
    pub(crate) remaining_decoded_bytes: usize,
    pub(crate) retired_images: Vec<Arc<RenderImage>>,
}

impl ImageCache {
    fn new(max_entries: usize, max_decoded_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            access_clock: 0,
            load_generation: 0,
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
        if self.entries.len() >= self.max_entries
            && self
                .entries
                .values()
                .all(|entry| matches!(entry.state, ImageState::Loading { .. }))
        {
            return ImageCacheLookup::AtCapacity;
        }
        self.load_generation = self.load_generation.wrapping_add(1).max(1);
        let generation = self.load_generation;
        self.entries.insert(
            src.to_owned(),
            CachedImage {
                state: ImageState::Loading { generation },
                decoded_bytes: 0,
                last_access: self.access_clock,
            },
        );
        ImageCacheLookup::StartLoad { generation }
    }

    fn finish(&mut self, src: String, generation: u64, state: ImageState) -> Vec<Arc<RenderImage>> {
        let current_generation = self.entries.get(&src).and_then(|entry| match entry.state {
            ImageState::Loading { generation } => Some(generation),
            ImageState::Ready(_) | ImageState::Failed => None,
        });
        if current_generation != Some(generation) {
            let mut retired = Vec::new();
            self.push_if_uncached(&mut retired, &state);
            return retired;
        }

        self.access_clock = self.access_clock.saturating_add(1);
        let mut retired = Vec::new();
        let decoded_bytes = match &state {
            ImageState::Ready(image) => decoded_image_bytes(image),
            ImageState::Loading { .. } | ImageState::Failed => 0,
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
            self.push_if_uncached(&mut retired, &previous.state);
        }
        self.decoded_bytes = self.decoded_bytes.saturating_add(decoded_bytes);
        self.trim_into(&mut retired);
        retired
    }

    fn abort_start(&mut self, src: &str, generation: u64) {
        if self.entries.get(src).is_some_and(|entry| {
            matches!(
                entry.state,
                ImageState::Loading {
                    generation: current
                } if current == generation
            )
        }) {
            self.entries.remove(src);
        }
    }

    fn retry_failed(&mut self, src: &str) {
        if self
            .entries
            .get(src)
            .is_some_and(|entry| matches!(entry.state, ImageState::Failed))
        {
            self.entries.remove(src);
        }
    }

    #[cfg(test)]
    fn trim(&mut self) -> Vec<Arc<RenderImage>> {
        let mut retired = Vec::new();
        self.trim_into(&mut retired);
        retired
    }

    fn trim_into(&mut self, retired: &mut Vec<Arc<RenderImage>>) {
        while self.entries.len() > self.max_entries || self.decoded_bytes > self.max_decoded_bytes {
            let candidate = self
                .entries
                .iter()
                .filter(|(_, entry)| !matches!(entry.state, ImageState::Loading { .. }))
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|(src, _)| src.clone());
            let Some(candidate) = candidate else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&candidate) {
                self.decoded_bytes = self.decoded_bytes.saturating_sub(evicted.decoded_bytes);
                self.push_if_uncached(retired, &evicted.state);
            }
        }
    }

    fn push_if_uncached(&self, retired: &mut Vec<Arc<RenderImage>>, state: &ImageState) {
        let Some(image) = ready_image(state) else {
            return;
        };
        if self
            .entries
            .values()
            .any(|entry| ready_image(&entry.state).is_some_and(|cached| Arc::ptr_eq(cached, image)))
            || retired.iter().any(|retired| Arc::ptr_eq(retired, image))
        {
            return;
        }
        retired.push(image.clone());
    }

    fn apply_memory_pressure(
        &mut self,
        pressure: crate::memory_pressure::CditorMemoryPressure,
    ) -> ImageCacheTrimResult {
        let before_entries = self.entries.len();
        let before_decoded_bytes = self.decoded_bytes;
        let mut retired_images = Vec::new();
        let mut invalidated_loads = 0usize;

        match pressure {
            crate::memory_pressure::CditorMemoryPressure::Normal => {
                self.trim_into(&mut retired_images);
            }
            crate::memory_pressure::CditorMemoryPressure::Warning => {
                let target_entries = self.max_entries / 2;
                let target_bytes = self.max_decoded_bytes / 2;
                while self.entries.len() > target_entries || self.decoded_bytes > target_bytes {
                    let candidate = self
                        .entries
                        .iter()
                        .filter(|(_, entry)| !matches!(entry.state, ImageState::Loading { .. }))
                        .min_by_key(|(_, entry)| {
                            (
                                usize::from(matches!(entry.state, ImageState::Ready(_))),
                                entry.last_access,
                            )
                        })
                        .map(|(src, _)| src.clone());
                    let Some(candidate) = candidate else {
                        break;
                    };
                    if let Some(evicted) = self.entries.remove(&candidate) {
                        self.decoded_bytes =
                            self.decoded_bytes.saturating_sub(evicted.decoded_bytes);
                        self.push_if_uncached(&mut retired_images, &evicted.state);
                    }
                }
            }
            crate::memory_pressure::CditorMemoryPressure::Critical => {
                while let Some(src) = self.entries.keys().next().cloned() {
                    let Some(evicted) = self.entries.remove(&src) else {
                        continue;
                    };
                    invalidated_loads = invalidated_loads.saturating_add(usize::from(matches!(
                        evicted.state,
                        ImageState::Loading { .. }
                    )));
                    self.decoded_bytes = self.decoded_bytes.saturating_sub(evicted.decoded_bytes);
                    self.push_if_uncached(&mut retired_images, &evicted.state);
                }
            }
        }

        ImageCacheTrimResult {
            evicted_entries: before_entries.saturating_sub(self.entries.len()),
            evicted_decoded_bytes: before_decoded_bytes.saturating_sub(self.decoded_bytes),
            invalidated_loads,
            remaining_entries: self.entries.len(),
            remaining_decoded_bytes: self.decoded_bytes,
            retired_images,
        }
    }

    fn diagnostics(&self) -> cditor_sdk::diagnostics::ImageCacheDiagnostics {
        let mut loading_entries = 0;
        let mut decoded_entries = 0;
        let mut failed_entries = 0;
        for entry in self.entries.values() {
            match &entry.state {
                ImageState::Loading { .. } => loading_entries += 1,
                ImageState::Ready(_) => decoded_entries += 1,
                ImageState::Failed => failed_entries += 1,
            }
        }
        cditor_sdk::diagnostics::ImageCacheDiagnostics {
            tracked_entries: self.entries.len(),
            decoded_entries,
            loading_entries,
            failed_entries,
            resident_decoded_bytes: self.decoded_bytes,
            max_entries: self.max_entries,
            decoded_byte_budget: self.max_decoded_bytes,
        }
    }
}

fn ready_image(state: &ImageState) -> Option<&Arc<RenderImage>> {
    match state {
        ImageState::Ready(image) => Some(image),
        ImageState::Loading { .. } | ImageState::Failed => None,
    }
}

fn retire_images_after_effect(images: Vec<Arc<RenderImage>>, cx: &mut App) {
    if images.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for image in images {
            cx.drop_image(image, None);
        }
    });
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

/// Returns a point-in-time snapshot of the process-wide decoded image cache.
/// The cache is shared by editor instances, so callers must not add this
/// value once per editor when aggregating process-wide diagnostics.
pub(crate) fn image_cache_diagnostics() -> cditor_sdk::diagnostics::ImageCacheDiagnostics {
    let cache = image_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    cache.diagnostics()
}

pub(crate) fn trim_image_cache(
    pressure: crate::memory_pressure::CditorMemoryPressure,
) -> ImageCacheTrimResult {
    image_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .apply_memory_pressure(pressure)
}

pub(crate) fn image_load_failed(src: &str) -> bool {
    image_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.entries.get(src).map(|entry| entry.state.clone()))
        .is_some_and(|state| matches!(state, ImageState::Failed))
}

pub(crate) fn retry_image_load(src: &str) {
    if let Ok(mut cache) = image_cache().lock() {
        cache.retry_failed(src);
    }
}

/// Resolve a decoded image for `src`, kicking off an off-UI-thread load on first use.
///
/// Returns `Some` once the image is decoded and cached. While loading (or after a
/// failure) it returns `None`, letting the caller render a stable placeholder.
pub fn load_render_image(
    src: &str,
    block_id: BlockId,
    content_version: u64,
    workers: &EditorWorkerAdmission,
    asset_provider: Option<Arc<dyn cditor_sdk::providers::AssetProvider>>,
    view: Entity<CditorV2View>,
    cx: &mut App,
) -> Option<Arc<RenderImage>> {
    if !image_source_allowed(src) {
        return None;
    }

    let lookup = image_cache()
        .lock()
        .ok()
        .map(|mut cache| cache.lookup_or_start(src))?;
    let generation = match lookup {
        ImageCacheLookup::Existing(state) => {
            return match state {
                ImageState::Ready(image) => Some(image),
                ImageState::Loading { .. } | ImageState::Failed => None,
            };
        }
        ImageCacheLookup::AtCapacity => return None,
        ImageCacheLookup::StartLoad { generation } => generation,
    };

    let Some(permit) = workers.try_acquire(WorkerTaskKind::ImageDecode) else {
        if let Ok(mut cache) = image_cache().lock() {
            cache.abort_start(src, generation);
        }
        return None;
    };

    let src = src.to_owned();
    let remote_source = cx
        .try_global::<RemoteImageDataSourceGlobal>()
        .map(|source| source.0.clone());
    let async_cx = cx.to_async();
    let executor = cx.background_executor().clone();
    cx.foreground_executor()
        .spawn(async move {
            let fetch_src = src.clone();
            let state = executor
                .spawn(async move {
                    let _permit = permit;
                    fetch_image_bytes(&fetch_src, remote_source.as_deref(), asset_provider)
                        .await
                        .as_deref()
                        .and_then(decode_display_render_image)
                })
                .await
                .map_or(ImageState::Failed, ImageState::Ready);
            let cancel_src = src.clone();
            let abort_src = cancel_src.clone();
            let queued = Arc::new(AtomicBool::new(false));
            let queued_for_update = Arc::clone(&queued);
            async_cx.update(|cx| {
                view.update(cx, |view, cx| {
                    queued_for_update.store(true, Ordering::Release);
                    view.enqueue_main_thread_apply_with_cancel(
                        MainThreadApplyRequest {
                            kind: MainThreadWorkKind::ImageDecodeApply,
                            generation: content_version,
                            block_id: Some(block_id),
                            cost: WorkCost::image_decode_apply(),
                        },
                        move |_view, cx| {
                            let retired = image_cache()
                                .lock()
                                .map(|mut cache| cache.finish(src, generation, state))
                                .unwrap_or_default();
                            retire_images_after_effect(retired, cx);
                            cx.refresh_windows();
                        },
                        move || {
                            if let Ok(mut cache) = image_cache().lock() {
                                let retired =
                                    cache.finish(cancel_src, generation, ImageState::Failed);
                                debug_assert!(retired.is_empty());
                            }
                        },
                        cx,
                    );
                })
            });
            if !queued.load(Ordering::Acquire) {
                if let Ok(mut cache) = image_cache().lock() {
                    cache.abort_start(&abort_src, generation);
                }
            }
        })
        .detach();

    None
}

fn image_source_allowed(src: &str) -> bool {
    !src.trim().is_empty() && src.len() <= MAX_IMAGE_SOURCE_BYTES
}

async fn fetch_image_bytes(
    src: &str,
    remote_source: Option<&dyn RemoteImageDataSource>,
    asset_provider: Option<Arc<dyn cditor_sdk::providers::AssetProvider>>,
) -> Option<Vec<u8>> {
    if src.starts_with("http://") || src.starts_with("https://") {
        fetch_remote_image_bytes(src, remote_source, builtin_remote_image_data_source())
    } else if src.starts_with("assets/") {
        let resolved = crate::provider_io::resolve_asset(
            asset_provider?,
            cditor_core::rich_text::AssetRef::local(src),
        )
        .await
        .ok()?;
        match (resolved.bytes, resolved.local_path) {
            (Some(bytes), _) => bounded_image_bytes(bytes),
            (None, Some(path)) => read_local_image_file(&path),
            (None, None) => None,
        }
    } else {
        read_local_image_file(&parse_local_path(src))
    }
}

fn fetch_remote_image_bytes(
    src: &str,
    configured_source: Option<&dyn RemoteImageDataSource>,
    fallback_source: Option<&dyn RemoteImageDataSource>,
) -> Option<Vec<u8>> {
    configured_source
        .or(fallback_source)
        .and_then(|source| source.load(src).ok())
        .and_then(bounded_image_bytes)
}

fn bounded_image_bytes(bytes: Vec<u8>) -> Option<Vec<u8>> {
    (bytes.len() as u64 <= REMOTE_IMAGE_MAX_BYTES).then_some(bytes)
}

fn read_local_image_file(path: &std::path::Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(REMOTE_IMAGE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    bounded_image_bytes(bytes)
}

fn read_remote_image_body(
    reader: impl Read,
    content_length: Option<u64>,
) -> Result<Vec<u8>, String> {
    if content_length.is_some_and(|length| length > REMOTE_IMAGE_MAX_BYTES) {
        return Err("remote image exceeds the 32 MiB limit".to_owned());
    }
    let mut bytes = Vec::new();
    reader
        .take(REMOTE_IMAGE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > REMOTE_IMAGE_MAX_BYTES {
        return Err("remote image exceeds the 32 MiB limit".to_owned());
    }
    Ok(bytes)
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

fn decode_display_render_image(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    decode_render_image(bytes, Some(DISPLAY_IMAGE_MAX_EDGE_PX))
}

fn decode_preview_render_image(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    decode_render_image(bytes, None)
}

fn decode_render_image(bytes: &[u8], max_raster_edge: Option<u32>) -> Option<Arc<RenderImage>> {
    // Probe the header with a separate reader first. Decoder limits protect
    // codec scratch allocations, but the final RGBA conversion allocates its
    // own buffer; rejecting the pixel budget here covers that second buffer
    // before either allocation can happen.
    let dimensions = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if !decoded_dimensions_within_limits(dimensions.0, dimensions.1) {
        return None;
    }

    let mut reader = image::ImageReader::new(Cursor::new(bytes));
    reader = reader.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_IMAGE_WIDTH);
    limits.max_image_height = Some(MAX_DECODED_IMAGE_HEIGHT);
    limits.max_alloc = Some(MAX_DECODED_IMAGE_ALLOC_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let raster_dimensions = max_raster_edge
        .map(|max_edge| display_raster_dimensions(dimensions.0, dimensions.1, max_edge))
        .unwrap_or(dimensions);
    let mut data = if raster_dimensions == dimensions {
        decoded.into_rgba8()
    } else {
        decoded
            .resize_exact(
                raster_dimensions.0,
                raster_dimensions.1,
                image::imageops::FilterType::Triangle,
            )
            .into_rgba8()
    };
    // gpui paints premultiplied BGRA; V1 swaps R/B after decoding to RGBA.
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new([image::Frame::new(data)])))
}

fn display_raster_dimensions(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let max_edge = max_edge.max(1);
    let longest_edge = width.max(height);
    if longest_edge <= max_edge {
        return (width, height);
    }
    if width >= height {
        let scaled_height =
            (u64::from(height) * u64::from(max_edge) + u64::from(width) / 2) / u64::from(width);
        (max_edge, u32::try_from(scaled_height).unwrap_or(1).max(1))
    } else {
        let scaled_width =
            (u64::from(width) * u64::from(max_edge) + u64::from(height) / 2) / u64::from(height);
        (u32::try_from(scaled_width).unwrap_or(1).max(1), max_edge)
    }
}

fn decoded_dimensions_within_limits(width: u32, height: u32) -> bool {
    if width == 0
        || height == 0
        || width > MAX_DECODED_IMAGE_WIDTH
        || height > MAX_DECODED_IMAGE_HEIGHT
    {
        return false;
    }
    u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4)
        <= MAX_DECODED_IMAGE_ALLOC_BYTES
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
    trace_identity: Option<(BlockId, u64)>,
}

impl RasterImageElement {
    #[must_use]
    pub fn new(image: Arc<RenderImage>, fit: ObjectFit, radius: Pixels) -> Self {
        Self {
            image,
            fit,
            radius,
            cover_position_y: None,
            trace_identity: None,
        }
    }

    #[must_use]
    pub fn trace_image_block(mut self, block_id: BlockId, content_version: u64) -> Self {
        self.trace_identity = Some((block_id, content_version));
        self
    }

    #[must_use]
    pub fn with_cover_position_y(mut self, position_y: f32) -> Self {
        self.cover_position_y = Some(position_y.clamp(0.0, 1.0));
        self
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
        bounds: Bounds<Pixels>,
        (): &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
        if let Some((block_id, content_version)) = self.trace_identity {
            crate::diagnostics::image_resize::trace(
                "raster.prepaint",
                format_args!(
                    "block={block_id} version={content_version} bounds={bounds:?} image_size={:?} frames={}",
                    self.image.size(0),
                    self.image.frame_count(),
                ),
            );
        }
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
        let result = window.paint_image(image_bounds, corner_radii, self.image.clone(), 0, false);
        if let Some((block_id, content_version)) = self.trace_identity {
            crate::diagnostics::image_resize::trace(
                "raster.paint",
                format_args!(
                    "block={block_id} version={content_version} container={bounds:?} image_bounds={image_bounds:?} image_size={:?} result={result:?}",
                    self.image.size(0),
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRemoteSource;

    impl RemoteImageDataSource for TestRemoteSource {
        fn load(&self, url: &str) -> Result<Vec<u8>, String> {
            Ok(url.as_bytes().to_vec())
        }
    }

    struct TestFallbackSource;

    impl RemoteImageDataSource for TestFallbackSource {
        fn load(&self, _url: &str) -> Result<Vec<u8>, String> {
            Ok(b"fallback".to_vec())
        }
    }

    #[test]
    fn local_path_parser_strips_file_scheme() {
        assert_eq!(
            parse_local_path("file:///tmp/a.png"),
            PathBuf::from("/tmp/a.png")
        );
        assert_eq!(parse_local_path("/tmp/a.png"), PathBuf::from("/tmp/a.png"));
    }

    #[test]
    fn configured_remote_source_overrides_the_builtin_fallback() {
        assert_eq!(
            fetch_remote_image_bytes(
                "https://example.test/image.png",
                Some(&TestRemoteSource),
                Some(&TestFallbackSource),
            ),
            Some(b"https://example.test/image.png".to_vec())
        );
        assert_eq!(
            fetch_remote_image_bytes(
                "https://example.test/image.png",
                None,
                Some(&TestFallbackSource),
            ),
            Some(b"fallback".to_vec())
        );
        assert!(builtin_remote_image_data_source().is_some());
    }

    #[test]
    fn remote_image_body_rejects_declared_oversize_payloads() {
        let result = read_remote_image_body(
            std::io::Cursor::new(Vec::<u8>::new()),
            Some(REMOTE_IMAGE_MAX_BYTES + 1),
        );
        assert_eq!(result.unwrap_err(), "remote image exceeds the 32 MiB limit");
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::new(width, height));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn decoded_image_accepts_normal_raster() {
        let image = decode_display_render_image(&png_bytes(2, 3)).unwrap();

        assert_eq!(image.size(0), size(DevicePixels(2), DevicePixels(3)));
    }

    #[test]
    fn decoded_image_rejects_pathological_dimensions_before_rgba_conversion() {
        let bytes = png_bytes(MAX_DECODED_IMAGE_WIDTH + 1, 1);

        assert!(decode_display_render_image(&bytes).is_none());
    }

    #[test]
    fn display_raster_is_bounded_and_preserves_aspect_ratio() {
        assert_eq!(display_raster_dimensions(4000, 2000, 2048), (2048, 1024));
        assert_eq!(display_raster_dimensions(2000, 4000, 2048), (1024, 2048));
        assert_eq!(display_raster_dimensions(100, 50, 2048), (100, 50));

        let image = decode_render_image(&png_bytes(128, 64), Some(32)).unwrap();
        assert_eq!(image.size(0), size(DevicePixels(32), DevicePixels(16)));
    }

    #[test]
    fn decoded_dimensions_reject_over_budget_rgba_buffer() {
        assert!(!decoded_dimensions_within_limits(5000, 5000));
        assert!(decoded_dimensions_within_limits(4096, 4096));
        assert!(!decoded_dimensions_within_limits(0, 1));
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

    fn start_load(cache: &mut ImageCache, src: &str) -> u64 {
        match cache.lookup_or_start(src) {
            ImageCacheLookup::StartLoad { generation } => generation,
            ImageCacheLookup::Existing(_) | ImageCacheLookup::AtCapacity => {
                panic!("expected a fresh image load for {src}")
            }
        }
    }

    #[test]
    fn decoded_image_cache_evicts_lru_by_bytes_and_entries() {
        let mut cache = ImageCache::new(2, 64);
        let a_generation = start_load(&mut cache, "a");
        assert!(
            cache
                .finish(
                    "a".to_owned(),
                    a_generation,
                    ImageState::Ready(test_image(2, 2)),
                )
                .is_empty()
        );
        let b_generation = start_load(&mut cache, "b");
        let image_b = test_image(2, 2);
        assert!(
            cache
                .finish(
                    "b".to_owned(),
                    b_generation,
                    ImageState::Ready(image_b.clone()),
                )
                .is_empty()
        );
        let _ = cache.lookup_or_start("a");
        let c_generation = start_load(&mut cache, "c");
        let retired = cache.finish(
            "c".to_owned(),
            c_generation,
            ImageState::Ready(test_image(3, 3)),
        );

        assert!(cache.entries.contains_key("a"));
        assert!(cache.entries.contains_key("c"));
        assert!(!cache.entries.contains_key("b"));
        assert!(cache.entries.len() <= 2);
        assert!(cache.decoded_bytes <= 64);
        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &image_b));
    }

    #[test]
    fn maximum_display_raster_remains_cache_resident_after_decode() {
        let mut cache = ImageCache::new(IMAGE_CACHE_MAX_ENTRIES, IMAGE_CACHE_MAX_DECODED_BYTES);
        let generation = start_load(&mut cache, "large");
        let display = test_image(DISPLAY_IMAGE_MAX_EDGE_PX, DISPLAY_IMAGE_MAX_EDGE_PX);
        assert!(
            cache
                .finish(
                    "large".to_owned(),
                    generation,
                    ImageState::Ready(display.clone()),
                )
                .is_empty()
        );
        assert!(matches!(
            cache.lookup_or_start("large"),
            ImageCacheLookup::Existing(ImageState::Ready(image)) if Arc::ptr_eq(&image, &display)
        ));
    }

    #[test]
    fn in_flight_decode_is_never_evicted_or_dispatched_twice() {
        let mut cache = ImageCache::new(1, 1);
        let _generation = start_load(&mut cache, "loading");
        assert!(matches!(
            cache.lookup_or_start("loading"),
            ImageCacheLookup::Existing(ImageState::Loading { .. })
        ));
        assert!(matches!(
            cache.lookup_or_start("second"),
            ImageCacheLookup::AtCapacity
        ));
        assert!(cache.trim().is_empty());

        assert!(cache.entries.contains_key("loading"));
        assert!(!cache.entries.contains_key("second"));
    }

    #[test]
    fn loading_capacity_does_not_grow_for_unique_sources() {
        let mut cache = ImageCache::new(2, 64);
        let _first = start_load(&mut cache, "first");
        let _second = start_load(&mut cache, "second");
        for index in 0..1024 {
            assert!(matches!(
                cache.lookup_or_start(&format!("pending-{index}")),
                ImageCacheLookup::AtCapacity
            ));
        }
        assert_eq!(cache.entries.len(), 2);
    }

    #[test]
    fn oversized_source_is_rejected_before_cache_admission() {
        let source = "x".repeat(MAX_IMAGE_SOURCE_BYTES + 1);
        assert!(!image_source_allowed(&source));
        assert!(!image_source_allowed("  \n"));
        assert!(image_source_allowed("assets/image.png"));
    }

    #[test]
    fn denied_worker_admission_returns_loading_slot_to_retryable_state() {
        let mut cache = ImageCache::new(2, 64);
        let generation = start_load(&mut cache, "deferred");

        cache.abort_start("deferred", generation);

        assert!(matches!(
            cache.lookup_or_start("deferred"),
            ImageCacheLookup::StartLoad { .. }
        ));
    }

    #[test]
    fn failed_image_cache_entry_can_be_retried_without_touching_ready_entries() {
        let mut cache = ImageCache::new(2, 64);
        let failed_generation = start_load(&mut cache, "failed");
        assert!(
            cache
                .finish("failed".to_owned(), failed_generation, ImageState::Failed)
                .is_empty()
        );
        let ready_generation = start_load(&mut cache, "ready");
        assert!(
            cache
                .finish(
                    "ready".to_owned(),
                    ready_generation,
                    ImageState::Ready(test_image(2, 2)),
                )
                .is_empty()
        );

        cache.retry_failed("failed");
        cache.retry_failed("ready");

        assert!(matches!(
            cache.lookup_or_start("failed"),
            ImageCacheLookup::StartLoad { .. }
        ));
        assert!(matches!(
            cache.lookup_or_start("ready"),
            ImageCacheLookup::Existing(ImageState::Ready(_))
        ));
    }

    #[test]
    fn image_cache_diagnostics_separate_resident_decodes_from_non_ready_keys() {
        let mut cache = ImageCache::new(8, 1024);
        let _loading_generation = start_load(&mut cache, "loading");
        let failed_generation = start_load(&mut cache, "failed");
        let _ = cache.finish("failed".to_owned(), failed_generation, ImageState::Failed);
        let ready_generation = start_load(&mut cache, "ready");
        let _ = cache.finish(
            "ready".to_owned(),
            ready_generation,
            ImageState::Ready(test_image(3, 2)),
        );

        let diagnostics = cache.diagnostics();
        assert_eq!(diagnostics.tracked_entries, 3);
        assert_eq!(diagnostics.loading_entries, 1);
        assert_eq!(diagnostics.failed_entries, 1);
        assert_eq!(diagnostics.decoded_entries, 1);
        assert_eq!(diagnostics.resident_decoded_bytes, 3 * 2 * 4);
        assert_eq!(diagnostics.max_entries, 8);
        assert_eq!(diagnostics.decoded_byte_budget, 1024);
    }
}
