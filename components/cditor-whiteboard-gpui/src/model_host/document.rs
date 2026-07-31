use crate::canvas::CanvasDocument;

use super::DrafftBoard;

impl DrafftBoard {
    pub fn document_json(&self) -> Result<String, serde_json::Error> {
        self.canvas.document.to_json()
    }

    pub fn replace_document(&mut self, document: CanvasDocument) {
        drop(self.swap_document(document));
    }

    pub(crate) fn swap_document(&mut self, document: CanvasDocument) -> CanvasDocument {
        self.cancel_pointer();
        self.canvas.clear_selection();
        let previous = std::mem::replace(&mut self.canvas.document, document);
        self.history.clear();
        self.bump_document_revision();
        previous
    }

    pub fn clear_document(&mut self) -> bool {
        if self.canvas.document.is_empty() {
            return false;
        }
        self.cancel_pointer();
        let ids = self.canvas.document.z_order.clone();
        self.history.begin();
        self.capture_history_shapes(ids);
        self.capture_history_order();
        self.canvas.document.clear();
        self.canvas.clear_selection();
        self.mark_document_changed();
        true
    }
}

pub fn parse_document(path: &std::path::Path, content: &str) -> Result<CanvasDocument, String> {
    let excalidraw = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("excalidraw"));
    if excalidraw {
        CanvasDocument::from_excalidraw(content)
    } else {
        CanvasDocument::from_json(content).map_err(|error| error.to_string())
    }
}

pub fn parse_document_json(content: &str) -> Result<CanvasDocument, String> {
    let content = content.trim();
    if content.is_empty() || content == "{}" {
        return Ok(CanvasDocument::new());
    }
    CanvasDocument::from_json(content).map_err(|error| error.to_string())
}

/// Direct adapter of Drafft's library parsing/layout sequence.
pub fn parse_library(content: &str, fallback_name: &str) -> Option<(String, CanvasDocument)> {
    let items = crate::library_from_excalidrawlib(content)?;
    let shapes = crate::library_layout_grid(&items);
    let mut document = CanvasDocument::new();
    for shape in shapes {
        document.add_shape(shape);
    }
    let name = if fallback_name.trim().is_empty() {
        "Library".to_string()
    } else {
        fallback_name.to_string()
    };
    Some((name, document))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{Rectangle, Shape};
    use kurbo::Point;

    #[test]
    fn clear_is_undoable_and_replace_resets_selection() {
        let mut board = DrafftBoard::new();
        board
            .canvas
            .document
            .add_shape(Shape::Rectangle(Rectangle::new(Point::ZERO, 10.0, 10.0)));
        board.canvas.select_all();
        assert!(board.clear_document());
        assert!(board.canvas.document.is_empty());
        assert!(board.undo());
        assert_eq!(board.canvas.document.len(), 1);

        board.canvas.select_all();
        board.replace_document(CanvasDocument::new());
        assert!(board.selected().is_empty());
    }
}
