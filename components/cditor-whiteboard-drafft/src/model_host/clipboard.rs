use drafftink_core::shapes::{Image, ImageFormat, Shape};
use kurbo::{Affine, Point, Rect, Vec2};

use super::DrafftBoard;

impl DrafftBoard {
    /// Serializes selected shapes in document order so paste preserves stacking.
    pub fn copy_selected_json(&self) -> Option<String> {
        let shapes = self
            .canvas
            .document
            .z_order
            .iter()
            .filter(|id| self.canvas.selection.contains(id))
            .filter_map(|id| self.canvas.document.get_shape(*id).cloned())
            .collect::<Vec<_>>();
        (!shapes.is_empty())
            .then(|| serde_json::to_string(&shapes).ok())
            .flatten()
    }

    pub fn cut_selected_json(&mut self) -> Option<String> {
        let json = self.copy_selected_json()?;
        self.begin_history_transaction();
        self.capture_history_order();
        for id in self.canvas.selection.clone() {
            self.canvas.document.remove_shape(id);
        }
        self.canvas.clear_selection();
        self.mark_document_changed();
        Some(json)
    }

    /// Imports Drafft shape JSON, Excalidraw clipboard JSON, or Mermaid source.
    pub fn paste_text_at(&mut self, text: &str, center_world: Point) -> bool {
        let shapes = serde_json::from_str::<Vec<Shape>>(text)
            .ok()
            .map(regenerate_shape_ids)
            .or_else(|| {
                drafftink_core::canvas::CanvasDocument::shapes_from_excalidraw_clipboard(text)
            })
            .or_else(|| drafftink_core::shapes_from_mermaid(text));
        let Some(shapes) = shapes else {
            return false;
        };
        self.place_shapes_centered_at(shapes, center_world)
    }

    pub fn duplicate_selected(&mut self, offset: Vec2) -> bool {
        let Some(json) = self.copy_selected_json() else {
            return false;
        };
        let Ok(mut shapes) = serde_json::from_str::<Vec<Shape>>(&json) else {
            return false;
        };
        for shape in &mut shapes {
            shape.regenerate_id();
            shape.transform(Affine::translate(offset));
        }
        self.add_shapes(shapes)
    }

    pub fn paste_image_at(
        &mut self,
        bytes: &[u8],
        width: u32,
        height: u32,
        format: ImageFormat,
        center_world: Point,
    ) -> bool {
        if bytes.is_empty() || width == 0 || height == 0 {
            return false;
        }
        let mut image = Image::new(Point::ZERO, bytes, width, height, format);
        if image.width > 800.0 || image.height > 800.0 {
            image = image.fit_within(800.0, 800.0);
        }
        image.position = Point::new(
            center_world.x - image.width / 2.0,
            center_world.y - image.height / 2.0,
        );
        self.add_shapes(vec![Shape::Image(image)])
    }

    fn place_shapes_centered_at(&mut self, mut shapes: Vec<Shape>, center_world: Point) -> bool {
        let Some(bounds) = shape_union(&shapes) else {
            return false;
        };
        let offset = center_world - bounds.center();
        for shape in &mut shapes {
            shape.transform(Affine::translate(offset));
        }
        self.add_shapes(shapes)
    }

    fn add_shapes(&mut self, shapes: Vec<Shape>) -> bool {
        if shapes.is_empty() {
            return false;
        }
        self.history.begin();
        self.capture_created_shapes(shapes.iter().map(Shape::id));
        self.canvas.clear_selection();
        for shape in shapes {
            let id = shape.id();
            self.canvas.document.add_shape(shape);
            self.canvas.add_to_selection(id);
        }
        self.mark_document_changed();
        true
    }
}

fn regenerate_shape_ids(mut shapes: Vec<Shape>) -> Vec<Shape> {
    for shape in &mut shapes {
        shape.regenerate_id();
    }
    shapes
}

fn shape_union(shapes: &[Shape]) -> Option<Rect> {
    shapes
        .iter()
        .map(Shape::bounds)
        .reduce(|bounds, next| bounds.union(next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use drafftink_core::shapes::Rectangle;

    #[test]
    fn paste_regenerates_ids_preserves_order_and_centers_as_one_transaction() {
        let mut source = DrafftBoard::new();
        source
            .canvas
            .document
            .add_shape(Shape::Rectangle(Rectangle::new(Point::ZERO, 20.0, 10.0)));
        source.canvas.select_all();
        let old_id = source.selected()[0];
        let json = source.copy_selected_json().unwrap();

        let mut target = DrafftBoard::new();
        assert!(target.paste_text_at(&json, Point::new(100.0, 50.0)));
        assert_eq!(target.selected().len(), 1);
        assert_ne!(target.selected()[0], old_id);
        let bounds = target
            .canvas
            .document
            .get_shape(target.selected()[0])
            .unwrap()
            .bounds();
        assert_eq!(bounds.center(), Point::new(100.0, 50.0));
        assert!(target.can_undo());
    }

    #[test]
    fn cut_is_one_undoable_document_change() {
        let mut board = DrafftBoard::new();
        board
            .canvas
            .document
            .add_shape(Shape::Rectangle(Rectangle::new(Point::ZERO, 20.0, 10.0)));
        board.canvas.select_all();
        assert!(board.cut_selected_json().is_some());
        assert!(board.canvas.document.is_empty());
        assert!(board.undo());
        assert_eq!(board.canvas.document.len(), 1);
    }
}
