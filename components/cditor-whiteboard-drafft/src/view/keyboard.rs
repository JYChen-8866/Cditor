use drafftink_core::tools::ToolKind;
use gpui::{Context, KeyDownEvent, KeyUpEvent, Window};

use crate::model_host::PointerOutcome;

use super::DrafftBoardView;

impl DrafftBoardView {
    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            cx.propagate();
            return;
        }
        if self.handle_text_key_down(event, cx) || self.handle_math_key_down(event, cx) {
            return;
        }
        if self.text_edit.is_some() || self.math_edit.is_some() {
            cx.propagate();
            return;
        }
        let keystroke = &event.keystroke;
        if matches!(keystroke.key.as_str(), "space" | " ") {
            if self.space_pressed {
                return;
            }
            self.space_pressed = true;
            let outcome =
                self.board
                    .pointer_down_with_options(self.last_pointer, false, false, false);
            if let PointerOutcome::BeginTextEdit(id) = outcome {
                self.begin_text_edit(id, false, cx);
            }
            cx.notify();
            return;
        }
        let command = keystroke.modifiers.platform || keystroke.modifiers.control;
        let handled = if command && keystroke.key == "g" {
            if keystroke.modifiers.shift {
                self.board.ungroup_selected()
            } else {
                self.board.group_selected()
            }
        } else if command && keystroke.key == "s" {
            self.save_document(false, cx);
            true
        } else if command && keystroke.key == "o" {
            self.open_document(cx);
            true
        } else if command && keystroke.key == "e" {
            self.export_png(keystroke.modifiers.shift, cx);
            true
        } else if command && keystroke.key == "c" && keystroke.modifiers.shift {
            self.export_png(true, cx);
            true
        } else if !command && !keystroke.modifiers.alt {
            match keystroke.key.as_str() {
                "escape" => {
                    self.board.cancel_pointer();
                    self.board.canvas.clear_selection();
                    self.pointer_interaction_active = false;
                    window.blur();
                    true
                }
                "backspace" | "delete" => self.board.delete_selected(),
                "h" => self.set_tool_from_key(ToolKind::Pan),
                "v" | "1" => self.set_tool_from_key(ToolKind::Select),
                "r" | "2" => self.set_tool_from_key(ToolKind::Rectangle),
                "o" | "4" => self.set_tool_from_key(ToolKind::Ellipse),
                "a" | "5" => self.set_tool_from_key(ToolKind::Arrow),
                "l" | "6" => self.set_tool_from_key(ToolKind::Line),
                "p" | "7" => self.set_tool_from_key(ToolKind::Freehand),
                "t" | "8" => self.set_tool_from_key(ToolKind::Text),
                "m" | "9" => self.set_tool_from_key(ToolKind::Math),
                "e" => self.set_tool_from_key(ToolKind::Eraser),
                "z" => self.set_tool_from_key(ToolKind::LaserPointer),
                _ => false,
            }
        } else {
            false
        };

        if handled {
            cx.notify();
        } else {
            cx.propagate();
        }
    }

    pub(super) fn on_key_up(
        &mut self,
        event: &KeyUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event.keystroke.key.as_str(), "space" | " ") {
            if !self.space_pressed {
                return;
            }
            self.space_pressed = false;
            match self.board.pointer_up(self.last_pointer, false) {
                PointerOutcome::BeginTextEdit(id) => self.begin_text_edit(id, true, cx),
                PointerOutcome::OpenMathEditor(id) => self.open_math_editor(id, true, cx),
                PointerOutcome::None => {}
            }
            cx.notify();
        } else {
            cx.propagate();
        }
    }

    fn set_tool_from_key(&mut self, tool: ToolKind) -> bool {
        self.board.set_tool(tool);
        true
    }
}
