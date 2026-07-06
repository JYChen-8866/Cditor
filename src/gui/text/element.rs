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

mod platform;
mod visual;

use platform::*;
pub(crate) use platform::{platform_index_for_point, platform_range_bounds};
use visual::render_visual_run_segments;

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
