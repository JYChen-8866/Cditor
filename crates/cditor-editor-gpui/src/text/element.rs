use std::{
    cell::RefCell,
    ops::Range,
    rc::Rc,
    sync::{Arc, OnceLock},
};

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, Entity, FocusHandle, FontStyle,
    FontWeight, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Size, Style,
    Window, fill, point, px, rgb, rgba,
};

use crate::app::{CditorV2View, GuiPlatformInputTarget};
use crate::input::platform_adapter::handle_registered_platform_input;
use crate::theme::GuiTheme;
use cditor_core::edit::TextAffinity;
use cditor_core::layout::normalize_text_inner_measured_height;
use cditor_core::rich_text::InlineSpan;
use cditor_runtime::TableCellPosition;

use super::background::text_selection_background;
use super::parley_adapter::{paint_parley_layout, parley_background_quads};
use super::platform::RichTextPlatformLayout;
use super::{
    ParleyInlineBoxSpec, ParleyLayoutSnapshot, ParleyPositionedInlineBox, ParleySelection,
    ParleyTextPosition, RichTextLayoutInput, TextHitPoint, TextLayoutCacheRequest,
    accessibility_node_ids, build_parley_accessibility_projection,
    cached_parley_layout_with_request, text_geometry_telemetry,
};

mod metrics;
#[cfg(test)]
pub(super) use metrics::{
    base_font_weight_for_kind, is_completed_todo, line_height_for_kind, text_color_for_kind,
    text_size_for_kind,
};
use metrics::{
    line_height_for, parley_layout_options, parley_range_rects, parley_rect_to_bounds,
    plain_text_from_spans,
};

fn input_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_INPUT")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

fn trace_input(event: &str, details: impl std::fmt::Display) {
    if input_trace_enabled() {
        eprintln!("[cditor][input][text][{event}] {details}");
    }
}

#[derive(Clone)]
pub struct RichTextElement {
    pub input: RichTextLayoutInput,
    pub theme: GuiTheme,
    pub caret_offset: Option<usize>,
    pub caret_affinity: TextAffinity,
    pub marked_range: Option<Range<usize>>,
    pub selection_range: Option<Range<usize>>,
    pub base_text_color: Option<u32>,
    pub typography: RichTextTypography,
    pub placeholder_text: Option<String>,
    pub inline_boxes: Vec<ParleyInlineBoxSpec>,
    pub inline_box_renderer: Option<ParleyInlineBoxRenderer>,
    pub input_handler: Option<RichTextInputHandler>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RichTextTypography {
    pub font_size_px: Option<f32>,
    pub line_height_px: Option<f32>,
    pub font_weight: Option<FontWeight>,
}

pub type ParleyInlineBoxRenderer =
    Arc<dyn Fn(&ParleyPositionedInlineBox, Bounds<Pixels>, &mut Window, &mut App) + 'static>;

impl RichTextElement {
    pub fn new(input: RichTextLayoutInput, theme: GuiTheme) -> Self {
        Self {
            input,
            theme,
            caret_offset: None,
            caret_affinity: TextAffinity::Downstream,
            marked_range: None,
            selection_range: None,
            base_text_color: None,
            typography: RichTextTypography::default(),
            placeholder_text: None,
            inline_boxes: Vec::new(),
            inline_box_renderer: None,
            input_handler: None,
        }
    }

    pub fn with_caret(mut self, caret_offset: Option<usize>) -> Self {
        self.caret_offset = caret_offset;
        self
    }

    pub fn with_caret_affinity(mut self, affinity: TextAffinity) -> Self {
        self.caret_affinity = affinity;
        self
    }

    pub fn with_marked_range(mut self, marked_range: Option<Range<usize>>) -> Self {
        self.marked_range = marked_range;
        self
    }

    pub fn with_selection_range(mut self, selection_range: Option<Range<usize>>) -> Self {
        self.selection_range = selection_range;
        self
    }

    pub fn with_base_text_color(mut self, color: Option<u32>) -> Self {
        self.base_text_color = color;
        self
    }

    pub fn with_typography(mut self, typography: RichTextTypography) -> Self {
        self.typography = typography;
        self
    }

    pub fn with_placeholder(mut self, placeholder: Option<impl Into<String>>) -> Self {
        self.placeholder_text = placeholder.map(Into::into);
        self
    }

    pub fn with_inline_boxes(
        mut self,
        inline_boxes: Vec<ParleyInlineBoxSpec>,
        renderer: ParleyInlineBoxRenderer,
    ) -> Self {
        self.inline_boxes = inline_boxes;
        self.inline_box_renderer = Some(renderer);
        self
    }

    pub fn with_input_handler(
        mut self,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        focused: bool,
    ) -> Self {
        self.input_handler = Some(RichTextInputHandler {
            view,
            focus,
            focused,
            table_cell_position: None,
        });
        self
    }

    pub fn with_table_cell_input_handler(
        mut self,
        view: Entity<CditorV2View>,
        focus: FocusHandle,
        focused: bool,
        table_cell_position: TableCellPosition,
    ) -> Self {
        self.input_handler = Some(RichTextInputHandler {
            view,
            focus,
            focused,
            table_cell_position: Some(table_cell_position),
        });
        self
    }

    pub fn hit_test(&self, point: TextHitPoint) -> usize {
        self.hit_test_position(point).offset
    }

    pub fn hit_test_position(&self, point: TextHitPoint) -> ParleyTextPosition {
        self.default_parley_layout()
            .position_for_point(point.x as f32, point.y as f32)
    }

    pub fn candidate_rect_for_offset(&self, offset: usize) -> super::TextCaretRect {
        let rect = self
            .default_parley_layout()
            .caret_rect(ParleyTextPosition::downstream(offset), 1.0);
        super::TextCaretRect {
            x: rect.x as f64,
            y: rect.y as f64,
            width: rect.width as f64,
            height: rect.height as f64,
        }
    }

    pub fn candidate_rect_for_caret(&self) -> Option<super::TextCaretRect> {
        self.caret_offset
            .map(|offset| self.candidate_rect_for_offset(offset))
    }

    pub fn positioned_inline_boxes(&self) -> Vec<ParleyPositionedInlineBox> {
        self.default_parley_layout().inline_boxes()
    }

    fn default_parley_layout(&self) -> ParleyLayoutSnapshot {
        let cache_request = if self
            .input_handler
            .as_ref()
            .is_some_and(|handler| handler.focused)
        {
            TextLayoutCacheRequest::editing()
        } else {
            TextLayoutCacheRequest::visible()
        };
        cached_parley_layout_with_request(
            &self.input,
            self.theme,
            &parley_layout_options(
                &self.input,
                self.theme,
                self.base_text_color,
                "system-ui",
                FontWeight::NORMAL,
                FontStyle::Normal,
                1.0,
                Some(self.input.width_px as f32),
                self.typography,
                self.inline_boxes.clone(),
            ),
            cache_request,
        )
        .layout
    }

    pub fn render(&self) -> AnyElement {
        RichTextGpuiElement {
            input: self.input.clone(),
            theme: self.theme,
            caret_offset: self.caret_offset,
            caret_affinity: self.caret_affinity,
            marked_range: self.marked_range.clone(),
            selection_range: self.selection_range.clone(),
            base_text_color: self.base_text_color,
            typography: self.typography,
            placeholder_text: self.placeholder_text.clone(),
            inline_boxes: self.inline_boxes.clone(),
            inline_box_renderer: self.inline_box_renderer.clone(),
            input_handler: self.input_handler.clone(),
        }
        .into_any_element()
    }
}

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
    base_text_color: Option<u32>,
    typography: RichTextTypography,
    placeholder_text: Option<String>,
    inline_boxes: Vec<ParleyInlineBoxSpec>,
    inline_box_renderer: Option<ParleyInlineBoxRenderer>,
    input_handler: Option<RichTextInputHandler>,
}

struct RichTextGpuiPrepaintState {
    layout: Option<ParleyLayoutSnapshot>,
    cursor: Option<gpui::PaintQuad>,
    inline_backgrounds: Vec<gpui::PaintQuad>,
    marked_backgrounds: Vec<gpui::PaintQuad>,
    marked_underlines: Vec<gpui::PaintQuad>,
    selection_backgrounds: Vec<gpui::PaintQuad>,
}

impl IntoElement for RichTextGpuiElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for RichTextGpuiElement {
    type RequestLayoutState = Rc<RefCell<Option<ParleyLayoutSnapshot>>>;
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
        let placeholder = plain_text_from_spans(&input.spans)
            .is_empty()
            .then(|| {
                self.placeholder_text
                    .clone()
                    .or_else(|| self.input_handler.is_none().then(|| "请输入...".to_owned()))
            })
            .flatten();
        if let Some(placeholder) = placeholder.as_ref() {
            input.spans = vec![InlineSpan::plain(placeholder)];
        }
        let theme = self.theme;
        let base_text_color = placeholder
            .is_some()
            .then_some(self.theme.muted)
            .or(self.base_text_color);
        let base_font = window.text_style().font();
        let font_family = base_font.family.to_string();
        let font_weight = base_font.weight;
        let font_style = base_font.style;
        let scale = window.scale_factor();
        let typography = self.typography;
        let inline_boxes = self.inline_boxes.clone();
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
            window.request_measured_layout(style, move |known, available, window, _cx| {
                let wrap_width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    AvailableSpace::MinContent => Some(px(1.0)),
                    AvailableSpace::MaxContent => Some(window.viewport_size().width.max(px(1.0))),
                });
                let options = parley_layout_options(
                    &input,
                    theme,
                    base_text_color,
                    &font_family,
                    font_weight,
                    font_style,
                    scale,
                    wrap_width.map(f32::from),
                    typography,
                    inline_boxes.clone(),
                );
                let layout =
                    cached_parley_layout_with_request(&input, theme, &options, cache_request)
                        .layout;
                let line_height = line_height_for(&input.kind, typography);
                let total_size = Size {
                    width: px(layout.width()),
                    height: px(layout.height()).max(line_height),
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
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let layout = request_layout.borrow_mut().take();
        let focused = self
            .input_handler
            .as_ref()
            .is_some_and(|handler| handler.focused);
        let cursor = if focused && self.marked_range.is_none() {
            self.caret_offset.and_then(|offset| {
                layout.as_ref().map(|layout| {
                    let rect = layout.caret_rect(
                        ParleyTextPosition {
                            offset,
                            affinity: self.caret_affinity,
                        },
                        1.5,
                    );
                    fill(parley_rect_to_bounds(bounds, rect), rgb(self.theme.focused))
                })
            })
        } else {
            None
        };
        let marked_rects = self
            .marked_range
            .clone()
            .and_then(|range| {
                layout
                    .as_ref()
                    .map(|layout| parley_range_rects(layout, range))
            })
            .unwrap_or_default();
        let marked_backgrounds = marked_rects
            .iter()
            .map(|rect| {
                fill(
                    parley_rect_to_bounds(bounds, *rect),
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
                fill(parley_rect_to_bounds(bounds, rect), rgb(self.theme.focused))
            })
            .collect();
        let selection_backgrounds = self
            .selection_range
            .clone()
            .and_then(|range| {
                layout
                    .as_ref()
                    .map(|layout| parley_range_rects(layout, range))
            })
            .unwrap_or_default()
            .into_iter()
            .map(|rect| {
                fill(
                    parley_rect_to_bounds(bounds, rect),
                    rgba(text_selection_background(self.theme)),
                )
            })
            .collect();
        let inline_backgrounds = layout
            .as_ref()
            .map(|layout| parley_background_quads(layout, bounds.origin))
            .unwrap_or_default();
        RichTextGpuiPrepaintState {
            layout,
            cursor,
            inline_backgrounds,
            marked_backgrounds,
            marked_underlines,
            selection_backgrounds,
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
        for background in prepaint.selection_backgrounds.drain(..) {
            window.paint_quad(background);
        }
        for background in prepaint.marked_backgrounds.drain(..) {
            window.paint_quad(background);
        }
        if let Some(layout) = prepaint.layout.as_ref() {
            let report = paint_parley_layout(layout, bounds.origin, input_trace_enabled(), window);
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
                    "parley.paint",
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
            let mut cache = RichTextPlatformLayout {
                block_id: self.input.block_id,
                surface_id: self.input.surface_id,
                content_version: self.input.content_version,
                layout_version: self.input.layout_version,
                input_session_identity: None,
                snapshot: layout.clone(),
                accessibility: input_handler.focused.then(|| {
                    let cell = input_handler
                        .table_cell_position
                        .map(|position| (position.row, position.col));
                    let (parent_id, first_child_id) =
                        accessibility_node_ids(self.input.block_id, cell);
                    let selection = self
                        .selection_range
                        .clone()
                        .map(|range| ParleySelection {
                            anchor: ParleyTextPosition::downstream(range.start),
                            focus: ParleyTextPosition {
                                offset: range.end,
                                affinity: TextAffinity::Upstream,
                            },
                        })
                        .or_else(|| {
                            self.caret_offset.map(|offset| {
                                let position = ParleyTextPosition {
                                    offset,
                                    affinity: self.caret_affinity,
                                };
                                ParleySelection {
                                    anchor: position,
                                    focus: position,
                                }
                            })
                        });
                    build_parley_accessibility_projection(
                        layout,
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
                if view.update_text_layout_cache(cache) {
                    cx.notify();
                }
            });
        }
    }
}
