use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};

use crate::features::code::language_is_mermaid;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayloadView, RichBlockKind};
use cditor_runtime::{EditorViewProjection, MainThreadWorkKind, WorkCost, WorkerTaskKind};
use gpui::{App, AppContext, Context, RenderImage, Task};

use crate::app::worker_admission::{EditorWorkerAdmission, WorkerPermit};
use crate::editor_view::CditorV2View;
use crate::theme::GuiTheme;

use super::theme::build_mermaid_theme;

const MAX_MERMAID_SOURCE_BYTES: usize = 256 * 1024;
const MAX_MERMAID_LAYOUT_ENTRIES: usize = 4096;
// Mermaid images retain both decoded CPU pixels and a backend texture. Keep
// their residency budget separate from the layout metadata budget: a large
// document can have many cheap dimensions but only a small number of heavy
// raster images should remain resident at once.
const MAX_MERMAID_RENDER_ENTRIES: usize = 96;
const MAX_MERMAID_RENDER_BYTES: usize = 64 * 1024 * 1024;
const MAX_MERMAID_RENDER_IMAGE_BYTES: usize = 16 * 1024 * 1024;
// `SvgRenderer::render_single_frame` applies a 2x smoothness factor. These
// limits therefore describe the physical pixmap, not the logical size shown
// by the editor. Keeping the cap here prevents a pathological SVG from
// allocating a huge pixmap before the layout clamps its display bounds.
const MAX_MERMAID_RASTER_EDGE: f64 = 2048.0;
const MAX_MERMAID_RASTER_PIXELS: f64 = (MAX_MERMAID_RENDER_IMAGE_BYTES / 4) as f64;
const SVG_SMOOTH_SCALE: f64 = 2.0;

type RenderResult = Result<Arc<RenderImage>, Arc<str>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MermaidRenderDimensions {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Copy)]
struct MermaidLayoutEntry {
    content_version: u64,
    theme: GuiTheme,
    dimensions: MermaidRenderDimensions,
}

#[derive(Default)]
struct MermaidLayoutCache {
    entries: HashMap<BlockId, MermaidLayoutEntry>,
    insertion_order: VecDeque<BlockId>,
}

impl MermaidLayoutCache {
    fn insert(&mut self, block_id: BlockId, entry: MermaidLayoutEntry) {
        if self.entries.contains_key(&block_id) {
            self.entries.insert(block_id, entry);
            return;
        }
        if self.entries.len() >= MAX_MERMAID_LAYOUT_ENTRIES
            && let Some(oldest) = self.insertion_order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(block_id, entry);
        self.insertion_order.push_back(block_id);
    }

    fn get(
        &self,
        block_id: BlockId,
        content_version: u64,
        theme: GuiTheme,
    ) -> Option<MermaidRenderDimensions> {
        self.entries.get(&block_id).and_then(|entry| {
            (entry.content_version == content_version && entry.theme == theme)
                .then_some(entry.dimensions)
        })
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.insertion_order.clear();
    }
}

#[derive(Clone)]
pub(crate) enum MermaidRenderStatus {
    Ready(Arc<RenderImage>),
    Rendering { fallback: Option<Arc<RenderImage>> },
    Failed { message: Arc<str> },
}

/// Result of trimming Mermaid's rebuildable raster state. SVG source and
/// intrinsic dimensions are deliberately excluded: they are cheap metadata
/// used to keep the block's outer geometry stable after a raster is evicted.
#[derive(Default)]
pub(crate) struct MermaidRenderCacheTrimResult {
    pub(crate) evicted_entries: usize,
    pub(crate) evicted_budgeted_bytes: usize,
    pub(crate) evicted_resident_bytes: usize,
    pub(crate) invalidated_renderings: usize,
    pub(crate) remaining_entries: usize,
    pub(crate) remaining_budgeted_bytes: usize,
    pub(crate) remaining_resident_bytes: usize,
    pub(crate) retired_images: Vec<Arc<RenderImage>>,
}

struct MermaidRenderEntry {
    content_version: u64,
    source_hash: u64,
    theme: GuiTheme,
    result: Arc<OnceLock<RenderResult>>,
    fallback: Option<Arc<RenderImage>>,
    last_access: u64,
    _task: Option<Task<()>>,
}

struct MermaidRenderRequest {
    block_id: BlockId,
    content_version: u64,
    source_hash: u64,
    source: String,
    theme: GuiTheme,
    fallback: Option<Arc<RenderImage>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenderSyncAction {
    Keep,
    Refresh,
}

fn render_sync_action(source_visible: bool, current_entry_matches: bool) -> RenderSyncAction {
    if source_visible || current_entry_matches {
        RenderSyncAction::Keep
    } else {
        RenderSyncAction::Refresh
    }
}

impl MermaidRenderEntry {
    fn failed(content_version: u64, source_hash: u64, theme: GuiTheme, message: String) -> Self {
        let result = Arc::new(OnceLock::new());
        let _ = result.set(Err(Arc::<str>::from(message)));
        Self {
            content_version,
            source_hash,
            theme,
            result,
            fallback: None,
            last_access: 0,
            _task: None,
        }
    }

    fn new(
        request: MermaidRenderRequest,
        permit: WorkerPermit,
        cx: &mut Context<CditorV2View>,
    ) -> Self {
        let MermaidRenderRequest {
            block_id,
            content_version,
            source_hash,
            source,
            theme,
            fallback,
        } = request;
        let result = Arc::new(OnceLock::new());
        if let Err(message) = validate_source(&source) {
            drop(fallback);
            drop(permit);
            return Self::failed(content_version, source_hash, theme, message);
        }

        let result_for_task = result.clone();
        let renderer = cx.svg_renderer();
        let render_theme = build_mermaid_theme(theme);
        let task = cx.spawn(async move |view, cx| {
            let rendered = cx
                .background_spawn(async move {
                    let _permit = permit;
                    let svg = mermaid_render::render_to_svg(&source, &render_theme)
                        .map_err(|error| Arc::<str>::from(format!("{error:#}")))?;
                    let scale = bounded_svg_scale(&svg)?;
                    renderer
                        .render_single_frame(svg.as_bytes(), scale)
                        .map_err(|error| Arc::<str>::from(error.to_string()))
                })
                .await;
            let _ = view.update(cx, |view, cx| {
                view.enqueue_main_thread_apply(
                    MainThreadWorkKind::ImageDecodeApply,
                    content_version,
                    Some(block_id),
                    WorkCost::image_decode_apply(),
                    move |_view, cx| {
                        let _ = result_for_task.set(rendered);
                        cx.notify();
                    },
                    cx,
                );
            });
        });

        Self {
            content_version,
            source_hash,
            theme,
            result,
            fallback,
            last_access: 0,
            _task: Some(task),
        }
    }

    fn resident_bytes(&self) -> usize {
        let mut bytes = 0usize;
        let mut seen = None;
        if let Some(Ok(image)) = self.result.get() {
            bytes = bytes.saturating_add(render_image_bytes(image));
            seen = Some(image);
        }
        if let Some(fallback) = self.fallback.as_ref()
            && seen.is_none_or(|image| !Arc::ptr_eq(image, fallback))
        {
            bytes = bytes.saturating_add(render_image_bytes(fallback));
        }
        bytes
    }

    fn reserved_bytes(&self) -> usize {
        usize::from(self.result.get().is_none()).saturating_mul(MAX_MERMAID_RENDER_IMAGE_BYTES)
    }

    /// Bytes charged against the cache before a render starts. An unfinished
    /// render is charged at the maximum raster size so several worker tasks
    /// cannot all pass a zero-byte admission check and complete above budget.
    fn budgeted_bytes(&self) -> usize {
        let fallback_bytes = self
            .fallback
            .as_ref()
            .map_or(0, |fallback| render_image_bytes(fallback));
        match self.result.get() {
            Some(Ok(image)) => {
                let rendered = render_image_bytes(image);
                if self
                    .fallback
                    .as_ref()
                    .is_some_and(|fallback| Arc::ptr_eq(fallback, image))
                {
                    rendered
                } else {
                    rendered.saturating_add(fallback_bytes)
                }
            }
            Some(Err(_)) => fallback_bytes,
            None => fallback_bytes.saturating_add(MAX_MERMAID_RENDER_IMAGE_BYTES),
        }
    }

    fn replacement_budgeted_bytes(&self) -> usize {
        let fallback = match self.result.get() {
            Some(Ok(image)) => Some(image),
            Some(Err(_)) | None => self.fallback.as_ref(),
        };
        fallback
            .map_or(0, |image| render_image_bytes(image))
            .saturating_add(MAX_MERMAID_RENDER_IMAGE_BYTES)
    }

    fn retains_image(&self, image: &Arc<RenderImage>) -> bool {
        self.result
            .get()
            .and_then(|result| result.as_ref().ok())
            .is_some_and(|candidate| Arc::ptr_eq(candidate, image))
            || self
                .fallback
                .as_ref()
                .is_some_and(|candidate| Arc::ptr_eq(candidate, image))
    }

    fn status(&self) -> MermaidRenderStatus {
        match self.result.get() {
            Some(Ok(image)) => MermaidRenderStatus::Ready(image.clone()),
            Some(Err(message)) => MermaidRenderStatus::Failed {
                message: message.clone(),
            },
            None => MermaidRenderStatus::Rendering {
                fallback: self.fallback.clone(),
            },
        }
    }

    fn render_dimensions(&self) -> Option<MermaidRenderDimensions> {
        let image = match self.result.get() {
            Some(Ok(image)) => Some(image),
            Some(Err(_)) | None => self.fallback.as_ref(),
        }?;
        let size = image.size(0);
        Some(MermaidRenderDimensions {
            width: i32::from(size.width).max(1) as u32,
            height: i32::from(size.height).max(1) as u32,
        })
    }

    fn take_completed_fallback(&mut self) -> Option<Arc<RenderImage>> {
        let result = self.result.get()?;
        let fallback = self.fallback.take()?;
        if matches!(result, Ok(image) if Arc::ptr_eq(image, &fallback)) {
            return None;
        }
        Some(fallback)
    }

    fn into_fallback_and_retired(mut self) -> (Option<Arc<RenderImage>>, Vec<Arc<RenderImage>>) {
        let preferred = match self.result.get() {
            Some(Ok(image)) => Some(image.clone()),
            Some(Err(_)) | None => self.fallback.take(),
        };
        let mut retired = Vec::new();
        if let Some(fallback) = self.fallback.take() {
            push_unique_image(&mut retired, fallback, preferred.as_ref());
        }
        (preferred, retired)
    }

    fn into_retired_images(mut self) -> Vec<Arc<RenderImage>> {
        let mut retired = Vec::new();
        if let Some(Ok(image)) = self.result.get() {
            push_unique_image(&mut retired, image.clone(), None);
        }
        if let Some(fallback) = self.fallback.take() {
            push_unique_image(&mut retired, fallback, None);
        }
        retired
    }

    fn matches(&self, content_version: u64, source_hash: u64, theme: GuiTheme) -> bool {
        self.content_version == content_version
            && self.source_hash == source_hash
            && self.theme == theme
    }
}

#[derive(Default)]
pub(crate) struct MermaidRenderCache {
    entries: HashMap<BlockId, MermaidRenderEntry>,
    layouts: MermaidLayoutCache,
    access_clock: u64,
    budgeted_bytes: usize,
}

impl MermaidRenderCache {
    pub(crate) fn diagnostics(&self) -> cditor_sdk::diagnostics::MermaidDiagnostics {
        let mut diagnostics = cditor_sdk::diagnostics::MermaidDiagnostics {
            tracked_entries: self.entries.len(),
            max_entries: MAX_MERMAID_RENDER_ENTRIES,
            render_byte_budget: MAX_MERMAID_RENDER_BYTES,
            ..Default::default()
        };
        for entry in self.entries.values() {
            match entry.result.get() {
                Some(Ok(_)) => diagnostics.ready_entries += 1,
                Some(Err(_)) => diagnostics.failed_entries += 1,
                None => diagnostics.rendering_entries += 1,
            }
            diagnostics.reserved_render_bytes = diagnostics
                .reserved_render_bytes
                .saturating_add(entry.reserved_bytes());
            // Render images never move between block entries. The entry-level
            // accounting de-duplicates the only valid alias: a result and its
            // fallback temporarily pointing at the same image.
            diagnostics.resident_image_bytes = diagnostics
                .resident_image_bytes
                .saturating_add(entry.resident_bytes());
        }
        diagnostics
    }

    pub(crate) fn sync_visible_window(
        &mut self,
        projection: &EditorViewProjection,
        source_blocks: &HashSet<BlockId>,
        preview_code_blocks: &HashSet<BlockId>,
        theme: GuiTheme,
        worker_admission: &EditorWorkerAdmission,
        cx: &mut Context<CditorV2View>,
    ) {
        self.remember_rendered_layouts();
        let mut retired = self
            .entries
            .values_mut()
            .filter_map(MermaidRenderEntry::take_completed_fallback)
            .collect::<Vec<_>>();
        let visible = projection
            .blocks
            .iter()
            .filter(|block| match &block.kind {
                RichBlockKind::Mermaid => true,
                RichBlockKind::Code { language } => {
                    language_is_mermaid(language.as_deref())
                        && preview_code_blocks.contains(&block.block_id)
                }
                _ => false,
            })
            .filter_map(|block| {
                let BlockPayloadView::Loaded(payload) = &block.payload else {
                    return None;
                };
                let source = payload.plain_text();
                Some((
                    block.block_id,
                    payload.content_version,
                    source_hash(&source),
                    source,
                ))
            })
            .collect::<Vec<_>>();
        let visible_ids = visible
            .iter()
            .map(|(block_id, _, _, _)| *block_id)
            .collect::<HashSet<_>>();
        // The projection is the active window. Touch its entries before
        // scheduling work so entries outside the window become LRU victims,
        // while a currently displayed image is never reclaimed in this pass.
        for (block_id, _, _, _) in &visible {
            self.touch(*block_id);
        }

        self.recompute_budgeted_bytes();
        self.trim_render_budget(&visible_ids, &mut retired);

        // `drop_image` must run after the current GPUI effect. Do not start a
        // replacement raster in the same pass: otherwise the evicted pixels,
        // their atlas allocation and the new worker pixmap overlap and can
        // transiently double the declared cache budget.
        let mut defer_new_renders = !retired.is_empty();

        for (block_id, content_version, hash, source) in visible {
            let current_entry_matches = self
                .entries
                .get(&block_id)
                .is_some_and(|entry| entry.matches(content_version, hash, theme));
            if render_sync_action(source_blocks.contains(&block_id), current_entry_matches)
                == RenderSyncAction::Keep
            {
                continue;
            }

            if let Err(message) = validate_source(&source) {
                if let Some(entry) = self.entries.remove(&block_id) {
                    for image in entry.into_retired_images() {
                        self.push_retired_if_unreferenced(&mut retired, image);
                    }
                }
                self.entries.insert(
                    block_id,
                    MermaidRenderEntry::failed(content_version, hash, theme, message),
                );
                self.touch(block_id);
                self.recompute_budgeted_bytes();
                defer_new_renders = true;
                continue;
            }
            if defer_new_renders {
                continue;
            }

            let replacement_bytes = self
                .entries
                .get(&block_id)
                .map_or(MAX_MERMAID_RENDER_IMAGE_BYTES, |entry| {
                    entry.replacement_budgeted_bytes()
                });
            let current_bytes = self
                .entries
                .get(&block_id)
                .map_or(0, MermaidRenderEntry::budgeted_bytes);
            let projected_bytes = self
                .budgeted_bytes
                .saturating_sub(current_bytes)
                .saturating_add(replacement_bytes);
            let projected_entries =
                self.entries.len() + usize::from(!self.entries.contains_key(&block_id));
            if projected_bytes > MAX_MERMAID_RENDER_BYTES
                || projected_entries > MAX_MERMAID_RENDER_ENTRIES
            {
                let evicted = self.evict_unprotected_until_fits(
                    block_id,
                    replacement_bytes,
                    &visible_ids,
                    &mut retired,
                );
                if evicted {
                    // Retired images remain live until the deferred GPUI
                    // effect. Re-evaluate admission on the next frame.
                    defer_new_renders = true;
                }
                if defer_new_renders || !self.replacement_fits(block_id, replacement_bytes) {
                    continue;
                }
            }
            let Some(permit) = worker_admission.try_acquire(WorkerTaskKind::MermaidRender) else {
                continue;
            };
            let fallback = self.entries.remove(&block_id).and_then(|entry| {
                let (fallback, superseded) = entry.into_fallback_and_retired();
                for image in superseded {
                    self.push_retired_if_unreferenced(&mut retired, image);
                }
                fallback
            });
            self.entries.insert(
                block_id,
                MermaidRenderEntry::new(
                    MermaidRenderRequest {
                        block_id,
                        content_version,
                        source_hash: hash,
                        source,
                        theme,
                        fallback,
                    },
                    permit,
                    cx,
                ),
            );
            self.touch(block_id);
            self.recompute_budgeted_bytes();
        }
        self.recompute_budgeted_bytes();
        self.trim_render_budget(&visible_ids, &mut retired);
        retire_images_after_effect(retired, cx);
    }

    fn touch(&mut self, block_id: BlockId) {
        self.access_clock = self.access_clock.wrapping_add(1).max(1);
        if let Some(entry) = self.entries.get_mut(&block_id) {
            entry.last_access = self.access_clock;
        }
    }

    fn recompute_budgeted_bytes(&mut self) {
        self.budgeted_bytes = self
            .entries
            .values()
            .map(MermaidRenderEntry::budgeted_bytes)
            .fold(0usize, usize::saturating_add);
    }

    fn replacement_fits(&self, block_id: BlockId, replacement_bytes: usize) -> bool {
        let current_bytes = self
            .entries
            .get(&block_id)
            .map_or(0, MermaidRenderEntry::budgeted_bytes);
        let projected_bytes = self
            .budgeted_bytes
            .saturating_sub(current_bytes)
            .saturating_add(replacement_bytes);
        let projected_entries =
            self.entries.len() + usize::from(!self.entries.contains_key(&block_id));
        projected_bytes <= MAX_MERMAID_RENDER_BYTES
            && projected_entries <= MAX_MERMAID_RENDER_ENTRIES
    }

    fn evict_unprotected_until_fits(
        &mut self,
        replacing: BlockId,
        replacement_bytes: usize,
        protected: &HashSet<BlockId>,
        retired: &mut Vec<Arc<RenderImage>>,
    ) -> bool {
        let mut evicted = false;
        while !self.replacement_fits(replacing, replacement_bytes) {
            let candidate = self
                .entries
                .iter()
                .filter(|(block_id, _)| **block_id != replacing && !protected.contains(block_id))
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|(block_id, _)| *block_id);
            let Some(candidate) = candidate else {
                break;
            };
            if let Some(entry) = self.entries.remove(&candidate) {
                for image in entry.into_retired_images() {
                    self.push_retired_if_unreferenced(retired, image);
                }
                evicted = true;
                self.recompute_budgeted_bytes();
            }
        }
        evicted
    }

    fn trim_render_budget(
        &mut self,
        protected: &HashSet<BlockId>,
        retired: &mut Vec<Arc<RenderImage>>,
    ) {
        self.recompute_budgeted_bytes();
        while self.entries.len() > MAX_MERMAID_RENDER_ENTRIES
            || self.budgeted_bytes > MAX_MERMAID_RENDER_BYTES
        {
            let candidate = self
                .entries
                .iter()
                .filter(|(block_id, _)| !protected.contains(block_id))
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|(block_id, _)| *block_id);
            let Some(candidate) = candidate else {
                break;
            };
            if let Some(entry) = self.entries.remove(&candidate) {
                for image in entry.into_retired_images() {
                    self.push_retired_if_unreferenced(retired, image);
                }
            }
            self.recompute_budgeted_bytes();
        }
    }

    /// Applies process-level memory pressure without touching the source or
    /// layout caches. `protected` normally contains the current projection's
    /// Mermaid blocks; their ready image/fallback is retained so a pressure
    /// event cannot blank the visible frame or change its dimensions.
    pub(crate) fn apply_memory_pressure(
        &mut self,
        pressure: crate::memory_pressure::CditorMemoryPressure,
        protected: &HashSet<BlockId>,
    ) -> MermaidRenderCacheTrimResult {
        self.remember_rendered_layouts();
        self.recompute_budgeted_bytes();
        let before_entries = self.entries.len();
        let before_budgeted = self.budgeted_bytes;
        let before_resident = self
            .entries
            .values()
            .map(MermaidRenderEntry::resident_bytes)
            .fold(0usize, usize::saturating_add);
        let mut retired_images = Vec::new();
        let mut invalidated_renderings = 0usize;
        let (target_entries, target_budgeted_bytes) = match pressure {
            crate::memory_pressure::CditorMemoryPressure::Normal => {
                (MAX_MERMAID_RENDER_ENTRIES, MAX_MERMAID_RENDER_BYTES)
            }
            crate::memory_pressure::CditorMemoryPressure::Warning => {
                (MAX_MERMAID_RENDER_ENTRIES / 2, MAX_MERMAID_RENDER_BYTES / 2)
            }
            crate::memory_pressure::CditorMemoryPressure::Critical => (0, 0),
        };
        while self.entries.len() > target_entries || self.budgeted_bytes > target_budgeted_bytes {
            let candidate = self
                .entries
                .iter()
                .filter(|(block_id, _)| !protected.contains(block_id))
                // Failed entries have no raster and pending entries own a
                // reservation. Prefer dropping the former, then in-flight
                // work, before evicting a completed image.
                .min_by_key(|(_, entry)| {
                    let state_rank = match entry.result.get() {
                        Some(Err(_)) => 0u8,
                        None => 1u8,
                        Some(Ok(_)) => 2u8,
                    };
                    (state_rank, entry.last_access)
                })
                .map(|(block_id, _)| *block_id);
            let Some(candidate) = candidate else {
                // A visible/pinned image is allowed to exceed a pressure
                // target. Reclaiming it would violate stable-frame and
                // selection/scroll invariants; the next non-critical pass can
                // trim it once it leaves the projection.
                break;
            };
            let Some(entry) = self.entries.remove(&candidate) else {
                continue;
            };
            if entry.result.get().is_none() {
                // Dropping the entry drops its GPUI task and makes any late
                // completion unreachable. It must never recreate this cache
                // slot because completion is committed only through its own
                // generation-bound entry cell.
                invalidated_renderings = invalidated_renderings.saturating_add(1);
            }
            for image in entry.into_retired_images() {
                self.push_retired_if_unreferenced(&mut retired_images, image);
            }
            self.recompute_budgeted_bytes();
        }

        let after_resident = self
            .entries
            .values()
            .map(MermaidRenderEntry::resident_bytes)
            .fold(0usize, usize::saturating_add);
        MermaidRenderCacheTrimResult {
            evicted_entries: before_entries.saturating_sub(self.entries.len()),
            evicted_budgeted_bytes: before_budgeted.saturating_sub(self.budgeted_bytes),
            evicted_resident_bytes: before_resident.saturating_sub(after_resident),
            invalidated_renderings,
            remaining_entries: self.entries.len(),
            remaining_budgeted_bytes: self.budgeted_bytes,
            remaining_resident_bytes: after_resident,
            retired_images,
        }
    }

    fn push_retired_if_unreferenced(
        &self,
        retired: &mut Vec<Arc<RenderImage>>,
        image: Arc<RenderImage>,
    ) {
        if self
            .entries
            .values()
            .any(|entry| entry.retains_image(&image))
        {
            return;
        }
        push_unique_image(retired, image, None);
    }

    pub(crate) fn status(&self, block_id: BlockId) -> Option<MermaidRenderStatus> {
        self.entries.get(&block_id).map(MermaidRenderEntry::status)
    }

    pub(crate) fn preview_dimensions(
        &self,
        block_id: BlockId,
        content_version: u64,
        theme: GuiTheme,
    ) -> Option<MermaidRenderDimensions> {
        if let Some(entry) = self.entries.get(&block_id)
            && entry.content_version == content_version
            && entry.theme == theme
            && let Some(dimensions) = entry.render_dimensions()
        {
            return Some(dimensions);
        }
        self.layouts.get(block_id, content_version, theme)
    }

    fn remember_rendered_layouts(&mut self) {
        let layouts = self
            .entries
            .iter()
            .filter_map(|(block_id, entry)| {
                entry.render_dimensions().map(|dimensions| {
                    (
                        *block_id,
                        MermaidLayoutEntry {
                            content_version: entry.content_version,
                            theme: entry.theme,
                            dimensions,
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        for (block_id, layout) in layouts {
            self.layouts.insert(block_id, layout);
        }
    }

    pub(crate) fn clear(&mut self) -> Vec<Arc<RenderImage>> {
        self.layouts.clear();
        self.access_clock = 0;
        self.budgeted_bytes = 0;
        self.entries
            .drain()
            .flat_map(|(_, entry)| entry.into_retired_images())
            .fold(Vec::new(), |mut retired, image| {
                push_unique_image(&mut retired, image, None);
                retired
            })
    }
}

fn push_unique_image(
    images: &mut Vec<Arc<RenderImage>>,
    candidate: Arc<RenderImage>,
    retained: Option<&Arc<RenderImage>>,
) {
    if retained.is_some_and(|retained| Arc::ptr_eq(&candidate, retained))
        || images.iter().any(|image| Arc::ptr_eq(image, &candidate))
    {
        return;
    }
    images.push(candidate);
}

fn render_image_bytes(image: &RenderImage) -> usize {
    (0..image.frame_count()).fold(0usize, |bytes, frame_index| {
        let frame = image.size(frame_index);
        let width = usize::try_from(frame.width.0.max(0)).unwrap_or(usize::MAX);
        let height = usize::try_from(frame.height.0.max(0)).unwrap_or(usize::MAX);
        bytes.saturating_add(width.saturating_mul(height).saturating_mul(4))
    })
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

fn source_hash(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn validate_source(source: &str) -> Result<(), String> {
    if source.trim().is_empty() {
        return Err("Mermaid 源码为空".to_owned());
    }
    if source.len() > MAX_MERMAID_SOURCE_BYTES {
        return Err(format!(
            "Mermaid 源码超过 {} KiB 安全上限",
            MAX_MERMAID_SOURCE_BYTES / 1024
        ));
    }
    Ok(())
}

/// Computes a safe logical scale for the GPUI SVG rasterizer. `usvg` parses
/// the same root dimensions that `SvgRenderer` uses, but we drop the tree
/// before invoking GPUI so only one parsed tree and one pixmap are live at a
/// time. A malformed SVG is rejected here with the same class of error that
/// the renderer would return, rather than falling back to an unbounded render.
fn bounded_svg_scale(svg: &str) -> Result<f32, Arc<str>> {
    let tree = usvg::Tree::from_data(svg.as_bytes(), &usvg::Options::default())
        .map_err(|error| Arc::<str>::from(format!("invalid Mermaid SVG: {error}")))?;
    let size = tree.size();
    let width = f64::from(size.width());
    let height = f64::from(size.height());
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("Mermaid SVG has invalid dimensions".into());
    }

    // GPUI multiplies the requested scale by SVG_SMOOTH_SCALE internally.
    let requested = 1.0_f64;
    let edge_scale = MAX_MERMAID_RASTER_EDGE / (width.max(height) * SVG_SMOOTH_SCALE);
    let pixel_scale =
        (MAX_MERMAID_RASTER_PIXELS / (width * height * SVG_SMOOTH_SCALE * SVG_SMOOTH_SCALE)).sqrt();
    let scale = requested.min(edge_scale).min(pixel_scale);
    if !scale.is_finite() || scale <= 0.0 {
        return Err("Mermaid SVG exceeds the raster size limit".into());
    }
    Ok(scale as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(width: u32) -> Arc<RenderImage> {
        Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(width, 1),
        )]))
    }

    fn completed_entry(
        result: RenderResult,
        fallback: Option<Arc<RenderImage>>,
    ) -> MermaidRenderEntry {
        let cell = Arc::new(OnceLock::new());
        cell.set(result).ok().expect("fresh result cell");
        MermaidRenderEntry {
            content_version: 1,
            source_hash: 1,
            theme: GuiTheme::light(),
            result: cell,
            fallback,
            last_access: 0,
            _task: None,
        }
    }

    fn pending_entry(fallback: Option<Arc<RenderImage>>) -> MermaidRenderEntry {
        MermaidRenderEntry {
            content_version: 1,
            source_hash: 1,
            theme: GuiTheme::light(),
            result: Arc::new(OnceLock::new()),
            fallback,
            last_access: 0,
            _task: None,
        }
    }

    #[test]
    fn source_validation_rejects_empty_and_oversized_input() {
        assert!(validate_source("  \n").is_err());
        assert!(validate_source("flowchart TD\n A --> B").is_ok());
        assert!(validate_source(&"x".repeat(MAX_MERMAID_SOURCE_BYTES + 1)).is_err());
    }

    #[test]
    fn bounded_svg_scale_keeps_normal_graphs_at_full_quality() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"><rect width="320" height="180"/></svg>"#;
        assert_eq!(bounded_svg_scale(svg).unwrap(), 1.0);
    }

    #[test]
    fn bounded_svg_scale_limits_pathological_dimensions_before_rasterization() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="100000"><rect width="100000" height="100000"/></svg>"#;
        let scale = bounded_svg_scale(svg).unwrap();
        assert!(scale < 0.02);
        let physical_edge = 100_000.0 * f64::from(scale) * SVG_SMOOTH_SCALE;
        let physical_pixels = physical_edge * physical_edge;
        assert!(physical_edge <= MAX_MERMAID_RASTER_EDGE + 1.0);
        assert!(physical_pixels <= MAX_MERMAID_RASTER_PIXELS + 1.0);
    }

    #[test]
    fn bounded_svg_scale_rejects_invalid_dimensions() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"/>"#;
        assert!(bounded_svg_scale(svg).is_err());
    }

    #[test]
    fn source_hash_changes_with_content() {
        assert_eq!(source_hash("A --> B"), source_hash("A --> B"));
        assert_ne!(source_hash("A --> B"), source_hash("A --> C"));
    }

    #[test]
    fn source_editor_retains_cached_render_when_source_changes() {
        assert_eq!(
            render_sync_action(true, false),
            RenderSyncAction::Keep,
            "source editing must not replace the cached render or its fallback"
        );
    }

    #[test]
    fn returning_to_preview_refreshes_only_stale_render() {
        assert_eq!(render_sync_action(false, false), RenderSyncAction::Refresh);
        assert_eq!(render_sync_action(false, true), RenderSyncAction::Keep);
    }

    #[test]
    fn completed_render_retires_its_superseded_fallback() {
        let current = test_image(2);
        let fallback = test_image(1);
        let mut entry = completed_entry(Ok(current), Some(fallback.clone()));

        let retired = entry
            .take_completed_fallback()
            .expect("completed render should release fallback");

        assert!(Arc::ptr_eq(&retired, &fallback));
        assert!(entry.fallback.is_none());
    }

    #[test]
    fn replacing_ready_render_keeps_latest_image_and_retires_only_old_fallback() {
        let current = test_image(2);
        let fallback = test_image(1);
        let entry = completed_entry(Ok(current.clone()), Some(fallback.clone()));

        let (next_fallback, retired) = entry.into_fallback_and_retired();

        assert!(next_fallback.is_some_and(|image| Arc::ptr_eq(&image, &current)));
        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &fallback));
    }

    #[test]
    fn pending_render_reserves_worst_case_raster_bytes() {
        let entry = pending_entry(None);
        assert_eq!(entry.resident_bytes(), 0);
        assert_eq!(entry.budgeted_bytes(), MAX_MERMAID_RENDER_IMAGE_BYTES);

        let fallback = test_image(256);
        let fallback_bytes = render_image_bytes(&fallback);
        let entry = pending_entry(Some(fallback));
        assert_eq!(entry.resident_bytes(), fallback_bytes);
        assert_eq!(
            entry.budgeted_bytes(),
            MAX_MERMAID_RENDER_IMAGE_BYTES + fallback_bytes
        );
    }

    #[test]
    fn cache_trims_pending_reservations_to_hard_byte_budget() {
        let mut cache = MermaidRenderCache::default();
        for block_id in 1..=5 {
            cache.entries.insert(block_id, pending_entry(None));
            cache.touch(block_id);
        }
        cache.recompute_budgeted_bytes();
        assert!(cache.budgeted_bytes > MAX_MERMAID_RENDER_BYTES);

        let mut retired = Vec::new();
        cache.trim_render_budget(&HashSet::new(), &mut retired);

        assert_eq!(cache.entries.len(), 4);
        assert_eq!(cache.budgeted_bytes, MAX_MERMAID_RENDER_BYTES);
        assert!(
            retired.is_empty(),
            "pending entries own no completed images"
        );
    }

    #[test]
    fn replacement_admission_includes_in_flight_reservation() {
        let mut cache = MermaidRenderCache::default();
        for block_id in 1..=4 {
            cache.entries.insert(block_id, pending_entry(None));
        }
        cache.recompute_budgeted_bytes();

        assert!(!cache.replacement_fits(99, MAX_MERMAID_RENDER_IMAGE_BYTES));
        assert!(cache.replacement_fits(1, MAX_MERMAID_RENDER_IMAGE_BYTES));
    }

    #[test]
    fn hard_entry_limit_evicts_the_least_recent_completed_render() {
        let mut cache = MermaidRenderCache::default();
        for block_id in 1..=MAX_MERMAID_RENDER_ENTRIES as u64 + 1 {
            cache
                .entries
                .insert(block_id, completed_entry(Ok(test_image(1)), None));
            cache.touch(block_id);
        }
        let mut retired = Vec::new();
        cache.trim_render_budget(&HashSet::new(), &mut retired);

        assert_eq!(cache.entries.len(), MAX_MERMAID_RENDER_ENTRIES);
        assert!(!cache.entries.contains_key(&1));
        assert_eq!(retired.len(), 1);
    }

    #[test]
    fn failed_render_does_not_retain_a_stale_fallback() {
        let fallback = test_image(64);
        let entry =
            MermaidRenderEntry::failed(2, 3, GuiTheme::light(), "invalid source".to_owned());
        assert_eq!(entry.resident_bytes(), 0);
        assert!(!entry.retains_image(&fallback));
    }

    #[test]
    fn diagnostics_separate_resident_images_from_inflight_reservations() {
        let shared = test_image(10);
        let shared_bytes = render_image_bytes(&shared);
        let pending_fallback = test_image(5);
        let pending_fallback_bytes = render_image_bytes(&pending_fallback);
        let mut cache = MermaidRenderCache::default();
        cache
            .entries
            .insert(1, completed_entry(Ok(shared.clone()), Some(shared.clone())));
        cache
            .entries
            .insert(2, pending_entry(Some(pending_fallback)));
        cache.entries.insert(
            3,
            MermaidRenderEntry::failed(2, 3, GuiTheme::light(), "invalid source".to_owned()),
        );

        let diagnostics = cache.diagnostics();
        assert_eq!(diagnostics.tracked_entries, 3);
        assert_eq!(diagnostics.ready_entries, 1);
        assert_eq!(diagnostics.rendering_entries, 1);
        assert_eq!(diagnostics.failed_entries, 1);
        assert_eq!(
            diagnostics.resident_image_bytes,
            shared_bytes + pending_fallback_bytes
        );
        assert_eq!(
            diagnostics.reserved_render_bytes,
            MAX_MERMAID_RENDER_IMAGE_BYTES
        );
        assert_eq!(diagnostics.max_entries, MAX_MERMAID_RENDER_ENTRIES);
        assert_eq!(diagnostics.render_byte_budget, MAX_MERMAID_RENDER_BYTES);
    }

    #[test]
    fn rendered_dimensions_survive_image_eviction() {
        let block_id = 7;
        let image = Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(640, 360),
        )]));
        let mut cache = MermaidRenderCache::default();
        cache
            .entries
            .insert(block_id, completed_entry(Ok(image), None));

        cache.remember_rendered_layouts();
        cache.entries.remove(&block_id);

        assert_eq!(
            cache.preview_dimensions(block_id, 1, GuiTheme::light()),
            Some(MermaidRenderDimensions {
                width: 640,
                height: 360,
            })
        );
    }

    #[test]
    fn cached_dimensions_are_rejected_after_content_or_theme_changes() {
        let mut cache = MermaidRenderCache::default();
        cache.layouts.insert(
            7,
            MermaidLayoutEntry {
                content_version: 3,
                theme: GuiTheme::light(),
                dimensions: MermaidRenderDimensions {
                    width: 640,
                    height: 360,
                },
            },
        );

        assert!(cache.preview_dimensions(7, 4, GuiTheme::light()).is_none());
        assert!(cache.preview_dimensions(7, 3, GuiTheme::dark()).is_none());
    }

    #[test]
    fn warning_pressure_preserves_protected_render_and_dimensions() {
        let protected_id = 1;
        let evictable_id = 2;
        let protected_image = test_image(640);
        let evictable_image = test_image(320);
        let mut cache = MermaidRenderCache::default();
        cache.entries.insert(
            protected_id,
            completed_entry(Ok(protected_image.clone()), None),
        );
        cache.entries.insert(
            evictable_id,
            completed_entry(Ok(evictable_image.clone()), None),
        );
        cache.touch(protected_id);
        cache.touch(evictable_id);
        cache.remember_rendered_layouts();
        let protected = [protected_id].into_iter().collect::<HashSet<_>>();

        let report = cache.apply_memory_pressure(
            crate::memory_pressure::CditorMemoryPressure::Critical,
            &protected,
        );

        assert_eq!(report.evicted_entries, 1);
        assert_eq!(report.invalidated_renderings, 0);
        assert!(cache.entries.contains_key(&protected_id));
        assert!(!cache.entries.contains_key(&evictable_id));
        assert_eq!(report.retired_images.len(), 1);
        assert!(Arc::ptr_eq(&report.retired_images[0], &evictable_image));
        assert!(
            cache
                .preview_dimensions(protected_id, 1, GuiTheme::light())
                .is_some()
        );
        assert!(
            cache
                .preview_dimensions(evictable_id, 1, GuiTheme::light())
                .is_some()
        );
    }

    #[test]
    fn critical_pressure_invalidates_pending_render_without_touching_layout_metadata() {
        let pending_id = 4;
        let mut cache = MermaidRenderCache::default();
        cache.entries.insert(pending_id, pending_entry(None));
        cache.layouts.insert(
            pending_id,
            MermaidLayoutEntry {
                content_version: 1,
                theme: GuiTheme::light(),
                dimensions: MermaidRenderDimensions {
                    width: 800,
                    height: 450,
                },
            },
        );
        let report = cache.apply_memory_pressure(
            crate::memory_pressure::CditorMemoryPressure::Critical,
            &HashSet::new(),
        );

        assert_eq!(report.evicted_entries, 1);
        assert_eq!(report.invalidated_renderings, 1);
        assert_eq!(report.remaining_entries, 0);
        assert!(report.retired_images.is_empty());
        assert_eq!(
            cache.preview_dimensions(pending_id, 1, GuiTheme::light()),
            Some(MermaidRenderDimensions {
                width: 800,
                height: 450,
            })
        );
    }
}
