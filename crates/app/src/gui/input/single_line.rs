use std::ops::Range;

use gpui::{
    App, Bounds, ContentMask, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Size,
    Style, TextAlign, TextRun, Window, fill, point, px, relative, rgb, transparent_black,
};

use crate::gui::input::ime::clamp_to_char_boundary;

pub const SINGLE_LINE_INPUT_FONT_SIZE_PX: f32 = 12.0;

pub struct SingleLineTextInputElement<T>
where
    T: EntityInputHandler + 'static,
{
    pub handler: Entity<T>,
    pub focus: FocusHandle,
    pub value: String,
    pub placeholder: Option<String>,
    pub caret_offset: Option<usize>,
    pub marked_range: Option<Range<usize>>,
    pub text_color: u32,
    pub placeholder_color: u32,
    pub caret_color: u32,
    pub font_size: Pixels,
}

impl<T> IntoElement for SingleLineTextInputElement<T>
where
    T: EntityInputHandler + 'static,
{
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<T> Element for SingleLineTextInputElement<T>
where
    T: EntityInputHandler + 'static,
{
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(
            &self.focus,
            ElementInputHandler::new(bounds, self.handler.clone()),
            cx,
        );

        let caret_offset = self.caret_offset.unwrap_or(self.value.len());
        let scroll_x = single_line_scroll_x_for_offset(
            &self.value,
            caret_offset,
            self.font_size,
            bounds,
            window,
        );
        let display = single_line_display_state(
            &self.value,
            self.placeholder.as_deref(),
            self.text_color,
            self.placeholder_color,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            paint_single_line_text(
                display.text,
                self.font_size,
                Hsla::from(rgb(display.color)),
                point(bounds.left() - px(scroll_x), bounds.top()),
                bounds.size.height,
                window,
                cx,
            );

            if let Some(marked_range) = self.marked_range.clone() {
                let max_x = single_line_input_max_x(bounds);
                let start = (single_line_text_x_for_offset(
                    &self.value,
                    marked_range.start,
                    self.font_size,
                    window,
                ) - scroll_x)
                    .max(0.0)
                    .min(max_x);
                let end = (single_line_text_x_for_offset(
                    &self.value,
                    marked_range.end,
                    self.font_size,
                    window,
                ) - scroll_x)
                    .max(0.0)
                    .min(max_x)
                    .max(start + 1.0);
                window.paint_quad(fill(
                    Bounds {
                        origin: point(bounds.left() + px(start), bounds.bottom() - px(3.0)),
                        size: Size {
                            width: px(end - start),
                            height: px(1.0),
                        },
                    },
                    rgb(self.caret_color),
                ));
            }

            if single_line_should_paint_caret(
                self.focus.is_focused(window),
                self.marked_range.as_ref(),
            ) {
                let x = (single_line_text_x_for_offset(
                    &self.value,
                    caret_offset,
                    self.font_size,
                    window,
                ) - scroll_x)
                    .max(0.0)
                    .min(single_line_input_max_x(bounds));
                window.paint_quad(fill(
                    Bounds {
                        origin: point(bounds.left() + px(x), bounds.top() + px(5.0)),
                        size: Size {
                            width: px(1.0),
                            height: bounds.size.height - px(10.0),
                        },
                    },
                    rgb(self.caret_color),
                ));
            }
        });
    }
}

fn single_line_should_paint_caret(focused: bool, marked_range: Option<&Range<usize>>) -> bool {
    focused && marked_range.is_none()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SingleLineDisplayState<'a> {
    text: &'a str,
    color: u32,
}

fn single_line_display_state<'a>(
    value: &'a str,
    placeholder: Option<&'a str>,
    text_color: u32,
    placeholder_color: u32,
) -> SingleLineDisplayState<'a> {
    if value.is_empty() {
        SingleLineDisplayState {
            text: placeholder.unwrap_or_default(),
            color: placeholder_color,
        }
    } else {
        SingleLineDisplayState {
            text: value,
            color: text_color,
        }
    }
}

fn paint_single_line_text(
    text: &str,
    font_size: Pixels,
    color: Hsla,
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    if text.is_empty() {
        return;
    }
    let runs = [TextRun {
        len: text.len(),
        font: gpui::Font::default(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }];
    if let Some(line) = window
        .text_system()
        .shape_text(text.into(), font_size, &runs, None, Some(1))
        .ok()
        .and_then(|mut lines| lines.pop())
    {
        line.paint(origin, line_height, TextAlign::Left, None, window, cx)
            .ok();
    }
}

pub fn single_line_visible_x_for_offset(
    text: &str,
    offset: usize,
    caret_offset: usize,
    font_size: Pixels,
    bounds: Bounds<Pixels>,
    window: &Window,
) -> f32 {
    (single_line_text_x_for_offset(text, offset, font_size, window)
        - single_line_scroll_x_for_offset(text, caret_offset, font_size, bounds, window))
    .max(0.0)
    .min(single_line_input_max_x(bounds))
}

pub fn single_line_visible_range_x(
    text: &str,
    range: Range<usize>,
    caret_offset: usize,
    font_size: Pixels,
    bounds: Bounds<Pixels>,
    window: &Window,
) -> Range<f32> {
    let scroll_x = single_line_scroll_x_for_offset(text, caret_offset, font_size, bounds, window);
    let max_x = single_line_input_max_x(bounds);
    let start = (single_line_text_x_for_offset(text, range.start, font_size, window) - scroll_x)
        .max(0.0)
        .min(max_x);
    let end = (single_line_text_x_for_offset(text, range.end, font_size, window) - scroll_x)
        .max(0.0)
        .min(max_x)
        .max(start + 1.0);
    start..end
}

pub fn single_line_local_x_for_point(
    point_x: Pixels,
    text: &str,
    caret_offset: usize,
    font_size: Pixels,
    bounds: Bounds<Pixels>,
    window: &Window,
) -> Pixels {
    let scroll_x = single_line_scroll_x_for_offset(text, caret_offset, font_size, bounds, window);
    px(single_line_text_x_from_point(
        f32::from(point_x),
        f32::from(bounds.left()),
        scroll_x,
        single_line_input_max_x(bounds),
    ))
}

pub fn single_line_text_x_from_point(
    point_x: f32,
    bounds_left: f32,
    scroll_x: f32,
    max_visible_x: f32,
) -> f32 {
    (point_x - bounds_left).max(0.0).min(max_visible_x) + scroll_x.max(0.0)
}

pub fn single_line_scroll_x_for_offset(
    text: &str,
    offset: usize,
    font_size: Pixels,
    bounds: Bounds<Pixels>,
    window: &Window,
) -> f32 {
    let caret_x = single_line_text_x_for_offset(text, offset, font_size, window);
    (caret_x - single_line_input_max_x(bounds)).max(0.0)
}

pub fn single_line_text_x_for_offset(
    text: &str,
    offset: usize,
    font_size: Pixels,
    window: &Window,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let offset = clamp_to_char_boundary(text, offset);
    f32::from(single_line_layout(text, font_size, window).x_for_index(offset))
}

pub fn single_line_text_offset_for_x(
    text: &str,
    x: Pixels,
    font_size: Pixels,
    window: &Window,
) -> usize {
    if text.is_empty() {
        return 0;
    }
    single_line_layout(text, font_size, window).closest_index_for_x(x)
}

pub fn single_line_input_max_x(bounds: Bounds<Pixels>) -> f32 {
    (f32::from(bounds.size.width) - 1.0).max(0.0)
}

fn single_line_layout(
    text: &str,
    font_size: Pixels,
    window: &Window,
) -> std::sync::Arc<gpui::LineLayout> {
    window.text_system().layout_line(
        text,
        font_size,
        &[TextRun {
            len: text.len(),
            font: gpui::Font::default(),
            color: transparent_black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, point, size};

    #[test]
    fn single_line_input_max_x_never_goes_negative() {
        assert_eq!(
            single_line_input_max_x(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(0.0), px(20.0)),
            }),
            0.0
        );
        assert_eq!(
            single_line_input_max_x(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(80.0), px(20.0)),
            }),
            79.0
        );
    }

    #[test]
    fn single_line_hides_custom_caret_while_ime_marked_range_is_active() {
        assert!(single_line_should_paint_caret(true, None));
        assert!(!single_line_should_paint_caret(true, Some(&(0..1))));
        assert!(!single_line_should_paint_caret(false, None));
    }

    #[test]
    fn single_line_display_state_uses_placeholder_only_when_value_is_empty() {
        assert_eq!(
            single_line_display_state("", Some("Search"), 0x111111, 0x999999),
            SingleLineDisplayState {
                text: "Search",
                color: 0x999999,
            }
        );
        assert_eq!(
            single_line_display_state("rust", Some("Search"), 0x111111, 0x999999),
            SingleLineDisplayState {
                text: "rust",
                color: 0x111111,
            }
        );
    }

    #[test]
    fn single_line_text_x_from_point_accounts_for_bounds_and_scroll() {
        assert_eq!(single_line_text_x_from_point(140.0, 100.0, 0.0, 80.0), 40.0);
        assert_eq!(
            single_line_text_x_from_point(140.0, 100.0, 24.0, 80.0),
            64.0
        );
        assert_eq!(single_line_text_x_from_point(90.0, 100.0, 24.0, 80.0), 24.0);
        assert_eq!(
            single_line_text_x_from_point(240.0, 100.0, 24.0, 80.0),
            104.0
        );
    }
}
