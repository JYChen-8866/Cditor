use std::ops::Range;

use crate::shapes::{Shape, ShapeId, ShapeTrait};
use gpui::{Context, KeyDownEvent};
use kurbo::Point;

use super::DrafftBoardView;

#[derive(Clone, Debug)]
pub(super) struct TextEditState {
    pub(super) shape_id: ShapeId,
    pub(super) caret: usize,
    pub(super) anchor: usize,
    pub(super) marked_range: Option<Range<usize>>,
    pub(super) created_new: bool,
    pub(super) transaction_started: bool,
}

impl TextEditState {
    pub(super) fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }
}

impl DrafftBoardView {
    pub(super) fn begin_text_edit(
        &mut self,
        shape_id: ShapeId,
        created_new: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .text_edit
            .as_ref()
            .is_some_and(|edit| edit.shape_id == shape_id)
        {
            return;
        }
        self.finish_text_edit(cx);
        let caret = self
            .board
            .text_content(shape_id)
            .map(str::len)
            .unwrap_or_default();
        self.text_edit = Some(TextEditState {
            shape_id,
            caret,
            anchor: caret,
            marked_range: None,
            created_new,
            transaction_started: created_new,
        });
        self.reset_text_caret_blink(cx);
    }

    pub(super) fn finish_text_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.text_edit.take() else {
            return;
        };
        self.text_caret_epoch = self.text_caret_epoch.wrapping_add(1);
        self.text_caret_visible = false;
        self.board
            .finish_text_edit(edit.shape_id, edit.created_new, edit.transaction_started);
        cx.notify();
    }

    pub(super) fn editing_content(&self) -> Option<&str> {
        let edit = self.text_edit.as_ref()?;
        self.board.text_content(edit.shape_id)
    }

    pub(super) fn replace_editing_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.text_edit.as_ref() else {
            return false;
        };
        let shape_id = edit.shape_id;
        let needs_transaction = !edit.transaction_started;
        if needs_transaction && !self.board.begin_text_edit_transaction(shape_id) {
            return false;
        }
        if !self
            .board
            .replace_text_range(shape_id, range.clone(), replacement)
        {
            return false;
        }
        let caret = range.start + replacement.len();
        if let Some(edit) = &mut self.text_edit {
            edit.caret = caret;
            edit.anchor = caret;
            edit.marked_range = None;
            edit.transaction_started = true;
        }
        self.reset_text_caret_blink(cx);
        true
    }

    pub(super) fn replace_text_selection(
        &mut self,
        replacement: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(range) = self.text_edit.as_ref().map(TextEditState::selection) else {
            return false;
        };
        self.replace_editing_range(range, replacement, cx)
    }

    pub(super) fn handle_text_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.text_edit.as_ref() else {
            return false;
        };
        let command = event.keystroke.modifiers.platform || event.keystroke.modifiers.control;
        let extend = event.keystroke.modifiers.shift;
        let content = self.editing_content().unwrap_or_default().to_string();
        let selection = edit.selection();
        let caret = edit.caret;

        match event.keystroke.key.as_str() {
            "escape" => self.finish_text_edit(cx),
            "enter" => {
                self.replace_text_selection("\n", cx);
            }
            "backspace" => {
                if selection.is_empty() {
                    let start = previous_boundary(&content, caret);
                    self.replace_editing_range(start..caret, "", cx);
                } else {
                    self.replace_editing_range(selection, "", cx);
                }
            }
            "delete" => {
                if selection.is_empty() {
                    let end = next_boundary(&content, caret);
                    self.replace_editing_range(caret..end, "", cx);
                } else {
                    self.replace_editing_range(selection, "", cx);
                }
            }
            "left" => self.move_text_caret(previous_boundary(&content, caret), extend, cx),
            "right" => self.move_text_caret(next_boundary(&content, caret), extend, cx),
            "home" => self.move_text_caret(line_start(&content, caret), extend, cx),
            "end" => self.move_text_caret(line_end(&content, caret), extend, cx),
            "a" if command => {
                if let Some(edit) = &mut self.text_edit {
                    edit.anchor = 0;
                    edit.caret = content.len();
                }
                self.reset_text_caret_blink(cx);
            }
            _ => return false,
        }
        true
    }

    pub(super) fn move_text_caret(&mut self, caret: usize, extend: bool, cx: &mut Context<Self>) {
        if let Some(edit) = &mut self.text_edit {
            edit.caret = caret;
            if !extend {
                edit.anchor = caret;
            }
            edit.marked_range = None;
        }
        self.reset_text_caret_blink(cx);
    }

    pub(super) fn place_text_caret(&mut self, screen: Point, extend: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.text_edit.as_ref() else {
            return;
        };
        let Some(Shape::Text(text)) = self.board.canvas.document.get_shape(edit.shape_id) else {
            return;
        };
        let world = self.board.canvas.camera.screen_to_world(screen);
        let local = if text.rotation.abs() > 0.001 {
            kurbo::Affine::rotate_about(-text.rotation, text.bounds().center()) * world
        } else {
            world
        };
        let geometry = self.text_outline_engine.borrow_mut().prepare(text);
        let layout_point = Point::new(
            local.x - text.position.x - geometry.origin_offset.x,
            local.y - text.position.y - geometry.origin_offset.y,
        );
        let caret = geometry.byte_index_for_point(layout_point);
        if let Some(edit) = &mut self.text_edit {
            edit.caret = caret;
            if !extend {
                edit.anchor = caret;
            }
        }
        self.reset_text_caret_blink(cx);
    }

    pub(super) fn reset_text_caret_blink(&mut self, cx: &mut Context<Self>) {
        self.text_caret_epoch = self.text_caret_epoch.wrapping_add(1);
        self.text_caret_visible = true;
        let epoch = self.text_caret_epoch;
        self.schedule_text_caret_tick(epoch, cx);
        cx.notify();
    }

    fn schedule_text_caret_tick(&self, epoch: u64, cx: &mut Context<Self>) {
        let tick = cx
            .background_executor()
            .timer(std::time::Duration::from_millis(500));
        cx.spawn(async move |view, cx| {
            let _ = tick.await;
            let _ = view.update(cx, |view, cx| {
                if view.text_edit.is_none() || view.text_caret_epoch != epoch {
                    return;
                }
                view.text_caret_visible = !view.text_caret_visible;
                view.schedule_text_caret_tick(epoch, cx);
                cx.notify();
            });
        })
        .detach();
    }
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

fn line_start(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn line_end(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text[offset..]
        .find('\n')
        .map(|index| offset + index)
        .unwrap_or(text.len())
}
