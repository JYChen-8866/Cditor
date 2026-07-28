#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;
use std::{fmt, ops::Range};

use gpui::{
    App, Bounds, Context, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Point, Style,
    UTF16Selection, Window, point, px, relative, size,
};
use kurbo::Affine;

use crate::paint;
use drafftink_core::shapes::Shape;

use super::DrafftBoardView;

impl EntityInputHandler for DrafftBoardView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.platform_input_content()?;
        let range = utf16_range_to_utf8(&text, &range_utf16);
        actual_range.replace(utf8_range_to_utf16(&text, &range));
        Some(text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = self.platform_input_content()?;
        let (selection, reversed) = self.platform_input_selection()?;
        Some(UTF16Selection {
            range: utf8_range_to_utf16(&text, &selection),
            reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let text = self.platform_input_content()?;
        self.platform_marked_range()
            .map(|range| utf8_range_to_utf16(&text, &range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.set_platform_marked_range(None);
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.platform_input_content() else {
            return;
        };
        let range = range_utf16
            .as_ref()
            .map(|range| utf16_range_to_utf8(&text, range))
            .or_else(|| self.platform_marked_range())
            .or_else(|| self.platform_input_selection().map(|value| value.0));
        if let Some(range) = range {
            self.replace_platform_input_range(range, new_text, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.platform_input_content() else {
            return;
        };
        let range = range_utf16
            .as_ref()
            .map(|range| utf16_range_to_utf8(&text, range))
            .or_else(|| self.platform_marked_range())
            .or_else(|| self.platform_input_selection().map(|value| value.0));
        let Some(range) = range else {
            return;
        };
        let insert_start = range.start;
        if !self.replace_platform_input_range(range, new_text, cx) {
            return;
        }
        self.set_platform_marked_range(
            (!new_text.is_empty()).then_some(insert_start..insert_start + new_text.len()),
        );
        if let Some(relative_utf16) = new_selected_range_utf16 {
            let relative = utf16_range_to_utf8(new_text, &relative_utf16);
            self.set_platform_selection(insert_start + relative.start, insert_start + relative.end);
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let result = self.platform_bounds_for_range(range_utf16.clone(), bounds);
        trace_platform_input(
            "bounds_for_range",
            format_args!(
                "range_utf16={range_utf16:?} surface={bounds:?} text_edit={} math_edit={} result={result:?}",
                self.text_edit.is_some(),
                self.math_edit.is_some(),
            ),
        );
        result
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let text = self.platform_input_content()?;
        let caret = self.platform_input_selection()?.0.end;
        Some(utf8_to_utf16(&text, caret))
    }
}

impl DrafftBoardView {
    fn platform_bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        if self.math_edit.is_some() {
            let input_bounds = self.math_input_bounds.get();
            return (input_bounds.size.width > px(0.0)).then_some(input_bounds);
        }

        let edit = self.text_edit.as_ref()?;
        let shape = self.board.canvas.document.get_shape(edit.shape_id)?;
        let Shape::Text(text) = shape else {
            return None;
        };
        let range = utf16_range_to_utf8(&text.content, &range_utf16);
        let geometry = self.text_outline_engine.borrow_mut().prepare(text);
        let caret = geometry.caret_rect(range.end, 1.0);
        let shape_transform =
            paint::rotation_transform(shape, self.board.canvas.camera.transform());
        let transform = shape_transform
            * Affine::translate(text.position.to_vec2())
            * Affine::translate(geometry.origin_offset);
        let top_left = transform * kurbo::Point::new(caret.x0, caret.y0);
        let top_right = transform * kurbo::Point::new(caret.x1, caret.y0);
        let bottom_left = transform * kurbo::Point::new(caret.x0, caret.y1);
        let bottom_right = transform * kurbo::Point::new(caret.x1, caret.y1);
        let min_x = top_left
            .x
            .min(top_right.x)
            .min(bottom_left.x)
            .min(bottom_right.x);
        let max_x = top_left
            .x
            .max(top_right.x)
            .max(bottom_left.x)
            .max(bottom_right.x);
        let min_y = top_left
            .y
            .min(top_right.y)
            .min(bottom_left.y)
            .min(bottom_right.y);
        let max_y = top_left
            .y
            .max(top_right.y)
            .max(bottom_left.y)
            .max(bottom_right.y);
        let candidate = Bounds {
            origin: point(
                bounds.origin.x + px(min_x as f32),
                bounds.origin.y + px(min_y as f32),
            ),
            size: size(
                px((max_x - min_x).max(1.0) as f32),
                px((max_y - min_y).max(12.0) as f32),
            ),
        };
        Some(candidate)
    }

    fn platform_input_content(&self) -> Option<String> {
        if let Some(edit) = &self.math_edit {
            return Some(edit.latex.clone());
        }
        self.editing_content().map(str::to_string)
    }

    fn platform_input_selection(&self) -> Option<(Range<usize>, bool)> {
        if let Some(edit) = &self.math_edit {
            return Some((edit.selection(), edit.caret < edit.anchor));
        }
        self.text_edit
            .as_ref()
            .map(|edit| (edit.selection(), edit.caret < edit.anchor))
    }

    fn platform_marked_range(&self) -> Option<Range<usize>> {
        if let Some(edit) = &self.math_edit {
            return edit.marked_range.clone();
        }
        self.text_edit
            .as_ref()
            .and_then(|edit| edit.marked_range.clone())
    }

    fn set_platform_marked_range(&mut self, range: Option<Range<usize>>) {
        if let Some(edit) = &mut self.math_edit {
            edit.marked_range = range;
        } else if let Some(edit) = &mut self.text_edit {
            edit.marked_range = range;
        }
    }

    fn set_platform_selection(&mut self, anchor: usize, caret: usize) {
        if let Some(edit) = &mut self.math_edit {
            edit.anchor = anchor;
            edit.caret = caret;
        } else if let Some(edit) = &mut self.text_edit {
            edit.anchor = anchor;
            edit.caret = caret;
        }
    }

    fn replace_platform_input_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.math_edit.is_some() {
            self.replace_math_range(range, replacement, cx)
        } else {
            self.replace_editing_range(range, replacement, cx)
        }
    }
}

pub(super) struct DrafftTextInputElement {
    input: Entity<DrafftBoardView>,
}

impl DrafftTextInputElement {
    pub(super) fn new(input: Entity<DrafftBoardView>) -> Self {
        Self { input }
    }
}

impl IntoElement for DrafftTextInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for DrafftTextInputElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        let (focus, input_active) = {
            let view = self.input.read(cx);
            (
                view.focus.clone(),
                view.text_edit.is_some() || view.math_edit.is_some(),
            )
        };
        if focus.is_focused(window) {
            trace_platform_input(
                "owner.registered",
                format_args!("surface={bounds:?} input_active={input_active}"),
            );
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn trace_platform_input(event: &str, args: fmt::Arguments<'_>) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_INPUT")
            .ok()
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
    }) {
        eprintln!("[cditor][input][whiteboard][{event}] {args}");
    }
}

#[cfg(target_arch = "wasm32")]
fn trace_platform_input(_event: &str, _args: fmt::Arguments<'_>) {}

fn utf16_to_utf8(text: &str, offset: usize) -> usize {
    text.chars()
        .scan((0usize, 0usize), |state, character| {
            let current = *state;
            state.0 += character.len_utf16();
            state.1 += character.len_utf8();
            Some(current)
        })
        .find(|(utf16, _)| *utf16 >= offset)
        .map(|(_, utf8)| utf8)
        .unwrap_or(text.len())
}

fn utf8_to_utf16(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].encode_utf16().count()
}

fn utf16_range_to_utf8(text: &str, range: &Range<usize>) -> Range<usize> {
    utf16_to_utf8(text, range.start)..utf16_to_utf8(text, range.end)
}

fn utf8_range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    utf8_to_utf16(text, range.start)..utf8_to_utf16(text, range.end)
}

#[cfg(test)]
mod tests {
    use drafftink_core::tools::ToolKind;
    use gpui::TestAppContext;
    use kurbo::Point as KurboPoint;

    use super::*;
    use crate::PointerOutcome;

    #[gpui::test]
    fn focused_text_edit_routes_platform_input_to_the_drafft_document(cx: &mut TestAppContext) {
        let (board, cx) = cx.add_window_view(|_, cx| DrafftBoardView::new(cx));
        let shape_id = board.update(cx, |view, cx| {
            view.board.set_tool(ToolKind::Text);
            view.board
                .pointer_down(KurboPoint::new(40.0, 50.0), false, false);
            let PointerOutcome::BeginTextEdit(shape_id) =
                view.board.pointer_up(KurboPoint::new(40.0, 50.0), false)
            else {
                panic!("text creation should enter edit mode");
            };
            view.begin_text_edit(shape_id, true, cx);
            shape_id
        });
        cx.update(|window, _cx| {
            window.activate_window();
        });

        cx.simulate_input("hello中文");

        board.update(cx, |view, _| {
            assert_eq!(view.board.text_content(shape_id), Some("hello中文"));
            let edit = view.text_edit.as_ref().expect("text edit remains active");
            assert_eq!(edit.selection(), "hello中文".len().."hello中文".len());

            let surface = Bounds::new(point(px(120.0), px(80.0)), size(px(800.0), px(600.0)));
            let first = view.platform_bounds_for_range(1..1, surface).unwrap();
            let last = view.platform_bounds_for_range(7..7, surface).unwrap();
            assert!(last.left() > first.left());
            assert!(last.left() >= surface.left() && last.left() < surface.right());
            assert!(last.top() >= surface.top() && last.top() < surface.bottom());
            assert!(last.size.width > px(0.0));
            assert!(last.size.height > px(0.0));
        });
    }

    #[gpui::test]
    fn composition_preview_returns_window_bounds_from_the_owned_surface(cx: &mut TestAppContext) {
        let (board, cx) = cx.add_window_view(|_, cx| DrafftBoardView::new(cx));
        board.update(cx, |view, cx| {
            view.board.set_tool(ToolKind::Text);
            view.board
                .pointer_down(KurboPoint::new(90.0, 120.0), false, false);
            let PointerOutcome::BeginTextEdit(shape_id) =
                view.board.pointer_up(KurboPoint::new(90.0, 120.0), false)
            else {
                panic!("text creation should enter edit mode");
            };
            view.begin_text_edit(shape_id, true, cx);
        });
        cx.update(|window, cx| {
            window.activate_window();
            board.update(cx, |view, cx| {
                view.replace_and_mark_text_in_range(None, "ni", Some(2..2), window, cx);
            });
        });

        board.update(cx, |view, _| {
            assert_eq!(view.platform_marked_range(), Some(0..2));
            let surface = Bounds::new(point(px(240.0), px(160.0)), size(px(900.0), px(700.0)));
            let first = view.platform_bounds_for_range(1..1, surface).unwrap();
            let second = view.platform_bounds_for_range(2..2, surface).unwrap();
            assert!(second.left() > first.left());
            assert!(second.left() >= surface.left() && second.left() < surface.right());
            assert!(second.top() >= surface.top() && second.top() < surface.bottom());
            assert_ne!(second, Bounds::default());
        });
    }

    #[test]
    fn utf16_ranges_preserve_cjk_and_surrogate_pair_boundaries() {
        let text = "a中😀z";
        assert_eq!(utf16_range_to_utf8(text, &(1..2)), 1..4);
        assert_eq!(utf16_range_to_utf8(text, &(2..4)), 4..8);
        assert_eq!(utf8_range_to_utf16(text, &(4..8)), 2..4);
    }
}
