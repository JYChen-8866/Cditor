use gpui::prelude::FluentBuilder;
use std::{cell::RefCell, ops::Range, rc::Rc, sync::OnceLock};

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, Entity, FocusHandle, FontStyle,
    FontWeight, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, ParentElement,
    Pixels, SharedString, Size, StrikethroughStyle, Style, Styled, TextAlign, TextRun,
    UnderlineStyle, Window, WrappedLine as GpuiWrappedLine, div, fill, point, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::{CditorV2View, GuiPlatformInputTarget};
use crate::gui::input::platform_adapter::handle_registered_platform_input;
use crate::gui::rich_text::{
    NOTION_INLINE_CODE_RADIUS_PX, NOTION_INLINE_CODE_TEXT_SIZE_PX, NOTION_MONO_FONT_FAMILY,
    inline_mark_visual_style,
};
use cditor_core::layout::block_metrics::{
    NOTION_BODY_LINE_HEIGHT_PX, NOTION_HEADING_1_LINE_HEIGHT_PX, NOTION_HEADING_2_LINE_HEIGHT_PX,
    NOTION_HEADING_3_LINE_HEIGHT_PX,
};
use cditor_core::layout::normalize_text_inner_measured_height;
use cditor_core::rich_text::{InlineSpan, RichBlockKind};
use cditor_runtime::TableCellPosition;

use super::platform::{
    RichTextPlatformLayout, normalized_text_range, platform_cursor_bounds_for_offset,
    platform_range_segment_bounds,
};
use super::{RichTextLayoutInput, TextHitPoint, VisualRun, wrap_rich_text};

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
    pub marked_range: Option<Range<usize>>,
    pub selection_range: Option<Range<usize>>,
    pub input_handler: Option<RichTextInputHandler>,
}

impl RichTextElement {
    pub fn new(input: RichTextLayoutInput, theme: GuiTheme) -> Self {
        Self {
            input,
            theme,
            caret_offset: None,
            marked_range: None,
            selection_range: None,
            input_handler: None,
        }
    }

    pub fn with_caret(mut self, caret_offset: Option<usize>) -> Self {
        self.caret_offset = caret_offset;
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
        let text = self.plain_text();
        let layout = wrap_rich_text(&self.input);
        layout.offset_for_point(&text, point)
    }

    pub fn candidate_rect_for_offset(&self, offset: usize) -> super::TextCaretRect {
        let text = self.plain_text();
        let layout = wrap_rich_text(&self.input);
        layout.caret_rect_for_offset(&text, offset)
    }

    pub fn candidate_rect_for_caret(&self) -> Option<super::TextCaretRect> {
        self.caret_offset
            .map(|offset| self.candidate_rect_for_offset(offset))
    }

    pub(super) fn plain_text(&self) -> String {
        self.input
            .spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>()
    }

    pub fn render(&self) -> AnyElement {
        if let Some(input_handler) = self.input_handler.clone() {
            return RichTextGpuiElement {
                input: self.input.clone(),
                theme: self.theme,
                caret_offset: self.caret_offset,
                marked_range: self.marked_range.clone(),
                selection_range: self.selection_range.clone(),
                input_handler,
            }
            .into_any_element();
        }

        let text = self.plain_text();
        let layout = wrap_rich_text(&self.input);
        let caret_rect = self
            .caret_offset
            .filter(|_| self.marked_range.is_none())
            .map(|offset| layout.caret_rect_for_offset(&text, offset));

        let text_layer = if text.is_empty() {
            div()
                .min_h(px(layout.height as f32))
                .text_color(rgb(self.theme.muted))
                .child("请输入...")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .children(layout.lines.iter().map(|line| {
                    div()
                        .flex()
                        .items_baseline()
                        .min_h(px(line.height as f32))
                        .children(line.runs.iter().flat_map(|run| {
                            render_visual_run_segments(
                                &text,
                                run,
                                self.theme,
                                self.marked_range.as_ref(),
                            )
                        }))
                }))
                .into_any_element()
        };

        div()
            .relative()
            .child(text_layer)
            .when_some(caret_rect, |this, caret| {
                this.child(
                    div()
                        .absolute()
                        .left(px(caret.x as f32))
                        .top(px(caret.y as f32))
                        .w(px(caret.width as f32))
                        .h(px(caret.height as f32))
                        .bg(rgb(self.theme.focused)),
                )
            })
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
    marked_range: Option<Range<usize>>,
    selection_range: Option<Range<usize>>,
    input_handler: RichTextInputHandler,
}

struct RichTextGpuiPrepaintState {
    lines: Vec<GpuiWrappedLine>,
    cursor: Option<gpui::PaintQuad>,
    marked_backgrounds: Vec<gpui::PaintQuad>,
    line_height: Pixels,
}

impl IntoElement for RichTextGpuiElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for RichTextGpuiElement {
    type RequestLayoutState = Rc<RefCell<Option<Vec<GpuiWrappedLine>>>>;
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
        let shared_lines = Rc::new(RefCell::new(None));
        let shared_lines_clone = shared_lines.clone();
        let text = plain_text_from_spans(&self.input.spans);
        let runs = platform_text_runs(
            &self.input.spans,
            &self.input.kind,
            self.marked_range.as_ref(),
            self.theme,
            window,
        );
        let kind = self.input.kind.clone();
        let text_size = text_size_for_kind(&kind);
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
                match window.text_system().shape_text(
                    text.clone().into(),
                    text_size,
                    &runs,
                    wrap_width,
                    None,
                ) {
                    Ok(lines) => {
                        let lines = lines.into_vec();
                        let line_height = line_height_for_kind(&kind, text_size);
                        let mut total_size: Size<Pixels> = Size::default();
                        for line in &lines {
                            let size = line.size(line_height);
                            total_size.height += size.height;
                            total_size.width = total_size.width.max(size.width);
                        }
                        if lines.is_empty() {
                            total_size.height = line_height;
                        }
                        *shared_lines_clone.borrow_mut() = Some(lines);
                        total_size
                    }
                    Err(_) => Size::default(),
                }
            });
        (layout_id, shared_lines)
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
        let lines = request_layout.borrow_mut().take().unwrap_or_default();
        let text_size = text_size_for_kind(&self.input.kind);
        let line_height = line_height_for_kind(&self.input.kind, text_size);
        let text = plain_text_from_spans(&self.input.spans);
        let cursor = if self.input_handler.focused && self.marked_range.is_none() {
            self.caret_offset.and_then(|offset| {
                platform_cursor_bounds_for_offset(
                    &lines,
                    bounds,
                    line_height,
                    &text,
                    offset,
                    px(1.5),
                )
                .map(|bounds| fill(bounds, rgb(self.theme.focused)))
            })
        } else {
            None
        };
        let marked_backgrounds = self
            .marked_range
            .clone()
            .map(|range| {
                let text = plain_text_from_spans(&self.input.spans);
                platform_range_segment_bounds(&lines, bounds, line_height, &text, range)
                    .into_iter()
                    .map(|segment| fill(segment, rgb(self.theme.action_background)))
                    .collect()
            })
            .unwrap_or_default();
        RichTextGpuiPrepaintState {
            lines,
            cursor,
            marked_backgrounds,
            line_height,
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
        if self.input_handler.focused {
            let target = if let Some(position) = self.input_handler.table_cell_position {
                GuiPlatformInputTarget::TableCell {
                    block_id: self.input.block_id,
                    row: position.row,
                    col: position.col,
                }
            } else {
                GuiPlatformInputTarget::BlockText {
                    block_id: self.input.block_id,
                }
            };
            handle_registered_platform_input(
                &self.input_handler.view,
                &self.input_handler.focus,
                target,
                bounds,
                window,
                cx,
            );
            trace_input(
                "handle_input",
                format_args!(
                    "block={} content_version={} bounds_origin={:?} bounds_size={:?} caret={:?} selection={:?} marked={:?}",
                    self.input.block_id,
                    self.input.content_version,
                    bounds.origin,
                    bounds.size,
                    self.caret_offset,
                    self.selection_range,
                    self.marked_range
                ),
            );
        }

        let lines = std::mem::take(&mut prepaint.lines);
        let text = plain_text_from_spans(&self.input.spans);
        if let Some(selection_range) = self.selection_range.clone() {
            for segment in platform_range_segment_bounds(
                &lines,
                bounds,
                prepaint.line_height,
                &text,
                selection_range,
            ) {
                window.paint_quad(fill(segment, rgb(self.theme.action_background)));
            }
        }
        for background in prepaint.marked_backgrounds.drain(..) {
            window.paint_quad(background);
        }
        let mut y_offset = Pixels::default();
        for line in &lines {
            line.paint(
                point(bounds.left(), bounds.top() + y_offset),
                prepaint.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
            y_offset += line.size(prepaint.line_height).height;
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }

        let cache = RichTextPlatformLayout {
            block_id: self.input.block_id,
            content_version: self.input.content_version,
            text,
            lines,
            bounds,
            line_height: prepaint.line_height,
            measured_height: normalize_text_inner_measured_height(
                &self.input.kind,
                f64::from(bounds.size.height),
            )
            .height,
            table_cell_position: self.input_handler.table_cell_position,
        };
        self.input_handler.view.update(cx, |view, cx| {
            if view.update_text_layout_cache(cache) {
                cx.notify();
            }
        });
    }
}

fn platform_text_runs(
    spans: &[InlineSpan],
    kind: &RichBlockKind,
    marked_range: Option<&Range<usize>>,
    theme: GuiTheme,
    window: &Window,
) -> Vec<TextRun> {
    let text = plain_text_from_spans(spans);
    let mut base_font = window.text_style().font();
    base_font.weight = base_font_weight_for_kind(kind, base_font.weight);
    let base_text_color = text_color_for_kind(kind, theme);
    let base_color = Hsla::from(rgb(base_text_color));
    let completed_todo = is_completed_todo(kind);
    if spans.is_empty() {
        return vec![TextRun {
            len: text.len(),
            font: base_font,
            color: base_color,
            background_color: None,
            underline: None,
            strikethrough: completed_todo.then_some(StrikethroughStyle {
                thickness: px(1.0),
                color: Some(base_color),
            }),
        }];
    }

    let span_ranges = span_ranges(spans);
    let marked_range = marked_range.map(|range| normalized_text_range(&text, range.clone()));
    let mut boundaries = vec![0, text.len()];
    for (range, _) in &span_ranges {
        boundaries.push(range.start);
        boundaries.push(range.end);
    }
    if let Some(marked_range) = marked_range.as_ref() {
        boundaries.push(marked_range.start.min(text.len()));
        boundaries.push(marked_range.end.min(text.len()));
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut runs = Vec::new();
    let mut span_idx = 0usize;
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if start >= end {
            continue;
        }
        while span_idx < span_ranges.len() && span_ranges[span_idx].0.end <= start {
            span_idx += 1;
        }
        let marks = span_ranges
            .get(span_idx)
            .filter(|(range, _)| range.start <= start && start < range.end)
            .map(|(_, span)| span.marks.as_slice())
            .unwrap_or(&[]);
        let visual_style = inline_mark_visual_style(marks, theme, base_text_color);
        let mut font = base_font.clone();
        if visual_style.bold && font.weight < FontWeight::BOLD {
            font.weight = FontWeight::BOLD;
        }
        if visual_style.italic {
            font.style = FontStyle::Italic;
        }
        if visual_style.code {
            font.family = NOTION_MONO_FONT_FAMILY.into();
        }
        let color = Hsla::from(rgb(visual_style.text_color));
        let is_marked = marked_range
            .as_ref()
            .map(|range| start < range.end && range.start < end)
            .unwrap_or(false);
        let underline = (is_marked || visual_style.underline).then_some(UnderlineStyle {
            color: Some(color),
            thickness: px(1.0),
            wavy: false,
        });
        runs.push(TextRun {
            len: end - start,
            font,
            color,
            background_color: visual_style
                .background_color
                .map(|color| Hsla::from(rgb(color))),
            underline,
            strikethrough: (visual_style.strike || completed_todo).then_some(StrikethroughStyle {
                thickness: px(1.0),
                color: Some(color),
            }),
        });
    }
    runs
}

fn plain_text_from_spans(spans: &[InlineSpan]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

fn span_ranges(spans: &[InlineSpan]) -> Vec<(Range<usize>, &InlineSpan)> {
    let mut offset = 0usize;
    spans
        .iter()
        .map(|span| {
            let start = offset;
            offset += span.text.len();
            (start..offset, span)
        })
        .collect()
}

pub(super) fn text_size_for_kind(kind: &RichBlockKind) -> Pixels {
    match kind {
        RichBlockKind::Heading { level: 1 } => px(30.0),
        RichBlockKind::Heading { level: 2 } => px(24.0),
        RichBlockKind::Heading { .. } => px(20.0),
        RichBlockKind::Code { .. } => px(14.0),
        RichBlockKind::FootnoteDefinition => px(14.0),
        _ => px(16.0),
    }
}

pub(super) fn base_font_weight_for_kind(kind: &RichBlockKind, inherited: FontWeight) -> FontWeight {
    if matches!(kind, RichBlockKind::Heading { .. }) && inherited < FontWeight::SEMIBOLD {
        FontWeight::SEMIBOLD
    } else {
        inherited
    }
}

pub(super) fn line_height_for_kind(kind: &RichBlockKind, _text_size: Pixels) -> Pixels {
    match kind {
        RichBlockKind::Code { .. } => px(24.0),
        RichBlockKind::Heading { level: 1 } => px(NOTION_HEADING_1_LINE_HEIGHT_PX as f32),
        RichBlockKind::Heading { level: 2 } => px(NOTION_HEADING_2_LINE_HEIGHT_PX as f32),
        RichBlockKind::Heading { .. } => px(NOTION_HEADING_3_LINE_HEIGHT_PX as f32),
        RichBlockKind::FootnoteDefinition => px(20.0),
        _ => px(NOTION_BODY_LINE_HEIGHT_PX as f32),
    }
}

pub(super) fn text_color_for_kind(kind: &RichBlockKind, theme: GuiTheme) -> u32 {
    match kind {
        RichBlockKind::Code { .. } => theme.code_text,
        RichBlockKind::Quote => theme.quote_text,
        RichBlockKind::Todo { checked: true } => theme.muted,
        _ => theme.text,
    }
}

pub(super) fn is_completed_todo(kind: &RichBlockKind) -> bool {
    matches!(kind, RichBlockKind::Todo { checked: true })
}

pub(super) fn render_visual_run_segments(
    text: &str,
    run: &VisualRun,
    theme: GuiTheme,
    marked_range: Option<&Range<usize>>,
) -> Vec<AnyElement> {
    let Some(marked_range) = marked_range else {
        return vec![render_visual_run_segment(
            text,
            run,
            theme,
            run.logical_range.clone(),
            false,
        )];
    };
    let marked_start = run.logical_range.start.max(marked_range.start);
    let marked_end = run.logical_range.end.min(marked_range.end);
    if marked_start >= marked_end {
        return vec![render_visual_run_segment(
            text,
            run,
            theme,
            run.logical_range.clone(),
            false,
        )];
    }

    let mut segments = Vec::with_capacity(3);
    if run.logical_range.start < marked_start {
        segments.push(render_visual_run_segment(
            text,
            run,
            theme,
            run.logical_range.start..marked_start,
            false,
        ));
    }
    segments.push(render_visual_run_segment(
        text,
        run,
        theme,
        marked_start..marked_end,
        true,
    ));
    if marked_end < run.logical_range.end {
        segments.push(render_visual_run_segment(
            text,
            run,
            theme,
            marked_end..run.logical_range.end,
            false,
        ));
    }
    segments
}

fn render_visual_run_segment(
    text: &str,
    run: &VisualRun,
    theme: GuiTheme,
    range: Range<usize>,
    marked: bool,
) -> AnyElement {
    let label = text.get(range).unwrap_or_default().to_owned();
    div()
        .when(run.mark_style.code, |this| {
            this.px_1()
                .rounded(px(NOTION_INLINE_CODE_RADIUS_PX))
                .bg(rgb(theme.inline_code_background))
                .font_family(NOTION_MONO_FONT_FAMILY)
                .text_size(px(NOTION_INLINE_CODE_TEXT_SIZE_PX))
        })
        .when(run.mark_style.bold, |this| {
            this.font_weight(FontWeight::BOLD)
        })
        .when(run.mark_style.italic, |this| this.italic())
        .when(
            marked || run.mark_style.underline || run.mark_style.link,
            |this| this.text_decoration_1(),
        )
        .when(run.mark_style.strike, |this| this.line_through())
        .text_color(rgb(if run.mark_style.link {
            theme.focused
        } else if run.mark_style.code {
            theme.inline_code_text
        } else {
            theme.text
        }))
        .child(SharedString::from(label))
        .into_any_element()
}
