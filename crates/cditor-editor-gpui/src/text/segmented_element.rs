mod cache;

use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use cditor_core::{edit::TextAffinity, layout::normalize_text_inner_measured_height};
use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Size, Style, Window, fill, point, px, rgb, rgba,
};

use super::element::RichTextInputHandler;
use super::{
    RichTextLayoutInput, RichTextTypography, SegmentedLayoutFragment, SegmentedPlatformLayout,
    SegmentedTextViewport, TextLayoutCacheRequest, TextLayoutPosition, TextPlatformLayoutIdentity,
    cached_text_layout_with_request,
};
use crate::{
    cache::{publish_text_layout, schedule_layout_correction_frame},
    editor_view::GuiPlatformInputTarget,
    input::platform_adapter::handle_registered_platform_input,
    text::{
        background::text_selection_background,
        element::metrics::{line_height_for, measured_wrap_width, text_layout_options},
        layout_adapter::{paint_text_layout, text_background_quads},
        platform::RichTextPlatformLayout,
    },
    theme::GuiTheme,
};

use cache::{admitted_segments, cached_segmented_surface, segmented_style_fingerprint};
#[cfg(test)]
use cache::{changed_text_range, remove_cached_segmented_surface_for_tests};

const OVERSCAN_VIEWPORTS: f32 = 1.0;

#[derive(Clone)]
pub(crate) struct SegmentedRichTextElement {
    input: RichTextLayoutInput,
    theme: GuiTheme,
    caret_offset: Option<usize>,
    caret_affinity: TextAffinity,
    marked_range: Option<Range<usize>>,
    selection_range: Option<Range<usize>>,
    search_ranges: Vec<super::TextSearchRange>,
    base_text_color: Option<u32>,
    typography: RichTextTypography,
    viewport: SegmentedTextViewport,
    input_handler: RichTextInputHandler,
}

impl SegmentedRichTextElement {
    #[expect(
        clippy::too_many_arguments,
        reason = "text input state is explicit at the element boundary"
    )]
    pub(crate) fn new(
        input: RichTextLayoutInput,
        theme: GuiTheme,
        caret_offset: Option<usize>,
        caret_affinity: TextAffinity,
        marked_range: Option<Range<usize>>,
        selection_range: Option<Range<usize>>,
        search_ranges: Vec<super::TextSearchRange>,
        base_text_color: Option<u32>,
        viewport: SegmentedTextViewport,
        input_handler: RichTextInputHandler,
    ) -> Self {
        Self {
            input,
            theme,
            caret_offset,
            caret_affinity,
            marked_range,
            selection_range,
            search_ranges,
            base_text_color,
            typography: RichTextTypography::default(),
            viewport,
            input_handler,
        }
    }

    pub(crate) fn render(self) -> AnyElement {
        self.into_any_element()
    }
}

pub(crate) struct SegmentedRequestState {
    total_height_px: f32,
    fragments: Vec<SegmentedLayoutFragment>,
    text: Arc<str>,
}

pub(crate) struct SegmentedPrepaintState {
    request: Option<SegmentedRequestState>,
    cursor: Option<gpui::PaintQuad>,
    backgrounds: Vec<gpui::PaintQuad>,
    marked_underlines: Vec<gpui::PaintQuad>,
}

impl IntoElement for SegmentedRichTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SegmentedRichTextElement {
    type RequestLayoutState = Rc<RefCell<Option<SegmentedRequestState>>>;
    type PrepaintState = SegmentedPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let shared = Rc::new(RefCell::new(None));
        let output = shared.clone();
        let input = self.input.clone();
        let theme = self.theme;
        let base_text_color = self.base_text_color;
        let typography = self.typography;
        let caret_offset = self.caret_offset;
        let selection_range = self.selection_range.clone();
        let marked_range = self.marked_range.clone();
        let editor_view = self.input_handler.view.clone();
        let viewport = self.viewport.clone();
        let base_font = window.text_style().font();
        let font_family = crate::platform::BODY_FONT_FAMILY.to_owned();
        let font_weight = base_font.weight;
        let font_style = base_font.style;
        let scale = window.scale_factor();
        let style_fingerprint = segmented_style_fingerprint(
            &input,
            theme,
            base_text_color,
            typography,
            &font_family,
            font_weight,
            font_style,
            scale,
        );
        let (text, segmented) = cached_segmented_surface(&input, style_fingerprint);
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.min_size.width = px(0.0).into();
        style.max_size.width = gpui::relative(1.0).into();
        let layout_id =
            window.request_measured_layout(style, move |known, available, _window, cx| {
                let wrap_width = measured_wrap_width(known.width, available.width, input.width_px);
                let width = Some(f32::from(wrap_width));
                let (viewport_top, viewport_height) = viewport.visible_range();
                let mut layout = segmented.borrow_mut();
                layout.set_width(width);
                let overscan = viewport_height * OVERSCAN_VIEWPORTS;
                let visible = layout.visible_segments(viewport_top, viewport_height);
                let overscanned = layout.visible_segments(
                    (viewport_top - overscan).max(0.0),
                    viewport_height + overscan * 2.0,
                );
                let measure = overscanned.start.saturating_sub(1)
                    ..(overscanned.end + 1).min(layout.segment_count());
                let mut segment_indices = measure.collect::<Vec<_>>();
                let interaction_offsets = [
                    caret_offset,
                    selection_range.as_ref().map(|range| range.start),
                    selection_range.as_ref().map(|range| range.end),
                    marked_range.as_ref().map(|range| range.start),
                    marked_range.as_ref().map(|range| range.end),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
                for offset in interaction_offsets.iter().copied() {
                    if let Some(index) = layout.segment_index_at_byte(offset) {
                        segment_indices.push(index);
                    }
                }
                segment_indices.sort_unstable();
                segment_indices.dedup();
                let scheduled = admitted_segments(
                    &layout,
                    &segment_indices,
                    visible,
                    &interaction_offsets,
                    marked_range.is_some(),
                    |kind, cost| {
                        editor_view.update(cx, |view, cx| {
                            let admitted = view.scheduling.main_thread.try_admit_inline(kind, cost);
                            if !admitted {
                                cx.notify();
                            }
                            admitted
                        })
                    },
                );
                let spans = input.spans.clone();
                let mut build_segment = |segment_text: &str, global_range: Range<usize>| {
                    let effective = global_range.start..global_range.start + segment_text.len();
                    let mut segment_input = input.clone();
                    segment_input.spans = spans.slice(effective).into();
                    let options = text_layout_options(
                        &segment_input,
                        theme,
                        base_text_color,
                        &font_family,
                        font_weight,
                        font_style,
                        scale,
                        width,
                        typography,
                        Vec::new(),
                    );
                    cached_text_layout_with_request(
                        &segment_input,
                        theme,
                        &options,
                        TextLayoutCacheRequest::visible(),
                    )
                    .layout
                };
                for index in scheduled {
                    layout.measure_segments(index..index + 1, &mut build_segment);
                }
                layout.retain_segment_snapshots(&segment_indices);
                let fragments = segment_indices
                    .into_iter()
                    .filter_map(|index| {
                        Some(SegmentedLayoutFragment {
                            byte_range: layout.segment_layout_byte_range(index)?,
                            top_px: layout.segment_top(index),
                            snapshot: layout.segment_snapshot(index)?.clone(),
                        })
                    })
                    .collect();
                let total_height_px = layout.total_height();
                output.replace(Some(SegmentedRequestState {
                    total_height_px,
                    fragments,
                    text: text.clone(),
                }));
                Size {
                    width: wrap_width,
                    height: px(total_height_px).max(line_height_for(&input.kind, typography)),
                }
            });
        (layout_id, shared)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let request = request_layout.borrow_mut().take();
        let platform = request.as_ref().map(|request| {
            SegmentedPlatformLayout::new(
                request.text.clone(),
                request.fragments.clone(),
                request.text.split('\n').count().max(1),
            )
        });
        let caret_bounds = self.caret_offset.and_then(|offset| {
            platform.as_ref().map(|layout| {
                rect_to_bounds(
                    bounds,
                    layout.caret_rect(
                        TextLayoutPosition {
                            offset,
                            affinity: self.caret_affinity,
                        },
                        2.0,
                    ),
                )
            })
        });
        let caret_visible = self.input_handler.focused
            && self.input_handler.view.read(cx).caret_blink_visible(cx)
            && self.marked_range.is_none();
        let cursor = caret_visible
            .then_some(caret_bounds)
            .flatten()
            .map(|bounds| fill(bounds, rgb(self.theme.focused)));
        let mut backgrounds = Vec::new();
        if let (Some(layout), Some(range)) = (&platform, self.selection_range.clone()) {
            backgrounds.extend(layout.range_rects(range).into_iter().map(|rect| {
                fill(
                    rect_to_bounds(bounds, rect),
                    rgba(text_selection_background(self.theme)),
                )
            }));
        }
        if let Some(layout) = &platform {
            for search in &self.search_ranges {
                backgrounds.extend(
                    layout
                        .range_rects(search.byte_range.clone())
                        .into_iter()
                        .map(|rect| {
                            fill(
                                rect_to_bounds(bounds, rect),
                                rgba(super::search_background(search.current)),
                            )
                        }),
                );
            }
        }
        if let Some(request) = &request {
            for fragment in &request.fragments {
                let origin = point(bounds.left(), bounds.top() + px(fragment.top_px));
                backgrounds.extend(text_background_quads(&fragment.snapshot, origin));
            }
        }
        let mut marked_underlines = Vec::new();
        if let (Some(layout), Some(range)) = (&platform, self.marked_range.clone()) {
            for mut rect in layout.range_rects(range) {
                backgrounds.push(fill(
                    rect_to_bounds(bounds, rect),
                    rgb(self.theme.action_background),
                ));
                rect.y += (rect.height - 1.0).max(0.0);
                rect.height = 1.0;
                marked_underlines.push(fill(rect_to_bounds(bounds, rect), rgb(self.theme.focused)));
            }
        }
        SegmentedPrepaintState {
            request,
            cursor,
            backgrounds,
            marked_underlines,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.input_handler.focused
            && let Some(target) = GuiPlatformInputTarget::from_surface_id(self.input.surface_id)
        {
            handle_registered_platform_input(
                &self.input_handler.view,
                &self.input_handler.focus,
                target,
                TextPlatformLayoutIdentity {
                    surface_id: self.input.surface_id,
                    content_version: self.input.content_version,
                    layout_version: self.input.layout_version,
                    wrap_width_bits: f32::from(bounds.size.width).to_bits(),
                    text_align: self.input.text_align,
                },
                bounds,
                window,
                cx,
            );
        }
        for background in prepaint.backgrounds.drain(..) {
            window.paint_quad(background);
        }
        if let Some(request) = &prepaint.request {
            for fragment in &request.fragments {
                paint_text_layout(
                    &fragment.snapshot,
                    point(bounds.left(), bounds.top() + px(fragment.top_px)),
                    false,
                    window,
                    cx,
                );
            }
        }
        for underline in prepaint.marked_underlines.drain(..) {
            window.paint_quad(underline);
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
        if let Some(request) = prepaint.request.take() {
            let estimated_line_count = request.text.split('\n').count().max(1);
            let mut cache = RichTextPlatformLayout {
                block_id: self.input.block_id,
                surface_id: self.input.surface_id,
                content_version: self.input.content_version,
                layout_version: self.input.layout_version,
                wrap_width_px: f32::from(bounds.size.width),
                text_align: self.input.text_align,
                input_session_identity: None,
                snapshot: super::PlatformTextLayoutSnapshot::Segmented(
                    SegmentedPlatformLayout::new(
                        request.text,
                        request.fragments,
                        estimated_line_count,
                    ),
                ),
                accessibility: None,
                bounds,
                measured_height: normalize_text_inner_measured_height(
                    &self.input.kind,
                    f64::from(request.total_height_px),
                )
                .height,
                table_cell_position: self.input_handler.table_cell_position,
            };
            self.input_handler.view.update(cx, |view, cx| {
                cache.input_session_identity = self
                    .input_handler
                    .focused
                    .then(|| view.registered_platform_input_session_identity())
                    .flatten();
                if publish_text_layout(view, cache) {
                    schedule_layout_correction_frame(view, window, cx);
                }
            });
        }
    }
}

fn rect_to_bounds(parent: Bounds<Pixels>, rect: super::TextLayoutRect) -> Bounds<Pixels> {
    Bounds::new(
        point(parent.left() + px(rect.x), parent.top() + px(rect.y)),
        Size {
            width: px(rect.width.max(0.0)),
            height: px(rect.height.max(0.0)),
        },
    )
}

#[cfg(test)]
#[path = "segmented_element_tests.rs"]
mod tests;
