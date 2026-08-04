use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use gpui::{
    App, Bounds, Element, ElementId, Entity, FocusHandle, FontWeight, GlobalElementId,
    InspectorElementId, LayoutId, Pixels, ScrollHandle, Size, Style, Window, fill, point, px, rgb,
    rgba,
};

use crate::editor_view::{CditorV2View, GuiPlatformInputTarget};
use crate::input::platform_adapter::handle_registered_platform_input;
use crate::theme::GuiTheme;
use cditor_core::edit::TextAffinity;
use cditor_core::layout::normalize_text_inner_measured_height;
use cditor_core::rich_text::InlineSpan;
use cditor_runtime::TableCellPosition;

use super::background::text_selection_background;
use super::caret_reveal::reveal_caret_in_scroll_handle;
use super::layout_adapter::{paint_text_layout, text_background_quads};
use super::platform::RichTextPlatformLayout;
use super::{
    InlineBoxSpec, PositionedInlineBox, RichTextLayoutInput, TextLayoutCacheRequest,
    TextLayoutPosition, TextLayoutSelection, TextLayoutSnapshot, accessibility_node_ids,
    build_text_accessibility_projection, text_geometry_telemetry,
};

mod fallback;
mod input_handler;
mod layout_resolution;
pub(crate) mod metrics;
mod rich_text;
mod trace;
use fallback::deferred_placeholder_quads;
use layout_resolution::resolve_measured_layout;
#[cfg(test)]
pub(super) use metrics::{
    base_font_weight_for_kind, is_completed_todo, line_height_for_kind, text_color_for_kind,
    text_size_for_kind,
};
use metrics::{line_height_for, measured_wrap_width, text_layout_options, text_rect_to_bounds};
use rich_text::default_text_layout_for_input;
use trace::{input_trace_enabled, selection_trace_enabled, trace_input, trace_selection};

#[derive(Clone)]
pub struct RichTextElement {
    pub input: RichTextLayoutInput,
    pub theme: GuiTheme,
    pub caret_offset: Option<usize>,
    pub caret_affinity: TextAffinity,
    pub marked_range: Option<Range<usize>>,
    pub selection_range: Option<Range<usize>>,
    pub search_ranges: Vec<super::TextSearchRange>,
    pub base_text_color: Option<u32>,
    pub typography: RichTextTypography,
    pub placeholder_text: Option<String>,
    pub inline_boxes: Vec<InlineBoxSpec>,
    pub inline_box_renderer: Option<InlineBoxRenderer>,
    pub caret_reveal_scroll_handle: Option<ScrollHandle>,
    pub input_handler: Option<RichTextInputHandler>,
    pub require_prewarmed_layout: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RichTextTypography {
    pub font_size_px: Option<f32>,
    pub line_height_px: Option<f32>,
    pub font_weight: Option<FontWeight>,
}

pub type InlineBoxRenderer =
    Arc<dyn Fn(&PositionedInlineBox, Bounds<Pixels>, &mut Window, &mut App) + 'static>;

#[derive(Clone)]
pub struct RichTextInputHandler {
    pub view: Entity<CditorV2View>,
    pub focus: FocusHandle,
    pub focused: bool,
    pub table_cell_position: Option<TableCellPosition>,
}

struct RichTextGpuiElement {
    input: RichTextLayoutInput,
    theme: GuiTheme,
    caret_offset: Option<usize>,
    caret_affinity: TextAffinity,
    marked_range: Option<Range<usize>>,
    selection_range: Option<Range<usize>>,
    search_ranges: Vec<super::TextSearchRange>,
    base_text_color: Option<u32>,
    typography: RichTextTypography,
    placeholder_text: Option<String>,
    inline_boxes: Vec<InlineBoxSpec>,
    inline_box_renderer: Option<InlineBoxRenderer>,
    caret_reveal_scroll_handle: Option<ScrollHandle>,
    input_handler: Option<RichTextInputHandler>,
    require_prewarmed_layout: bool,
}

struct RichTextGpuiPrepaintState {
    layout: Option<TextLayoutSnapshot>,
    cursor: Option<gpui::PaintQuad>,
    inline_backgrounds: Vec<gpui::PaintQuad>,
    marked_backgrounds: Vec<gpui::PaintQuad>,
    marked_underlines: Vec<gpui::PaintQuad>,
    selection_backgrounds: Vec<gpui::PaintQuad>,
    search_backgrounds: Vec<gpui::PaintQuad>,
    deferred_placeholders: Vec<gpui::PaintQuad>,
}

impl Element for RichTextGpuiElement {
    type RequestLayoutState = Rc<RefCell<Option<Option<TextLayoutSnapshot>>>>;
    type PrepaintState = RichTextGpuiPrepaintState;

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
        let shared_layout = Rc::new(RefCell::new(None));
        let shared_layout_clone = shared_layout.clone();
        let mut input = self.input.clone();
        let placeholder = input
            .is_empty()
            .then(|| {
                self.placeholder_text
                    .clone()
                    .or_else(|| self.input_handler.is_none().then(|| "请输入...".to_owned()))
            })
            .flatten();
        if let Some(placeholder) = placeholder.as_ref() {
            input.spans = vec![InlineSpan::plain(placeholder)].into();
        }
        let theme = self.theme;
        let base_text_color = placeholder
            .is_some()
            .then_some(self.theme.muted)
            .or(self.base_text_color);
        let base_font = window.text_style().font();
        let font_family = crate::platform::BODY_FONT_FAMILY.to_string();
        let font_weight = base_font.weight;
        let font_style = base_font.style;
        let scale = window.scale_factor();
        let typography = self.typography;
        let inline_boxes = self.inline_boxes.clone();
        let require_prewarmed_layout = self.require_prewarmed_layout;
        let prewarm_view = self
            .input_handler
            .as_ref()
            .map(|handler| handler.view.clone());
        let cache_request = if self
            .input_handler
            .as_ref()
            .is_some_and(|handler| handler.focused)
        {
            TextLayoutCacheRequest::editing()
        } else {
            TextLayoutCacheRequest::visible()
        };
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.min_size.width = px(0.0).into();
        style.max_size.width = gpui::relative(1.0).into();
        let layout_id =
            window.request_measured_layout(style, move |known, available, _window, cx| {
                let wrap_width = measured_wrap_width(known.width, available.width, input.width_px);
                let options = text_layout_options(
                    &input,
                    theme,
                    base_text_color,
                    &font_family,
                    font_weight,
                    font_style,
                    scale,
                    Some(f32::from(wrap_width)),
                    typography,
                    inline_boxes.clone(),
                );
                let mut layout = resolve_measured_layout(
                    &input,
                    theme,
                    &options,
                    cache_request,
                    require_prewarmed_layout,
                );
                if layout.is_none()
                    && let Some(view) = prewarm_view.as_ref()
                {
                    view.update(cx, |view, cx| {
                        view.ensure_text_layout_prewarm(
                            input.clone(),
                            theme,
                            options.clone(),
                            cache_request,
                            cx,
                        );
                    });
                    layout = resolve_measured_layout(
                        &input,
                        theme,
                        &options,
                        cache_request,
                        require_prewarmed_layout,
                    );
                }
                let line_height = line_height_for(&input.kind, typography);
                let total_size = Size {
                    width: layout
                        .as_ref()
                        .map(|layout| px(layout.width()))
                        .or(known.width)
                        .unwrap_or(wrap_width),
                    height: layout
                        .as_ref()
                        .map(|layout| px(layout.height()))
                        .unwrap_or(line_height)
                        .max(line_height),
                };
                *shared_layout_clone.borrow_mut() = Some(layout);
                total_size
            });
        (layout_id, shared_layout)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let layout = request_layout.borrow_mut().take().flatten();
        let focused = self
            .input_handler
            .as_ref()
            .is_some_and(|handler| handler.focused);
        let caret_visible = self.input_handler.as_ref().is_some_and(|handler| {
            handler.focused && handler.view.read(cx).caret_blink_visible(cx)
        });
        let caret_bounds = if focused {
            self.caret_offset.and_then(|offset| {
                layout.as_ref().map(|layout| {
                    let rect = layout.caret_rect(
                        TextLayoutPosition {
                            offset,
                            affinity: self.caret_affinity,
                        },
                        2.0,
                    );
                    text_rect_to_bounds(bounds, rect)
                })
            })
        } else {
            None
        };
        if let (Some(scroll_handle), Some(caret_bounds)) =
            (self.caret_reveal_scroll_handle.as_ref(), caret_bounds)
        {
            reveal_caret_in_scroll_handle(scroll_handle, caret_bounds, window);
        }
        let cursor = if self.marked_range.is_none() && caret_visible {
            caret_bounds.map(|bounds| fill(bounds, rgb(self.theme.focused)))
        } else {
            None
        };
        let marked_rects = self
            .marked_range
            .clone()
            .and_then(|range| layout.as_ref().map(|layout| layout.range_rects(range)))
            .unwrap_or_default();
        let marked_backgrounds = marked_rects
            .iter()
            .map(|rect| {
                fill(
                    text_rect_to_bounds(bounds, *rect),
                    rgb(self.theme.action_background),
                )
            })
            .collect();
        let marked_underlines = marked_rects
            .iter()
            .map(|rect| {
                let mut rect = *rect;
                rect.y += (rect.height - 1.0).max(0.0);
                rect.height = 1.0;
                fill(text_rect_to_bounds(bounds, rect), rgb(self.theme.focused))
            })
            .collect();
        let selection_rects = self
            .selection_range
            .clone()
            .and_then(|range| layout.as_ref().map(|layout| layout.range_rects(range)))
            .unwrap_or_default();
        if selection_trace_enabled() && self.selection_range.is_some() {
            trace_selection(format_args!(
                "block={} kind={:?} range={:?} element_bounds={:?} layout_size={:?} line_count={} rects={:?}",
                self.input.block_id,
                self.input.kind,
                self.selection_range,
                bounds,
                layout
                    .as_ref()
                    .map(|layout| (layout.width(), layout.height())),
                layout.as_ref().map_or(0, |layout| layout.line_count()),
                selection_rects,
            ));
        }
        let selection_backgrounds = selection_rects
            .into_iter()
            .map(|rect| {
                fill(
                    text_rect_to_bounds(bounds, rect),
                    rgba(text_selection_background(self.theme)),
                )
            })
            .collect();
        let search_backgrounds = self
            .search_ranges
            .iter()
            .flat_map(|search| {
                layout
                    .as_ref()
                    .map(|layout| layout.range_rects(search.byte_range.clone()))
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |rect| {
                        fill(
                            text_rect_to_bounds(bounds, rect),
                            rgba(super::search_background(search.current)),
                        )
                    })
            })
            .collect();
        let inline_backgrounds = layout
            .as_ref()
            .map(|layout| text_background_quads(layout, bounds.origin))
            .unwrap_or_default();
        let deferred_placeholders = if layout.is_none() {
            deferred_placeholder_quads(bounds, &self.input.kind, self.typography, self.theme)
        } else {
            Vec::new()
        };
        RichTextGpuiPrepaintState {
            layout,
            cursor,
            inline_backgrounds,
            marked_backgrounds,
            marked_underlines,
            selection_backgrounds,
            search_backgrounds,
            deferred_placeholders,
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
        if let Some(input_handler) = self
            .input_handler
            .as_ref()
            .filter(|handler| handler.focused)
        {
            let Some(target) = GuiPlatformInputTarget::from_surface_id(self.input.surface_id)
            else {
                return;
            };
            handle_registered_platform_input(
                &input_handler.view,
                &input_handler.focus,
                target,
                super::TextPlatformLayoutIdentity {
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
            let geometry = text_geometry_telemetry();
            trace_input(
                "handle_input",
                format_args!(
                    "block={} content_version={} bounds_origin={:?} bounds_size={:?} caret={:?} selection={:?} marked={:?} geometry={:?} fallback_rate={:.6}",
                    self.input.block_id,
                    self.input.content_version,
                    bounds.origin,
                    bounds.size,
                    self.caret_offset,
                    self.selection_range,
                    self.marked_range,
                    geometry,
                    geometry.fallback_rate(),
                ),
            );
        }

        for background in prepaint.inline_backgrounds.drain(..) {
            window.paint_quad(background);
        }
        for background in prepaint.search_backgrounds.drain(..) {
            window.paint_quad(background);
        }
        for background in prepaint.selection_backgrounds.drain(..) {
            window.paint_quad(background);
        }
        for background in prepaint.marked_backgrounds.drain(..) {
            window.paint_quad(background);
        }
        for placeholder in prepaint.deferred_placeholders.drain(..) {
            window.paint_quad(placeholder);
        }
        if let Some(layout) = prepaint.layout.as_ref() {
            let report =
                paint_text_layout(layout, bounds.origin, input_trace_enabled(), window, cx);
            if input_trace_enabled()
                && (report.glyph_errors != 0
                    || report.font_registration_errors != 0
                    || report.synthesized_runs != 0
                    || report.variable_runs != 0
                    || report.collection_face_runs != 0
                    || report.inexact_font_runs != 0
                    || report.glyph_validation_mismatches != 0)
            {
                trace_input(
                    "text_layout.paint",
                    format_args!("block={} report={report:?}", self.input.block_id),
                );
            }
            if let Some(renderer) = self.inline_box_renderer.as_ref() {
                for inline_box in layout.inline_boxes() {
                    let inline_bounds = Bounds::new(
                        point(
                            bounds.left() + px(inline_box.x),
                            bounds.top() + px(inline_box.y),
                        ),
                        Size {
                            width: px(inline_box.width),
                            height: px(inline_box.height),
                        },
                    );
                    renderer(&inline_box, inline_bounds, window, cx);
                }
            }
        }
        for underline in prepaint.marked_underlines.drain(..) {
            window.paint_quad(underline);
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }

        if let (Some(input_handler), Some(layout)) =
            (self.input_handler.as_ref(), prepaint.layout.as_ref())
        {
            // Placeholder glyphs are visual only. Hit testing, IME geometry,
            // and accessibility must remain constrained to the real text.
            let interaction_layout = if self.placeholder_text.is_some() && self.input.is_empty() {
                default_text_layout_for_input(
                    &self.input,
                    self.theme,
                    self.base_text_color,
                    self.typography,
                    self.inline_boxes.clone(),
                    input_handler.focused,
                )
            } else {
                layout.clone()
            };
            let mut cache = RichTextPlatformLayout {
                block_id: self.input.block_id,
                surface_id: self.input.surface_id,
                content_version: self.input.content_version,
                layout_version: self.input.layout_version,
                wrap_width_px: f32::from(bounds.size.width),
                text_align: self.input.text_align,
                input_session_identity: None,
                snapshot: interaction_layout.clone().into(),
                accessibility: input_handler.focused.then(|| {
                    let cell = input_handler
                        .table_cell_position
                        .map(|position| (position.row, position.col));
                    let (parent_id, first_child_id) =
                        accessibility_node_ids(self.input.block_id, cell);
                    let selection = self
                        .selection_range
                        .clone()
                        .map(|range| TextLayoutSelection {
                            anchor: TextLayoutPosition::downstream(range.start),
                            focus: TextLayoutPosition {
                                offset: range.end,
                                affinity: TextAffinity::Upstream,
                            },
                        })
                        .or_else(|| {
                            self.caret_offset.map(|offset| {
                                let position = TextLayoutPosition {
                                    offset,
                                    affinity: self.caret_affinity,
                                };
                                TextLayoutSelection {
                                    anchor: position,
                                    focus: position,
                                }
                            })
                        });
                    build_text_accessibility_projection(
                        &interaction_layout,
                        parent_id,
                        first_child_id,
                        f64::from(bounds.left()),
                        f64::from(bounds.top()),
                        selection,
                    )
                }),
                bounds,
                measured_height: normalize_text_inner_measured_height(
                    &self.input.kind,
                    f64::from(bounds.size.height),
                )
                .height,
                table_cell_position: input_handler.table_cell_position,
            };
            input_handler.view.update(cx, |view, cx| {
                cache.input_session_identity = input_handler
                    .focused
                    .then(|| view.registered_platform_input_session_identity())
                    .flatten();
                if crate::cache::publish_text_layout(view, cache) {
                    crate::cache::schedule_layout_correction_frame(view, window, cx);
                }
            });
        }
    }
}
