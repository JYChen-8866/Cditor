use gpui::{App, ClipboardEntry, ClipboardItem, Context, KeyBinding, Window, actions};
use kurbo::Vec2;

use super::DrafftBoardView;

pub const DRAFFT_KEY_CONTEXT: &str = "DrafftBoard";

actions!(
    drafft_board,
    [
        Newline,
        Cancel,
        MoveLeft,
        MoveRight,
        SelectLeft,
        SelectRight,
        MoveToStart,
        MoveToEnd,
        SelectToStart,
        SelectToEnd,
        DeleteBackward,
        DeleteForward,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        Duplicate,
        Ignore,
    ]
);

pub fn bind_drafft_keys(cx: &mut App) {
    let context = Some(DRAFFT_KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("escape", Cancel, context),
        KeyBinding::new("left", MoveLeft, context),
        KeyBinding::new("right", MoveRight, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("home", MoveToStart, context),
        KeyBinding::new("end", MoveToEnd, context),
        KeyBinding::new("shift-home", SelectToStart, context),
        KeyBinding::new("shift-end", SelectToEnd, context),
        KeyBinding::new("backspace", DeleteBackward, context),
        KeyBinding::new("delete", DeleteForward, context),
        KeyBinding::new("secondary-a", SelectAll, context),
        KeyBinding::new("secondary-c", Copy, context),
        KeyBinding::new("secondary-x", Cut, context),
        KeyBinding::new("secondary-v", Paste, context),
        KeyBinding::new("secondary-z", Undo, context),
        KeyBinding::new("secondary-shift-z", Redo, context),
        KeyBinding::new("secondary-d", Duplicate, context),
        KeyBinding::new("tab", Ignore, context),
        KeyBinding::new("shift-tab", Ignore, context),
        KeyBinding::new("secondary-b", Ignore, context),
        KeyBinding::new("secondary-i", Ignore, context),
        KeyBinding::new("secondary-u", Ignore, context),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("secondary-y", Redo, context)]);
}

impl DrafftBoardView {
    pub(super) fn handle_newline_action(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            self.finish_action(cx);
            return;
        }
        if self.text_edit.is_some() {
            self.replace_text_selection("\n", cx);
        } else if self.math_edit.is_some() {
            self.apply_math_editor(cx);
        }
        self.finish_action(cx);
    }

    pub(super) fn handle_cancel_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.text_edit.is_some() {
            self.finish_text_edit(cx);
        } else if self.math_edit.is_some() {
            self.cancel_math_editor(cx);
        } else {
            self.board.cancel_pointer();
            self.board.canvas.clear_selection();
            self.pointer_interaction_active = false;
            window.blur();
        }
        self.finish_action(cx);
    }

    pub(super) fn handle_horizontal_action(
        &mut self,
        backwards: bool,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            self.finish_action(cx);
            return;
        }
        if let Some(edit) = self.text_edit.as_ref() {
            let content = self.editing_content().unwrap_or_default().to_owned();
            let caret = if backwards {
                previous_boundary(&content, edit.caret)
            } else {
                next_boundary(&content, edit.caret)
            };
            self.move_text_caret(caret, extend, cx);
        } else if let Some(edit) = self.math_edit.as_ref() {
            let caret = if backwards {
                previous_boundary(&edit.latex, edit.caret)
            } else {
                next_boundary(&edit.latex, edit.caret)
            };
            self.move_math_caret(caret, extend, cx);
        }
        self.finish_action(cx);
    }

    pub(super) fn handle_line_edge_action(
        &mut self,
        start: bool,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            self.finish_action(cx);
            return;
        }
        if let Some(edit) = self.text_edit.as_ref() {
            let content = self.editing_content().unwrap_or_default().to_owned();
            let caret = if start {
                line_start(&content, edit.caret)
            } else {
                line_end(&content, edit.caret)
            };
            self.move_text_caret(caret, extend, cx);
        } else if let Some(edit) = self.math_edit.as_ref() {
            self.move_math_caret(if start { 0 } else { edit.latex.len() }, extend, cx);
        }
        self.finish_action(cx);
    }

    pub(super) fn handle_delete_action(&mut self, backwards: bool, cx: &mut Context<Self>) {
        if self.read_only {
            self.finish_action(cx);
            return;
        }
        if let Some(edit) = self.text_edit.as_ref() {
            let content = self.editing_content().unwrap_or_default().to_owned();
            let range = edit.selection();
            let range = if !range.is_empty() {
                range
            } else if backwards {
                previous_boundary(&content, edit.caret)..edit.caret
            } else {
                edit.caret..next_boundary(&content, edit.caret)
            };
            self.replace_editing_range(range, "", cx);
        } else if let Some(edit) = self.math_edit.as_ref() {
            let range = edit.selection();
            let range = if !range.is_empty() {
                range
            } else if backwards {
                previous_boundary(&edit.latex, edit.caret)..edit.caret
            } else {
                edit.caret..next_boundary(&edit.latex, edit.caret)
            };
            self.replace_math_range(range, "", cx);
        } else {
            self.board.delete_selected();
        }
        self.finish_action(cx);
    }

    pub(super) fn handle_select_all_action(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            self.finish_action(cx);
            return;
        }
        if let Some(edit) = &mut self.text_edit {
            edit.anchor = 0;
            edit.caret = self.board.text_content(edit.shape_id).map_or(0, str::len);
            self.reset_text_caret_blink(cx);
        } else if let Some(edit) = &mut self.math_edit {
            edit.anchor = 0;
            edit.caret = edit.latex.len();
        } else {
            self.board.select_all();
        }
        self.finish_action(cx);
    }

    pub(super) fn handle_copy_action(&mut self, cut: bool, cx: &mut Context<Self>) {
        if self.read_only && cut {
            self.finish_action(cx);
            return;
        }
        let editing_selection = self
            .text_edit
            .as_ref()
            .and_then(|edit| {
                self.editing_content()
                    .map(|text| (text.to_owned(), edit.selection()))
            })
            .or_else(|| {
                self.math_edit
                    .as_ref()
                    .map(|edit| (edit.latex.clone(), edit.selection()))
            });
        if let Some((text, range)) = editing_selection {
            if !range.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text[range.clone()].to_owned()));
                if cut {
                    if self.text_edit.is_some() {
                        self.replace_editing_range(range, "", cx);
                    } else {
                        self.replace_math_range(range, "", cx);
                    }
                }
            }
        } else {
            let json = if cut {
                self.board.cut_selected_json()
            } else {
                self.board.copy_selected_json()
            };
            if let Some(json) = json {
                self.shape_clipboard = Some(json.clone());
                cx.write_to_clipboard(ClipboardItem::new_string(json));
            }
        }
        self.finish_action(cx);
    }

    pub(super) fn handle_paste_action(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            self.finish_action(cx);
            return;
        }
        let clipboard = cx.read_from_clipboard();
        let clipboard_text = clipboard.as_ref().and_then(ClipboardItem::text);
        if self.text_edit.is_some() {
            if let Some(text) = clipboard_text.as_deref() {
                self.replace_text_selection(text, cx);
            }
            self.finish_action(cx);
            return;
        }
        if self.math_edit.is_some() {
            if let Some(text) = clipboard_text.as_deref() {
                let range = self.math_edit.as_ref().unwrap().selection();
                self.replace_math_range(range, text, cx);
            }
            self.finish_action(cx);
            return;
        }

        let center = self.board.canvas.camera.screen_to_world(self.last_pointer);
        let text = clipboard_text.or_else(|| self.shape_clipboard.clone());
        let pasted_text = text
            .as_deref()
            .is_some_and(|text| self.board.paste_text_at(text, center));
        let _pasted_image = !pasted_text
            && clipboard.as_ref().is_some_and(|item| {
                item.entries().iter().any(|entry| {
                    let ClipboardEntry::Image(image) = entry else {
                        return false;
                    };
                    let Ok(decoded) = image::load_from_memory(image.bytes()) else {
                        return false;
                    };
                    let format = match image.format() {
                        gpui::ImageFormat::Png => crate::shapes::ImageFormat::Png,
                        gpui::ImageFormat::Jpeg => crate::shapes::ImageFormat::Jpeg,
                        gpui::ImageFormat::Webp => crate::shapes::ImageFormat::WebP,
                        _ => return false,
                    };
                    self.board.paste_image_at(
                        image.bytes(),
                        decoded.width(),
                        decoded.height(),
                        format,
                        center,
                    )
                })
            });
        self.finish_action(cx);
    }

    pub(super) fn handle_history_action(&mut self, redo: bool, cx: &mut Context<Self>) {
        if self.read_only {
            self.finish_action(cx);
            return;
        }
        if redo {
            self.board.redo();
        } else {
            self.board.undo();
        }
        self.finish_action(cx);
    }

    pub(super) fn handle_duplicate_action(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            self.finish_action(cx);
            return;
        }
        let offset = Vec2::new(16.0, 16.0) / self.board.canvas.camera.zoom.max(0.1);
        self.board.duplicate_selected(offset);
        self.finish_action(cx);
    }

    pub(super) fn ignore_bound_action(&self, cx: &mut Context<Self>) {
        cx.stop_propagation();
    }

    fn finish_action(&mut self, cx: &mut Context<Self>) {
        cx.stop_propagation();
        cx.notify();
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
