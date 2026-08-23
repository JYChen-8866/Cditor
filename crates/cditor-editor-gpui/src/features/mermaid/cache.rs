use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};

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

struct MermaidRenderEntry {
    content_version: u64,
    source_hash: u64,
    theme: GuiTheme,
    result: Arc<OnceLock<RenderResult>>,
    fallback: Option<Arc<RenderImage>>,
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
            let _ = result.set(Err(message.into()));
            return Self {
                content_version,
                source_hash,
                theme,
                result,
                fallback,
                _task: None,
            };
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
                    renderer
                        .render_single_frame(svg.as_bytes(), 1.0)
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
            _task: Some(task),
        }
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
}

impl MermaidRenderCache {
    pub(crate) fn sync_visible_window(
        &mut self,
        projection: &EditorViewProjection,
        source_blocks: &HashSet<BlockId>,
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
            .filter(|block| matches!(block.kind, RichBlockKind::Mermaid))
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
        let invisible = self
            .entries
            .keys()
            .filter(|block_id| !visible_ids.contains(block_id))
            .copied()
            .collect::<Vec<_>>();
        for block_id in invisible {
            if let Some(entry) = self.entries.remove(&block_id) {
                retired.extend(entry.into_retired_images());
            }
        }

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
            let Some(permit) = worker_admission.try_acquire(WorkerTaskKind::MermaidRender) else {
                continue;
            };
            let fallback = self.entries.remove(&block_id).and_then(|entry| {
                let (fallback, superseded) = entry.into_fallback_and_retired();
                retired.extend(superseded);
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
        }
        retire_images_after_effect(retired, cx);
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
}
