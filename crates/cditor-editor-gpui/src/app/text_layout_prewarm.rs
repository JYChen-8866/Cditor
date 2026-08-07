use cditor_core::{ids::SurfaceId, rich_text::RichBlockKind};
use cditor_runtime::{EditorViewProjection, MainThreadWorkKind, WorkCost};
use cditor_text::requires_segmentation;
use gpui::{Context, FontStyle, FontWeight, Window};
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
};

use crate::{
    block::block_content::{page_title_typography, parse_block_hex_color},
    diagnostics::text_layout::trace as trace_text_layout,
    document::{DocumentLayoutMetrics, DocumentTextGeometry},
    editor_view::CditorV2View,
    features::code::highlight::code_theme_item,
    platform::body_font_family,
    text::{
        RichTextLayoutInput, TextLayoutCacheRequest, TextLayoutOptions,
        cached_text_layout_with_request, element::metrics::text_layout_options,
        text_layout_cache_stats, try_cached_text_layout_with_request,
    },
    theme::GuiTheme,
};

struct PrimaryTextPrewarm {
    input: RichTextLayoutInput,
    theme: GuiTheme,
    options: TextLayoutOptions,
    request: TextLayoutCacheRequest,
    kind: MainThreadWorkKind,
    generation: u64,
    rank: u8,
    cost: WorkCost,
}

const MAX_SYNCHRONOUS_VISIBLE_LAYOUTS_PER_FRAME: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TextLayoutPrewarmKey {
    surface_id: SurfaceId,
    content_version: u64,
    layout_version: u64,
    theme_version: u64,
    font_version: u64,
    width_bits: Option<u32>,
    text_fingerprint: u64,
}

#[derive(Default)]
pub(crate) struct TextLayoutPrewarmGenerationState {
    latest: HashMap<SurfaceId, (TextLayoutPrewarmKey, u64)>,
    next: u64,
}

impl TextLayoutPrewarmGenerationState {
    fn generation_for(&mut self, key: TextLayoutPrewarmKey) -> u64 {
        if let Some((latest_key, generation)) = self.latest.get(&key.surface_id)
            && latest_key == &key
        {
            return *generation;
        }
        self.next = self.next.saturating_add(1).max(1);
        self.latest.insert(key.surface_id, (key, self.next));
        self.next
    }

    pub(crate) fn clear(&mut self) {
        self.latest.clear();
        self.next = 0;
    }
}

impl CditorV2View {
    pub(crate) fn ensure_text_layout_prewarm(
        &mut self,
        input: RichTextLayoutInput,
        theme: GuiTheme,
        options: TextLayoutOptions,
        request: TextLayoutCacheRequest,
        cx: &mut Context<Self>,
    ) {
        let kind = if request.pin_surface {
            MainThreadWorkKind::EditingTextShape
        } else {
            MainThreadWorkKind::CurrentWindowMeasure
        };
        let bytes = input.text_len();
        let cost = text_shape_cost(bytes);
        if self.scheduling.main_thread.try_admit_inline(kind, cost) {
            let cached = cached_text_layout_with_request(&input, theme, &options, request);
            let stats = text_layout_cache_stats();
            trace_text_layout(
                "prewarm.inline",
                format_args!(
                    "surface={:?} kind={kind:?} width={:?} strategy={:?} entries={} pinned={} misses={} reflows={} evictions={}",
                    input.surface_id,
                    options.width,
                    cached.strategy,
                    stats.entries,
                    stats.pinned_entries,
                    stats.misses,
                    stats.reflows,
                    stats.evictions,
                ),
            );
            return;
        }

        let prewarm_key = prewarm_key(&input, &options);
        let generation = self
            .cache
            .text_layout_prewarm_generations
            .generation_for(prewarm_key);
        let block_id = match input.surface_id {
            SurfaceId::Block(block_id) => Some(block_id),
            _ => None,
        };
        if let Some(block_id) = block_id
            && self
                .scheduling
                .main_thread
                .has_pending(kind, block_id, generation)
        {
            trace_text_layout(
                "prewarm.skip-pending",
                format_args!(
                    "surface={:?} kind={kind:?} generation={generation} width={:?}",
                    input.surface_id, options.width
                ),
            );
            return;
        }
        let pending_key = block_id.is_none().then_some(prewarm_key);
        if let Some(key) = pending_key
            && !self.cache.pending_text_layout_prewarms.insert(key)
        {
            trace_text_layout(
                "prewarm.skip-key",
                format_args!(
                    "surface={:?} kind={kind:?} generation={generation} width={:?}",
                    input.surface_id, options.width
                ),
            );
            return;
        }
        let surface_id = input.surface_id;
        let requested_width = options.width;
        let decision = self.enqueue_main_thread_apply(
            kind,
            generation,
            block_id,
            cost,
            move |view, cx| {
                if let Some(key) = pending_key {
                    view.cache.pending_text_layout_prewarms.remove(&key);
                }
                let current = view.ready_session().is_some_and(|session| {
                    session
                        .surface_version(surface_id)
                        .ok()
                        .flatten()
                        .is_some_and(|version| {
                            version.content_version == input.content_version
                                && version.layout_version == input.layout_version
                        })
                });
                if current {
                    let cached =
                        cached_text_layout_with_request(&input, theme, &options, request);
                    let stats = text_layout_cache_stats();
                    trace_text_layout(
                        "prewarm.apply",
                        format_args!(
                            "surface={surface_id:?} kind={kind:?} generation={generation} width={:?} strategy={:?} entries={} pinned={} misses={} reflows={} evictions={}",
                            options.width,
                            cached.strategy,
                            stats.entries,
                            stats.pinned_entries,
                            stats.misses,
                            stats.reflows,
                            stats.evictions,
                        ),
                    );
                    cx.notify();
                } else {
                    trace_text_layout(
                        "prewarm.drop-version",
                        format_args!(
                            "surface={surface_id:?} kind={kind:?} generation={generation} width={:?}",
                            options.width
                        ),
                    );
                }
            },
            cx,
        );
        trace_text_layout(
            "prewarm.enqueue",
            format_args!(
                "surface={surface_id:?} kind={kind:?} generation={generation} width={requested_width:?} decision={decision:?}"
            ),
        );
    }

    pub(crate) fn prewarm_primary_text_layouts(
        &mut self,
        projection: &EditorViewProjection,
        document_layout: DocumentLayoutMetrics,
        viewport_height_px: f32,
        theme: GuiTheme,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let viewport_top = projection.scroll.global_scroll_top;
        let viewport_bottom = viewport_top + f64::from(viewport_height_px.max(1.0));
        let scale = window.scale_factor();
        let mut block_top = projection.before_window_height;
        let mut pending = Vec::new();

        for block in &projection.blocks {
            let block_bottom = block_top + block.layout.effective_height();
            let intersects_viewport = block_bottom >= viewport_top && block_top <= viewport_bottom;
            block_top = block_bottom;
            let text_geometry = DocumentTextGeometry::for_block(block, theme, document_layout);
            let Some(mut input) =
                RichTextLayoutInput::from_snapshot(block, text_geometry.width_px, 1, 1)
            else {
                continue;
            };
            if matches!(block.kind, RichBlockKind::Code { .. })
                && let Some(spans) = self
                    .cache
                    .code_highlights
                    .spans(block.block_id, input.content_version)
            {
                input.spans = spans;
            }
            let text_len = input.text_len();
            if requires_segmentation(text_len) {
                continue;
            }
            let text_theme = if matches!(block.kind, RichBlockKind::Code { .. }) {
                GuiTheme {
                    code_text: code_theme_item(self.features.code_highlight_theme).foreground,
                    ..theme
                }
            } else {
                theme
            };
            let options = text_layout_options(
                &input,
                text_theme,
                block.attrs.color.as_deref().and_then(parse_block_hex_color),
                &body_font_family(),
                FontWeight::NORMAL,
                FontStyle::Normal,
                scale,
                Some(text_geometry.width_px as f32),
                page_title_typography(block).unwrap_or_default(),
                Vec::new(),
            );
            let request = if block.focused {
                TextLayoutCacheRequest::editing()
            } else {
                TextLayoutCacheRequest::visible()
            };
            if try_cached_text_layout_with_request(&input, &options, request).is_some() {
                continue;
            }
            let (kind, rank) = if block.focused {
                (MainThreadWorkKind::EditingTextShape, 0)
            } else if intersects_viewport {
                (MainThreadWorkKind::CurrentWindowMeasure, 1)
            } else {
                (MainThreadWorkKind::Prefetch, 2)
            };
            let generation = self
                .cache
                .text_layout_prewarm_generations
                .generation_for(prewarm_key(&input, &options));
            pending.push(PrimaryTextPrewarm {
                generation,
                cost: text_shape_cost(text_len),
                input,
                theme: text_theme,
                options,
                request,
                kind,
                rank,
            });
        }

        pending.sort_by_key(|task| task.rank);
        let mut synchronous_visible_layouts = 0;
        let scrollbar_dragging = self.interaction.scrollbar_drag.is_some();
        for task in pending {
            let visible_inline_slot = task.rank > 1
                || synchronous_visible_layouts < MAX_SYNCHRONOUS_VISIBLE_LAYOUTS_PER_FRAME;
            // A resident payload is not visibly real until its text layout has
            // been shaped. The generic drag budget admits only four measurements
            // per frame, which turns the rest of a 15-25 block viewport into
            // deferred placeholder bars even though SQLite already returned.
            // Shape the bounded physical viewport now; render-window overscan and
            // segmented long text continue through the normal budgeted path.
            let force_drag_visible = scrollbar_dragging && task.rank == 1 && visible_inline_slot;
            let admitted = force_drag_visible
                || (visible_inline_slot
                    && self
                        .scheduling
                        .main_thread
                        .try_admit_inline(task.kind, task.cost));
            if admitted {
                let cached = cached_text_layout_with_request(
                    &task.input,
                    task.theme,
                    &task.options,
                    task.request,
                );
                let stats = text_layout_cache_stats();
                trace_text_layout(
                    "primary.inline",
                    format_args!(
                        "surface={:?} kind={:?} rank={} width={:?} strategy={:?} entries={} pinned={} misses={} reflows={} evictions={}",
                        task.input.surface_id,
                        task.kind,
                        task.rank,
                        task.options.width,
                        cached.strategy,
                        stats.entries,
                        stats.pinned_entries,
                        stats.misses,
                        stats.reflows,
                        stats.evictions,
                    ),
                );
                if task.rank <= 1 {
                    synchronous_visible_layouts += 1;
                }
                continue;
            }
            let block_id = task.input.block_id;
            if self
                .scheduling
                .main_thread
                .has_pending(task.kind, block_id, task.generation)
            {
                trace_text_layout(
                    "primary.skip-pending",
                    format_args!(
                        "block={block_id} kind={:?} generation={} width={:?}",
                        task.kind, task.generation, task.options.width
                    ),
                );
                continue;
            }
            let input = task.input;
            let options = task.options;
            let kind = task.kind;
            let generation = task.generation;
            let requested_width = options.width;
            let decision = self.enqueue_main_thread_apply(
                task.kind,
                task.generation,
                Some(block_id),
                task.cost,
                move |view, cx| {
                    let current = view.ready_session().is_some_and(|session| {
                        session
                            .surface_version(SurfaceId::Block(block_id))
                            .ok()
                            .flatten()
                            .is_some_and(|version| {
                                version.content_version == input.content_version
                                    && version.layout_version == input.layout_version
                            })
                    });
                    if current {
                        let cached = cached_text_layout_with_request(
                            &input,
                            task.theme,
                            &options,
                            task.request,
                        );
                        let stats = text_layout_cache_stats();
                        trace_text_layout(
                            "primary.apply",
                            format_args!(
                                "block={block_id} kind={kind:?} generation={generation} width={:?} strategy={:?} entries={} pinned={} misses={} reflows={} evictions={}",
                                options.width,
                                cached.strategy,
                                stats.entries,
                                stats.pinned_entries,
                                stats.misses,
                                stats.reflows,
                                stats.evictions,
                            ),
                        );
                        cx.notify();
                    } else {
                        trace_text_layout(
                            "primary.drop-version",
                            format_args!(
                                "block={block_id} kind={kind:?} generation={generation} width={:?}",
                                options.width
                            ),
                        );
                    }
                },
                cx,
            );
            trace_text_layout(
                "primary.enqueue",
                format_args!(
                    "block={block_id} kind={kind:?} generation={generation} width={requested_width:?} decision={decision:?}"
                ),
            );
        }
    }
}

fn prewarm_key(input: &RichTextLayoutInput, options: &TextLayoutOptions) -> TextLayoutPrewarmKey {
    let mut hasher = DefaultHasher::new();
    for span in &input.spans {
        span.text.hash(&mut hasher);
        span.marks.hash(&mut hasher);
    }
    TextLayoutPrewarmKey {
        surface_id: input.surface_id,
        content_version: input.content_version,
        layout_version: input.layout_version,
        theme_version: input.theme_version,
        font_version: input.font_version,
        width_bits: options.width.map(f32::to_bits),
        text_fingerprint: hasher.finish(),
    }
}

fn text_shape_cost(bytes: usize) -> WorkCost {
    WorkCost {
        sync_ms: (0.12 + bytes as f64 / (32.0 * 1024.0)).clamp(0.12, 2.5),
        measure_applies: 1,
        window_diff_items: 1,
        ..WorkCost::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_cost_is_bounded_and_scales_with_payload_size() {
        assert_eq!(text_shape_cost(0).sync_ms, 0.12);
        assert!(text_shape_cost(64 * 1024).sync_ms > text_shape_cost(1024).sync_ms);
        assert_eq!(text_shape_cost(10 * 1024 * 1024).sync_ms, 2.5);
        assert_eq!(text_shape_cost(1).measure_applies, 1);
    }

    #[test]
    fn synchronous_visible_layout_cap_covers_a_normal_viewport_without_unbounded_work() {
        assert_eq!(MAX_SYNCHRONOUS_VISIBLE_LAYOUTS_PER_FRAME, 64);
        assert!(MAX_SYNCHRONOUS_VISIBLE_LAYOUTS_PER_FRAME >= 32);
    }

    #[test]
    fn prewarm_generation_is_stable_for_one_key_and_advances_for_new_widths() {
        let input = RichTextLayoutInput {
            block_id: 5,
            surface_id: SurfaceId::Block(5),
            content_version: 7,
            layout_version: 11,
            kind: RichBlockKind::Paragraph,
            text_align: cditor_core::rich_text::TextAlign::Start,
            spans: vec![cditor_core::rich_text::InlineSpan::plain("resize")].into(),
            width_px: 200.0,
            theme_version: 13,
            font_version: 17,
        };
        let narrow = TextLayoutOptions {
            width: Some(100.0),
            ..TextLayoutOptions::default()
        };
        let wide = TextLayoutOptions {
            width: Some(200.0),
            ..TextLayoutOptions::default()
        };
        let mut generations = TextLayoutPrewarmGenerationState::default();

        let first = generations.generation_for(prewarm_key(&input, &narrow));
        assert_eq!(
            generations.generation_for(prewarm_key(&input, &narrow)),
            first
        );
        let second = generations.generation_for(prewarm_key(&input, &wide));
        let third = generations.generation_for(prewarm_key(&input, &narrow));

        assert!(second > first);
        assert!(third > second);
    }

    #[test]
    fn auxiliary_pending_key_distinguishes_surface_width_and_styled_text() {
        let mut input = RichTextLayoutInput {
            block_id: 5,
            surface_id: SurfaceId::TableCell {
                block_id: 5,
                row: 2,
                column: 3,
            },
            content_version: 7,
            layout_version: 11,
            kind: RichBlockKind::Paragraph,
            text_align: cditor_core::rich_text::TextAlign::Start,
            spans: vec![cditor_core::rich_text::InlineSpan::plain("cell")].into(),
            width_px: 200.0,
            theme_version: 13,
            font_version: 17,
        };
        let narrow = TextLayoutOptions {
            width: Some(100.0),
            ..TextLayoutOptions::default()
        };
        let wide = TextLayoutOptions {
            width: Some(200.0),
            ..TextLayoutOptions::default()
        };

        assert_ne!(prewarm_key(&input, &narrow), prewarm_key(&input, &wide));
        let plain = prewarm_key(&input, &narrow);
        input.spans = vec![cditor_core::rich_text::InlineSpan {
            text: "cell".to_owned(),
            marks: vec![cditor_core::rich_text::InlineMark::Bold],
        }]
        .into();
        assert_ne!(plain, prewarm_key(&input, &narrow));
    }
}
