use cditor_core::{ids::SurfaceId, rich_text::RichBlockKind};
use cditor_runtime::{EditorViewProjection, MainThreadWorkKind, WorkCost};
use cditor_text::requires_segmentation;
use gpui::{Context, FontStyle, FontWeight, Window};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::{
    block::block_content::parse_block_hex_color,
    document::{DocumentLayoutMetrics, DocumentTextGeometry},
    editor_view::CditorV2View,
    features::code::highlight::code_theme_item,
    text::{
        RichTextLayoutInput, RichTextTypography, TextLayoutCacheRequest, TextLayoutOptions,
        cached_text_layout_with_request, element::metrics::text_layout_options,
        try_cached_text_layout_with_request,
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
            cached_text_layout_with_request(&input, theme, &options, request);
            return;
        }

        let generation = layout_generation(input.content_version, input.layout_version);
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
            return;
        }
        let pending_key = block_id.is_none().then(|| prewarm_key(&input, &options));
        if let Some(key) = pending_key
            && !self.cache.pending_text_layout_prewarms.insert(key)
        {
            return;
        }
        let surface_id = input.surface_id;
        self.enqueue_main_thread_apply(
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
                    cached_text_layout_with_request(&input, theme, &options, request);
                    cx.notify();
                }
            },
            cx,
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
                "system-ui",
                FontWeight::NORMAL,
                FontStyle::Normal,
                scale,
                Some(text_geometry.width_px as f32),
                RichTextTypography::default(),
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
            pending.push(PrimaryTextPrewarm {
                generation: layout_generation(input.content_version, input.layout_version),
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
        for task in pending {
            let visible_inline_slot = task.rank > 1
                || synchronous_visible_layouts < MAX_SYNCHRONOUS_VISIBLE_LAYOUTS_PER_FRAME;
            if visible_inline_slot
                && self
                    .scheduling
                    .main_thread
                    .try_admit_inline(task.kind, task.cost)
            {
                cached_text_layout_with_request(
                    &task.input,
                    task.theme,
                    &task.options,
                    task.request,
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
                continue;
            }
            let input = task.input;
            let options = task.options;
            self.enqueue_main_thread_apply(
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
                        cached_text_layout_with_request(&input, task.theme, &options, task.request);
                        cx.notify();
                    }
                },
                cx,
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

fn layout_generation(content_version: u64, layout_version: u64) -> u64 {
    content_version
        .saturating_mul(1_000_000_000)
        .saturating_add(layout_version.min(999_999_999))
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
    fn layout_generation_orders_content_before_layout_versions() {
        assert!(layout_generation(4, 1) > layout_generation(3, 999_999_999));
        assert!(layout_generation(4, 2) > layout_generation(4, 1));
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
