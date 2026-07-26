use drafftink_core::shapes::{Shape, ShapeId};

use super::DrafftBoard;

impl DrafftBoard {
    pub(crate) fn math_latex(&self, id: ShapeId) -> Option<&str> {
        let Some(Shape::Math(math)) = self.canvas.document.get_shape(id) else {
            return None;
        };
        Some(&math.latex)
    }

    pub(crate) fn set_math_latex(&mut self, id: ShapeId, latex: String, push_undo: bool) -> bool {
        if !matches!(self.canvas.document.get_shape(id), Some(Shape::Math(_))) {
            return false;
        }
        if push_undo {
            self.history.begin();
            self.capture_history_shapes([id]);
        }
        let Some(Shape::Math(math)) = self.canvas.document.get_shape_mut(id) else {
            return false;
        };
        math.set_latex(latex);
        self.mark_document_changed();
        true
    }

    pub(crate) fn cancel_new_math(&mut self, id: ShapeId) {
        if matches!(self.canvas.document.get_shape(id), Some(Shape::Math(_))) {
            self.history.begin();
            self.capture_history_shapes([id]);
            self.capture_history_order();
            self.canvas.remove_shape(id);
            self.mark_document_changed();
        }
    }
}
