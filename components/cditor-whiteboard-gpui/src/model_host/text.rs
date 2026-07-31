use std::ops::Range;

use crate::shapes::{Shape, ShapeId};

use super::{DrafftBoard, PointerOutcome};

impl DrafftBoard {
    pub(crate) fn editable_shape_at(&mut self, screen: kurbo::Point) -> PointerOutcome {
        let world = self.canvas.camera.screen_to_world(screen);
        let Some(id) = self.shape_at(world, 5.0) else {
            return PointerOutcome::None;
        };
        let outcome = match self.canvas.document.get_shape(id) {
            Some(Shape::Text(_)) => PointerOutcome::BeginTextEdit(id),
            Some(Shape::Math(_)) => PointerOutcome::OpenMathEditor(id),
            _ => PointerOutcome::None,
        };
        if outcome != PointerOutcome::None {
            self.canvas.clear_selection();
            self.canvas.add_to_selection(id);
        }
        outcome
    }

    pub(crate) fn text_content(&self, id: ShapeId) -> Option<&str> {
        let Some(Shape::Text(text)) = self.canvas.document.get_shape(id) else {
            return None;
        };
        Some(&text.content)
    }

    pub(crate) fn begin_text_edit_transaction(&mut self, id: ShapeId) -> bool {
        if !matches!(self.canvas.document.get_shape(id), Some(Shape::Text(_))) {
            return false;
        }
        self.history.begin();
        self.capture_history_shapes([id]);
        true
    }

    pub(crate) fn replace_text_range(
        &mut self,
        id: ShapeId,
        range: Range<usize>,
        replacement: &str,
    ) -> bool {
        let Some(Shape::Text(text)) = self.canvas.document.get_shape_mut(id) else {
            return false;
        };
        if range.start > range.end
            || range.end > text.content.len()
            || !text.content.is_char_boundary(range.start)
            || !text.content.is_char_boundary(range.end)
        {
            return false;
        }
        let edit_char_pos = text.content[..range.start].chars().count();
        let old_char_count = text.content.chars().count();
        text.content.replace_range(range, replacement);
        text.sync_char_colors_after_edit(edit_char_pos, old_char_count);
        text.invalidate_cache();
        self.bump_document_revision();
        true
    }

    pub(crate) fn finish_text_edit(&mut self, id: ShapeId, created_new: bool, changed: bool) {
        let empty = self
            .text_content(id)
            .is_some_and(|content| content.trim().is_empty());
        if empty {
            self.history.begin();
            self.capture_history_shapes([id]);
            self.capture_history_order();
            self.canvas.remove_shape(id);
            self.mark_document_changed();
            return;
        }
        if !created_new && !changed {
            // No transaction was opened for an unchanged existing text shape.
            return;
        }
        self.history.commit(&self.canvas.document);
        self.canvas.clear_selection();
        self.canvas.add_to_selection(id);
    }
}
