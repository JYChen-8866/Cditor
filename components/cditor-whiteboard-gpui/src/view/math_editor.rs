use std::ops::Range;

use crate::shapes::ShapeId;
use crate::theme::{WhiteboardChrome, chrome};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, ParentElement,
    StatefulInteractiveElement, Styled, canvas, div, prelude::FluentBuilder, px, rgb,
};

use super::DrafftBoardView;
use crate::font::UI_FONT_FAMILY;

#[derive(Clone, Debug)]
pub(super) struct MathEditState {
    pub(super) shape_id: ShapeId,
    pub(super) latex: String,
    pub(super) caret: usize,
    pub(super) anchor: usize,
    pub(super) marked_range: Option<Range<usize>>,
    pub(super) created_new: bool,
}

impl MathEditState {
    pub(super) fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }
}

impl DrafftBoardView {
    pub(super) fn open_math_editor(
        &mut self,
        shape_id: ShapeId,
        created_new: bool,
        cx: &mut Context<Self>,
    ) {
        self.finish_text_edit(cx);
        let Some(latex) = self.board.math_latex(shape_id).map(str::to_string) else {
            return;
        };
        let caret = latex.len();
        self.math_edit = Some(MathEditState {
            shape_id,
            latex,
            caret,
            anchor: caret,
            marked_range: None,
            created_new,
        });
        cx.notify();
    }

    pub(super) fn apply_math_editor(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.math_edit.take() else {
            return;
        };
        let latex = if edit.latex.trim().is_empty() {
            r"x^2".to_string()
        } else {
            edit.latex
        };
        self.board
            .set_math_latex(edit.shape_id, latex, !edit.created_new);
        cx.notify();
    }

    pub(super) fn cancel_math_editor(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.math_edit.take() else {
            return;
        };
        if edit.created_new {
            self.board.cancel_new_math(edit.shape_id);
        }
        cx.notify();
    }

    pub(super) fn handle_math_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.math_edit.as_ref() else {
            return false;
        };
        let command = event.keystroke.modifiers.platform || event.keystroke.modifiers.control;
        let extend = event.keystroke.modifiers.shift;
        let content = edit.latex.clone();
        let selection = edit.selection();
        let caret = edit.caret;
        match event.keystroke.key.as_str() {
            "escape" => self.cancel_math_editor(cx),
            "enter" => self.apply_math_editor(cx),
            "backspace" => {
                let range = if selection.is_empty() {
                    previous_boundary(&content, caret)..caret
                } else {
                    selection
                };
                self.replace_math_range(range, "", cx);
            }
            "delete" => {
                let range = if selection.is_empty() {
                    caret..next_boundary(&content, caret)
                } else {
                    selection
                };
                self.replace_math_range(range, "", cx);
            }
            "left" => self.move_math_caret(previous_boundary(&content, caret), extend, cx),
            "right" => self.move_math_caret(next_boundary(&content, caret), extend, cx),
            "home" => self.move_math_caret(0, extend, cx),
            "end" => self.move_math_caret(content.len(), extend, cx),
            "a" if command => {
                if let Some(edit) = &mut self.math_edit {
                    edit.anchor = 0;
                    edit.caret = content.len();
                }
                cx.notify();
            }
            _ => return false,
        }
        true
    }

    pub(super) fn replace_math_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = &mut self.math_edit else {
            return false;
        };
        if range.start > range.end
            || range.end > edit.latex.len()
            || !edit.latex.is_char_boundary(range.start)
            || !edit.latex.is_char_boundary(range.end)
        {
            return false;
        }
        edit.latex.replace_range(range.clone(), replacement);
        edit.caret = range.start + replacement.len();
        edit.anchor = edit.caret;
        edit.marked_range = None;
        cx.notify();
        true
    }

    pub(super) fn move_math_caret(&mut self, caret: usize, extend: bool, cx: &mut Context<Self>) {
        if let Some(edit) = &mut self.math_edit {
            edit.caret = caret;
            if !extend {
                edit.anchor = caret;
            }
            edit.marked_range = None;
        }
        cx.notify();
    }

    pub(super) fn render_math_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let Some(edit) = &self.math_edit else {
            return div().into_any_element();
        };
        let selection = edit.selection();
        let prefix = edit.latex[..selection.start].to_string();
        let selected = edit.latex[selection.clone()].to_string();
        let suffix = edit.latex[selection.end..].to_string();
        let collapsed = selection.is_empty();
        let input_bounds = self.math_input_bounds.clone();

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x000000).opacity(0.28))
            .occlude()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(440.0))
                    .p(px(20.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgb(c.border))
                    .bg(rgb(c.bg))
                    .shadow_lg()
                    .child(div().text_size(px(15.0)).child("Edit equation"))
                    .child(
                        div()
                            .id("drafft-math-input")
                            .relative()
                            .h(px(42.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(rgb(c.accent))
                            .bg(rgb(c.bg))
                            .font_family(UI_FONT_FAMILY)
                            .text_size(px(14.0))
                            .child(
                                canvas(
                                    move |bounds, _, _| input_bounds.set(bounds),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(prefix)
                            .when(collapsed, |input| {
                                input.child(div().w(px(2.0)).h(px(19.0)).bg(rgb(c.accent)))
                            })
                            .when(!collapsed, |input| {
                                input.child(div().bg(rgb(c.accent).opacity(0.2)).child(selected))
                            })
                            .child(suffix),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(modal_button("Cancel", false, c).on_click(
                                cx.listener(|view, _, _, cx| view.cancel_math_editor(cx)),
                            ))
                            .child(modal_button("Apply", true, c).on_click(
                                cx.listener(|view, _, _, cx| view.apply_math_editor(cx)),
                            )),
                    ),
            )
            .into_any_element()
    }
}

fn modal_button(
    label: &'static str,
    primary: bool,
    c: WhiteboardChrome,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(("drafft-math-modal", usize::from(primary)))
        .h(px(32.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(if primary {
            rgb(c.accent)
        } else {
            rgb(c.border)
        })
        .bg(if primary { rgb(c.accent) } else { rgb(c.bg) })
        .text_color(if primary { rgb(c.bg) } else { rgb(c.text) })
        .cursor_pointer()
        .hover(move |style| style.bg(if primary { rgb(c.accent) } else { rgb(c.hover) }))
        .child(label)
}

fn previous_boundary(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_boundary(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text[offset..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| offset + index)
        .unwrap_or(text.len())
}
