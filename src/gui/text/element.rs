use gpui::prelude::FluentBuilder;
use std::{cell::RefCell, ops::Range, rc::Rc, sync::OnceLock};

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, ElementInputHandler, Entity,
    FocusHandle, FontStyle, FontWeight, GlobalElementId, Hsla, InspectorElementId, IntoElement,
    LayoutId, ParentElement, Pixels, Point, SharedString, Size, Style, Styled, TextAlign, TextRun,
    UnderlineStyle, Window, WrappedLine as GpuiWrappedLine, div, fill, point, px, rgb, rgba, size,
};

use crate::core::layout::normalize_text_inner_measured_height;
use crate::core::rich_text::{InlineMark, InlineSpan, RichBlockKind};
use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;

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

    fn plain_text(&self) -> String {
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
}

pub(crate) struct RichTextPlatformLayout {
    pub block_id: crate::core::ids::BlockId,
    pub content_version: u64,
    pub text: String,
    pub lines: Vec<GpuiWrappedLine>,
    pub bounds: Bounds<Pixels>,
    pub line_height: Pixels,
    pub measured_height: f64,
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
        RichTextGpuiPrepaintState {
            lines,
            cursor,
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
            window.handle_input(
                &self.input_handler.focus,
                ElementInputHandler::new(bounds, self.input_handler.view.clone()),
                cx,
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
                window.paint_quad(fill(segment, rgba(0x0969da33)));
            }
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
    let base_font = window.text_style().font();
    let base_color = Hsla::from(rgb(text_color_for_kind(kind, theme)));
    if spans.is_empty() {
        return vec![TextRun {
            len: text.len(),
            font: base_font,
            color: base_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    }

    let span_ranges = span_ranges(spans);
    let mut boundaries = vec![0, text.len()];
    for (range, _) in &span_ranges {
        boundaries.push(range.start);
        boundaries.push(range.end);
    }
    if let Some(marked_range) = marked_range {
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
        let mut font = base_font.clone();
        if marks.iter().any(|mark| matches!(mark, InlineMark::Bold))
            && font.weight < FontWeight::BOLD
        {
            font.weight = FontWeight::BOLD;
        }
        if marks.iter().any(|mark| matches!(mark, InlineMark::Italic)) {
            font.style = FontStyle::Italic;
        }
        let is_link = marks
            .iter()
            .any(|mark| matches!(mark, InlineMark::Link { .. }));
        let color = if is_link {
            Hsla::from(rgb(theme.focused))
        } else {
            base_color
        };
        let is_marked = marked_range
            .map(|range| start < range.end && range.start < end)
            .unwrap_or(false);
        let underline = (is_marked
            || marks
                .iter()
                .any(|mark| matches!(mark, InlineMark::Underline | InlineMark::Link { .. })))
        .then_some(UnderlineStyle {
            color: Some(color),
            thickness: px(1.0),
            wavy: false,
        });
        runs.push(TextRun {
            len: end - start,
            font,
            color,
            background_color: marks
                .iter()
                .any(|mark| matches!(mark, InlineMark::Code))
                .then_some(Hsla::from(rgb(theme.code_background))),
            underline,
            strikethrough: None,
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

fn text_size_for_kind(kind: &RichBlockKind) -> Pixels {
    match kind {
        RichBlockKind::Heading { level: 1 } => px(28.0),
        RichBlockKind::Heading { level: 2 } => px(24.0),
        RichBlockKind::Heading { .. } => px(20.0),
        RichBlockKind::Code { .. } => px(14.0),
        _ => px(16.0),
    }
}

fn line_height_for_kind(kind: &RichBlockKind, text_size: Pixels) -> Pixels {
    match kind {
        RichBlockKind::Code { .. } => px(24.0),
        _ => text_size * 1.25,
    }
}

fn text_color_for_kind(kind: &RichBlockKind, theme: GuiTheme) -> u32 {
    match kind {
        RichBlockKind::Code { .. } => theme.code_text,
        RichBlockKind::Quote => theme.quote_text,
        _ => theme.text,
    }
}

pub(crate) fn platform_range_bounds(
    cache: &RichTextPlatformLayout,
    range: Range<usize>,
) -> Option<Bounds<Pixels>> {
    let segments = platform_range_segment_bounds(
        &cache.lines,
        cache.bounds,
        cache.line_height,
        &cache.text,
        range.clone(),
    );
    if segments.is_empty() {
        return platform_cursor_bounds_for_offset(
            &cache.lines,
            cache.bounds,
            cache.line_height,
            &cache.text,
            range.start,
            px(1.0),
        );
    }
    let mut union = segments[0];
    for segment in segments.iter().skip(1) {
        union = Bounds::from_corners(
            point(
                union.left().min(segment.left()),
                union.top().min(segment.top()),
            ),
            point(
                union.right().max(segment.right()),
                union.bottom().max(segment.bottom()),
            ),
        );
    }
    Some(union)
}

pub(crate) fn platform_index_for_point(
    cache: &RichTextPlatformLayout,
    position: Point<Pixels>,
) -> usize {
    if cache.text.is_empty() || cache.lines.is_empty() {
        return 0;
    }
    if position.y < cache.bounds.top() {
        return 0;
    }
    if position.y > cache.bounds.bottom() {
        return cache.text.len();
    }
    let ranges = hard_line_ranges(&cache.text);
    let relative_y = position.y - cache.bounds.top();
    let Some((line_idx, y_in_line)) =
        platform_wrapped_line_for_y(&cache.lines, cache.line_height, relative_y)
    else {
        return 0;
    };
    let Some(layout) = cache.lines.get(line_idx) else {
        return 0;
    };
    let offset_in_line = match layout.closest_index_for_position(
        point(position.x - cache.bounds.left(), y_in_line),
        cache.line_height,
    ) {
        Ok(index) | Err(index) => index,
    };
    ranges
        .get(line_idx)
        .map(|range| range.start + offset_in_line)
        .unwrap_or(0)
}

fn platform_cursor_bounds_for_offset(
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    offset: usize,
    cursor_width: Pixels,
) -> Option<Bounds<Pixels>> {
    let ranges = hard_line_ranges(text);
    let (line_idx, offset_in_line) = line_index_for_offset(&ranges, offset);
    let layout = lines.get(line_idx)?;
    let cursor_pos = platform_position_for_offset(layout, offset_in_line, line_height, true)?;
    let y_offset = bounds.top() + platform_wrapped_line_top(lines, line_height, line_idx);
    Some(Bounds::new(
        point(bounds.left() + cursor_pos.x, y_offset + cursor_pos.y),
        size(cursor_width, line_height),
    ))
}

fn platform_position_for_offset(
    line: &GpuiWrappedLine,
    offset: usize,
    line_height: Pixels,
    prefer_next_wrap_start: bool,
) -> Option<Point<Pixels>> {
    let offsets = platform_wrapped_row_offsets(line);
    for row_idx in 0..offsets.len().saturating_sub(1) {
        let row_start = offsets[row_idx];
        let row_end = offsets[row_idx + 1];
        let is_start_of_wrapped_row = prefer_next_wrap_start && row_idx > 0 && offset == row_start;
        if is_start_of_wrapped_row || (offset >= row_start && offset < row_end) {
            let row_start_x = line.unwrapped_layout.x_for_index(row_start);
            let x = line.unwrapped_layout.x_for_index(offset) - row_start_x;
            return Some(point(x, line_height * row_idx as f32));
        }
    }
    line.position_for_index(offset, line_height)
}

fn platform_range_segment_bounds(
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    text: &str,
    range: Range<usize>,
) -> Vec<Bounds<Pixels>> {
    if range.start >= range.end || lines.is_empty() {
        return Vec::new();
    }
    let ranges = hard_line_ranges(text);
    let (start_line, start_offset) = line_index_for_offset(&ranges, range.start);
    let (end_line, end_offset) = line_index_for_offset(&ranges, range.end);
    let mut segments = Vec::new();
    for line_idx in start_line..=end_line {
        let Some(hard_range) = ranges.get(line_idx) else {
            continue;
        };
        let line_start = if line_idx == start_line {
            start_offset
        } else {
            0
        };
        let line_end = if line_idx == end_line {
            end_offset
        } else {
            hard_range.len()
        };
        segments.extend(platform_range_segment_bounds_for_hard_line(
            lines,
            bounds,
            line_height,
            line_idx,
            line_start,
            line_end,
        ));
    }
    segments
}

fn platform_range_segment_bounds_for_hard_line(
    lines: &[GpuiWrappedLine],
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    line_idx: usize,
    start_offset: usize,
    end_offset: usize,
) -> Vec<Bounds<Pixels>> {
    let Some(line) = lines.get(line_idx) else {
        return Vec::new();
    };
    let line_top = bounds.top() + platform_wrapped_line_top(lines, line_height, line_idx);
    let offsets = platform_wrapped_row_offsets(line);
    let mut segments = Vec::new();
    for row_idx in 0..offsets.len().saturating_sub(1) {
        let row_start = offsets[row_idx];
        let row_end = offsets[row_idx + 1];
        let seg_start = start_offset.max(row_start).min(row_end);
        let seg_end = end_offset.min(row_end).max(row_start);
        if seg_start >= seg_end {
            continue;
        }
        let row_start_x = line.unwrapped_layout.x_for_index(row_start);
        let start_x = line.unwrapped_layout.x_for_index(seg_start) - row_start_x;
        let end_x = line.unwrapped_layout.x_for_index(seg_end) - row_start_x;
        let y = line_top + line_height * row_idx as f32;
        segments.push(Bounds::from_corners(
            point(bounds.left() + start_x, y),
            point(bounds.left() + end_x, y + line_height),
        ));
    }
    segments
}

fn hard_line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if ch == '\n' {
            ranges.push(start..index);
            start = index + ch.len_utf8();
        }
    }
    ranges.push(start..text.len());
    ranges
}

fn line_index_for_offset(ranges: &[Range<usize>], offset: usize) -> (usize, usize) {
    let clamped = offset.min(ranges.last().map(|range| range.end).unwrap_or(0));
    for (index, range) in ranges.iter().enumerate() {
        if clamped <= range.end {
            return (index, clamped.saturating_sub(range.start));
        }
    }
    let last = ranges.len().saturating_sub(1);
    (
        last,
        ranges
            .get(last)
            .map(|range| range.len())
            .unwrap_or_default(),
    )
}

fn platform_wrapped_line_top(
    lines: &[GpuiWrappedLine],
    line_height: Pixels,
    line_idx: usize,
) -> Pixels {
    lines.iter().take(line_idx).fold(px(0.0), |height, line| {
        height + line.size(line_height).height
    })
}

fn platform_wrapped_line_for_y(
    lines: &[GpuiWrappedLine],
    line_height: Pixels,
    relative_y: Pixels,
) -> Option<(usize, Pixels)> {
    let mut top = px(0.0);
    for (line_idx, line) in lines.iter().enumerate() {
        let height = line.size(line_height).height;
        if relative_y < top + height || line_idx + 1 == lines.len() {
            return Some((line_idx, (relative_y - top).max(px(0.0))));
        }
        top += height;
    }
    None
}

fn platform_wrapped_row_offsets(line: &GpuiWrappedLine) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(line.wrap_boundaries().len() + 2);
    offsets.push(0);
    for wrap_idx in 0..line.wrap_boundaries().len() {
        if let Some(offset) = platform_wrap_boundary_offset(line, wrap_idx) {
            offsets.push(offset.min(line.len()));
        }
    }
    offsets.push(line.len());
    offsets.dedup();
    offsets
}

fn platform_wrap_boundary_offset(line: &GpuiWrappedLine, wrap_idx: usize) -> Option<usize> {
    let boundary = line.wrap_boundaries().get(wrap_idx)?;
    let run = line.unwrapped_layout.runs.get(boundary.run_ix)?;
    let glyph = run.glyphs.get(boundary.glyph_ix)?;
    Some(glyph.index)
}

fn render_visual_run_segments(
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
                .rounded(px(4.0))
                .bg(rgb(theme.code_background))
                .font_family("Menlo")
                .text_size(px(13.0))
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
        } else {
            theme.text
        }))
        .child(SharedString::from(label))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use crate::core::rich_text::{InlineMark, InlineSpan, RichBlockKind};

    use super::*;

    #[test]
    fn rich_text_element_paints_spans() {
        let input = RichTextLayoutInput {
            block_id: 1,
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Paragraph,
            spans: vec![
                InlineSpan::plain("hello "),
                InlineSpan {
                    text: "bold".to_owned(),
                    marks: vec![InlineMark::Bold],
                },
            ],
            width_px: 320.0,
            theme_version: 1,
            font_version: 1,
        };

        let element = RichTextElement::new(input.clone(), GuiTheme::light())
            .with_caret(Some(6))
            .with_marked_range(Some(0..5));
        let layout = wrap_rich_text(&input);

        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].runs.len(), 2);
        assert!(layout.lines[0].runs[1].mark_style.bold);
        let _paintable = element.render();
    }

    #[test]
    fn rich_text_element_candidate_rect_tracks_caret_geometry() {
        let input = RichTextLayoutInput {
            block_id: 1,
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Paragraph,
            spans: vec![InlineSpan::plain("abcd")],
            width_px: 320.0,
            theme_version: 1,
            font_version: 1,
        };
        let element = RichTextElement::new(input, GuiTheme::light()).with_caret(Some(2));

        let rect = element.candidate_rect_for_caret().unwrap();

        assert!(rect.x > 0.0);
        assert_eq!(rect.y, 0.0);
        assert_eq!(rect.height, 22.0);
    }

    #[test]
    fn rich_text_element_hides_custom_caret_while_ime_marked_range_is_active() {
        let input = RichTextLayoutInput {
            block_id: 1,
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Paragraph,
            spans: vec![InlineSpan::plain("ab中cd")],
            width_px: 320.0,
            theme_version: 1,
            font_version: 1,
        };
        let element = RichTextElement::new(input.clone(), GuiTheme::light())
            .with_caret(Some("ab中".len()))
            .with_marked_range(Some(2.."ab中".len()));
        let text = element.plain_text();
        let layout = wrap_rich_text(&input);
        let caret_rect = element
            .caret_offset
            .filter(|_| element.marked_range.is_none())
            .map(|offset| layout.caret_rect_for_offset(&text, offset));

        assert!(caret_rect.is_none());
        let _paintable = element.render();
    }

    #[test]
    fn rich_text_element_marks_only_the_ime_subrange() {
        let input = RichTextLayoutInput {
            block_id: 1,
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Paragraph,
            spans: vec![InlineSpan::plain("ab中cd")],
            width_px: 320.0,
            theme_version: 1,
            font_version: 1,
        };
        let text = "ab中cd";
        let layout = wrap_rich_text(&input);
        let run = &layout.lines[0].runs[0];

        let segments =
            render_visual_run_segments(text, run, GuiTheme::light(), Some(&(2.."ab中".len())));

        assert_eq!(segments.len(), 3);
    }

    #[test]
    fn rich_text_element_hit_test() {
        let input = RichTextLayoutInput {
            block_id: 1,
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Paragraph,
            spans: vec![InlineSpan::plain("abcd")],
            width_px: 320.0,
            theme_version: 1,
            font_version: 1,
        };
        let element = RichTextElement::new(input, GuiTheme::light());

        assert_eq!(element.hit_test(TextHitPoint { x: 0.0, y: 0.0 }), 0);
        assert_eq!(element.hit_test(TextHitPoint { x: 1_000.0, y: 0.0 }), 4);
    }
}
