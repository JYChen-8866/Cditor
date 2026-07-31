use crate::shapes::{
    FillPattern, FontFamily, FontWeight, PathStyle, SerializableColor, Shape, ShapeId, ShapeStyle,
    Sloppiness, StrokeStyle,
};
use kurbo::{Affine, Point, Rect, Vec2};

use super::DrafftBoard;

impl DrafftBoard {
    pub fn group_selected(&mut self) -> bool {
        if self.canvas.selection.len() < 2 {
            return false;
        }
        let shape_ids = self.canvas.selection.clone();
        self.begin_history_transaction();
        self.capture_history_order();
        let Some(group_id) = self.canvas.document.group_shapes(&shape_ids) else {
            self.history.cancel_pending();
            return false;
        };
        self.capture_created_shapes([group_id]);
        for id in shape_ids {
            self.canvas.widgets.remove(id);
        }
        self.canvas.selection.clear();
        self.canvas.selection.push(group_id);
        self.canvas.widgets.add_to_selection(group_id);
        self.mark_document_changed();
        true
    }

    pub fn ungroup_selected(&mut self) -> bool {
        let groups = self
            .canvas
            .selection
            .iter()
            .filter(|id| {
                self.canvas
                    .document
                    .get_shape(**id)
                    .is_some_and(Shape::is_group)
            })
            .copied()
            .collect::<Vec<_>>();
        if groups.is_empty() {
            return false;
        }
        self.begin_history_transaction();
        self.capture_history_order();
        for group_id in groups {
            self.canvas.widgets.remove(group_id);
            self.canvas.selection.retain(|id| *id != group_id);
            if let Some(children) = self.canvas.document.ungroup_shape(group_id) {
                self.capture_created_shapes(children.iter().copied());
                for child_id in children {
                    if !self.canvas.selection.contains(&child_id) {
                        self.canvas.selection.push(child_id);
                    }
                    self.canvas.widgets.add_to_selection(child_id);
                }
            }
        }
        self.mark_document_changed();
        true
    }

    pub fn set_text_font_family(&mut self, family: FontFamily) {
        let selected = self.selection_matching(|shape| matches!(shape, Shape::Text(_)));
        if selected.is_empty() {
            return;
        }
        self.begin_history_transaction();
        for id in selected {
            if let Some(Shape::Text(text)) = self.canvas.document.get_shape_mut(id) {
                text.font_family = family;
                text.invalidate_cache();
            }
        }
        self.mark_document_changed();
    }

    pub fn set_text_font_weight(&mut self, weight: FontWeight) {
        let selected = self.selection_matching(|shape| matches!(shape, Shape::Text(_)));
        if selected.is_empty() {
            return;
        }
        self.begin_history_transaction();
        for id in selected {
            if let Some(Shape::Text(text)) = self.canvas.document.get_shape_mut(id) {
                text.font_weight = weight;
                text.invalidate_cache();
            }
        }
        self.mark_document_changed();
    }

    pub fn set_text_font_size(&mut self, size: f64) {
        let selected = self.selection_matching(|shape| matches!(shape, Shape::Text(_)));
        if selected.is_empty() {
            return;
        }
        self.begin_history_transaction();
        for id in selected {
            if let Some(Shape::Text(text)) = self.canvas.document.get_shape_mut(id) {
                text.font_size = size.max(1.0);
                text.invalidate_cache();
            }
        }
        self.mark_document_changed();
    }

    pub fn set_math_font_size(&mut self, size: f64) {
        let selected = self.selection_matching(|shape| matches!(shape, Shape::Math(_)));
        if selected.is_empty() {
            return;
        }
        self.begin_history_transaction();
        for id in selected {
            if let Some(Shape::Math(math)) = self.canvas.document.get_shape_mut(id) {
                math.font_size = size.max(1.0);
                math.invalidate_cache();
            }
        }
        self.mark_document_changed();
    }

    pub fn set_stroke_color(&mut self, color: SerializableColor) {
        self.canvas.tool_manager.current_style.stroke_color = color;
        self.update_selected_styles(|style| style.stroke_color = color);
    }

    pub fn set_fill_color(&mut self, color: Option<SerializableColor>) {
        self.canvas.tool_manager.current_style.fill_color = color;
        self.update_selected_styles(|style| style.fill_color = color);
    }

    pub fn set_stroke_width(&mut self, width: f64) {
        self.canvas.tool_manager.current_style.stroke_width = width;
        self.update_selected_styles(|style| style.stroke_width = width);
    }

    pub fn set_sloppiness(&mut self, sloppiness: Sloppiness) {
        self.canvas.tool_manager.current_style.sloppiness = sloppiness;
        self.update_selected_styles(|style| style.sloppiness = sloppiness);
    }

    pub fn set_fill_pattern(&mut self, pattern: FillPattern) {
        self.canvas.tool_manager.current_style.fill_pattern = pattern;
        self.update_selected_styles(|style| style.fill_pattern = pattern);
    }

    pub fn path_style(&self) -> PathStyle {
        self.canvas
            .selection
            .iter()
            .find_map(|id| match self.canvas.document.get_shape(*id) {
                Some(Shape::Line(line)) => Some(line.path_style),
                Some(Shape::Arrow(arrow)) => Some(arrow.path_style),
                _ => None,
            })
            .unwrap_or(self.path_style)
    }

    pub fn set_path_style(&mut self, path_style: PathStyle) {
        self.path_style = path_style;
        let selected = self.linear_selection();
        if selected.is_empty() {
            return;
        }
        self.begin_history_transaction();
        for id in selected {
            match self.canvas.document.get_shape_mut(id) {
                Some(Shape::Line(line)) => {
                    line.path_style = path_style;
                    if path_style == PathStyle::Angular {
                        line.intermediate_points.clear();
                    }
                }
                Some(Shape::Arrow(arrow)) => {
                    arrow.path_style = path_style;
                    if path_style == PathStyle::Angular {
                        arrow.intermediate_points.clear();
                    }
                }
                _ => {}
            }
        }
        self.mark_document_changed();
    }

    pub fn stroke_style(&self) -> StrokeStyle {
        self.canvas
            .selection
            .iter()
            .find_map(|id| match self.canvas.document.get_shape(*id) {
                Some(Shape::Line(line)) => Some(line.stroke_style),
                Some(Shape::Arrow(arrow)) => Some(arrow.stroke_style),
                _ => None,
            })
            .unwrap_or(self.stroke_style)
    }

    pub fn set_stroke_style(&mut self, stroke_style: StrokeStyle) {
        self.stroke_style = stroke_style;
        let selected = self.linear_selection();
        if selected.is_empty() {
            return;
        }
        self.begin_history_transaction();
        for id in selected {
            match self.canvas.document.get_shape_mut(id) {
                Some(Shape::Line(line)) => line.stroke_style = stroke_style,
                Some(Shape::Arrow(arrow)) => arrow.stroke_style = stroke_style,
                _ => {}
            }
        }
        self.mark_document_changed();
    }

    pub fn calligraphy_mode(&self) -> bool {
        self.canvas.tool_manager.calligraphy_mode
    }

    pub fn toggle_calligraphy(&mut self) {
        self.canvas.tool_manager.calligraphy_mode = !self.canvas.tool_manager.calligraphy_mode;
    }

    pub fn pressure_simulation(&self) -> bool {
        self.canvas.tool_manager.pressure_simulation
    }

    pub fn toggle_pressure_simulation(&mut self) {
        self.canvas.tool_manager.pressure_simulation =
            !self.canvas.tool_manager.pressure_simulation;
    }

    pub fn set_opacity(&mut self, opacity: f64) {
        let opacity = opacity.clamp(0.0, 1.0);
        self.canvas.tool_manager.current_style.opacity = opacity;
        self.update_selected_styles(|style| style.opacity = opacity);
    }

    pub fn begin_opacity_edit(&mut self) {
        if !self.canvas.selection.is_empty() {
            self.begin_history_transaction();
        }
    }

    pub fn set_opacity_live(&mut self, opacity: f64) {
        let opacity = opacity.clamp(0.0, 1.0);
        self.canvas.tool_manager.current_style.opacity = opacity;
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                shape.style_mut().opacity = opacity;
            }
        }
        if !self.canvas.selection.is_empty() {
            self.bump_document_revision();
        }
    }

    pub fn commit_opacity_edit(&mut self) {
        self.history.commit(&self.canvas.document);
    }

    pub fn bring_to_front(&mut self) {
        if self.canvas.selection.is_empty() {
            return;
        }
        let (unselected, selected) = self.partition_z_order();
        let reordered = unselected.into_iter().chain(selected).collect::<Vec<_>>();
        if reordered == self.canvas.document.z_order {
            return;
        }
        self.begin_order_history_transaction();
        self.canvas.document.z_order = reordered;
        self.mark_document_changed();
    }

    pub fn send_to_back(&mut self) {
        if self.canvas.selection.is_empty() {
            return;
        }
        let (unselected, selected) = self.partition_z_order();
        let reordered = selected.into_iter().chain(unselected).collect::<Vec<_>>();
        if reordered == self.canvas.document.z_order {
            return;
        }
        self.begin_order_history_transaction();
        self.canvas.document.z_order = reordered;
        self.mark_document_changed();
    }

    pub fn bring_forward(&mut self) {
        let mut selected = self.canvas.selection.clone();
        if selected.is_empty() {
            return;
        }
        self.begin_order_history_transaction();
        selected.sort_by_key(|id| {
            self.canvas
                .document
                .z_order
                .iter()
                .position(|candidate| candidate == id)
                .unwrap_or(0)
        });
        for id in selected.into_iter().rev() {
            self.canvas.document.bring_forward(id);
        }
        self.mark_document_changed();
    }

    pub fn send_backward(&mut self) {
        let mut selected = self.canvas.selection.clone();
        if selected.is_empty() {
            return;
        }
        self.begin_order_history_transaction();
        selected.sort_by_key(|id| {
            self.canvas
                .document
                .z_order
                .iter()
                .position(|candidate| candidate == id)
                .unwrap_or(0)
        });
        for id in selected {
            self.canvas.document.send_backward(id);
        }
        self.mark_document_changed();
    }

    pub fn flip_horizontal(&mut self) {
        if self.canvas.selection.is_empty() {
            return;
        }
        self.begin_history_transaction();
        let bounds = self
            .selection_visual_bounds()
            .expect("selection is non-empty");
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                mirror_shape(shape, bounds.center().x, MirrorAxis::Horizontal);
            }
        }
        self.mark_document_changed();
    }

    pub fn flip_vertical(&mut self) {
        if self.canvas.selection.is_empty() {
            return;
        }
        self.begin_history_transaction();
        let bounds = self
            .selection_visual_bounds()
            .expect("selection is non-empty");
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                mirror_shape(shape, bounds.center().y, MirrorAxis::Vertical);
            }
        }
        self.mark_document_changed();
    }

    pub fn align_left(&mut self) {
        if self.canvas.selection.len() < 2 {
            return;
        }
        self.begin_history_transaction();
        let min_x = self
            .canvas
            .selection
            .iter()
            .filter_map(|&id| self.canvas.document.get_shape(id))
            .map(|shape| shape.bounds().x0)
            .fold(f64::INFINITY, f64::min);
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                let delta = min_x - shape.bounds().x0;
                shape.transform(Affine::translate(Vec2::new(delta, 0.0)));
            }
        }
        self.mark_document_changed();
    }

    pub fn align_center_vertical(&mut self) {
        if self.canvas.selection.len() < 2 {
            return;
        }
        self.begin_history_transaction();
        let combined = self
            .canvas
            .selection
            .iter()
            .filter_map(|&id| self.canvas.document.get_shape(id).map(Shape::bounds))
            .reduce(|bounds, shape_bounds| bounds.union(shape_bounds));
        let Some(center_x) = combined.map(|bounds| bounds.center().x) else {
            return;
        };
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                let delta = center_x - shape.bounds().center().x;
                shape.transform(Affine::translate(Vec2::new(delta, 0.0)));
            }
        }
        self.mark_document_changed();
    }

    pub fn align_right(&mut self) {
        if self.canvas.selection.len() < 2 {
            return;
        }
        self.begin_history_transaction();
        let max_x = self
            .canvas
            .selection
            .iter()
            .filter_map(|&id| self.canvas.document.get_shape(id))
            .map(|shape| shape.bounds().x1)
            .fold(f64::NEG_INFINITY, f64::max);
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                let delta = max_x - shape.bounds().x1;
                shape.transform(Affine::translate(Vec2::new(delta, 0.0)));
            }
        }
        self.mark_document_changed();
    }

    pub fn align_top(&mut self) {
        if self.canvas.selection.len() < 2 {
            return;
        }
        self.begin_history_transaction();
        let min_y = self
            .canvas
            .selection
            .iter()
            .filter_map(|&id| self.canvas.document.get_shape(id))
            .map(|shape| shape.bounds().y0)
            .fold(f64::INFINITY, f64::min);
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                let delta = min_y - shape.bounds().y0;
                shape.transform(Affine::translate(Vec2::new(0.0, delta)));
            }
        }
        self.mark_document_changed();
    }

    pub fn align_center_horizontal(&mut self) {
        if self.canvas.selection.len() < 2 {
            return;
        }
        self.begin_history_transaction();
        let combined = self
            .canvas
            .selection
            .iter()
            .filter_map(|&id| self.canvas.document.get_shape(id).map(Shape::bounds))
            .reduce(|bounds, shape_bounds| bounds.union(shape_bounds));
        let Some(center_y) = combined.map(|bounds| bounds.center().y) else {
            return;
        };
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                let delta = center_y - shape.bounds().center().y;
                shape.transform(Affine::translate(Vec2::new(0.0, delta)));
            }
        }
        self.mark_document_changed();
    }

    pub fn align_bottom(&mut self) {
        if self.canvas.selection.len() < 2 {
            return;
        }
        self.begin_history_transaction();
        let max_y = self
            .canvas
            .selection
            .iter()
            .filter_map(|&id| self.canvas.document.get_shape(id))
            .map(|shape| shape.bounds().y1)
            .fold(f64::NEG_INFINITY, f64::max);
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                let delta = max_y - shape.bounds().y1;
                shape.transform(Affine::translate(Vec2::new(0.0, delta)));
            }
        }
        self.mark_document_changed();
    }

    pub fn set_corner_radius(&mut self, radius: f64) {
        let radius = radius.max(0.0);
        self.canvas.tool_manager.corner_radius = radius;
        let rectangles = self
            .canvas
            .selection
            .iter()
            .filter(|id| {
                matches!(
                    self.canvas.document.get_shape(**id),
                    Some(Shape::Rectangle(_))
                )
            })
            .copied()
            .collect::<Vec<_>>();
        if rectangles.is_empty() {
            return;
        }
        self.begin_history_transaction();
        for id in rectangles {
            if let Some(Shape::Rectangle(rectangle)) = self.canvas.document.get_shape_mut(id) {
                rectangle.corner_radius = radius;
            }
        }
        self.mark_document_changed();
    }

    fn update_selected_styles(&mut self, mut update: impl FnMut(&mut ShapeStyle)) {
        if self.canvas.selection.is_empty() {
            return;
        }
        self.begin_history_transaction();
        let selected = self.canvas.selection.clone();
        for id in selected {
            if let Some(shape) = self.canvas.document.get_shape_mut(id) {
                update(shape.style_mut());
            }
        }
        self.mark_document_changed();
    }

    fn linear_selection(&self) -> Vec<ShapeId> {
        self.canvas
            .selection
            .iter()
            .copied()
            .filter(|id| {
                matches!(
                    self.canvas.document.get_shape(*id),
                    Some(Shape::Line(_) | Shape::Arrow(_))
                )
            })
            .collect()
    }

    fn selection_matching(&self, predicate: impl Fn(&Shape) -> bool) -> Vec<ShapeId> {
        self.canvas
            .selection
            .iter()
            .copied()
            .filter(|id| self.canvas.document.get_shape(*id).is_some_and(&predicate))
            .collect()
    }

    fn partition_z_order(&self) -> (Vec<ShapeId>, Vec<ShapeId>) {
        self.canvas
            .document
            .z_order
            .iter()
            .copied()
            .partition(|id| !self.canvas.selection.contains(id))
    }

    fn selection_visual_bounds(&self) -> Option<Rect> {
        self.canvas
            .selection
            .iter()
            .filter_map(|id| self.canvas.document.get_shape(*id).map(visual_bounds))
            .reduce(|combined, bounds| combined.union(bounds))
    }
}

#[derive(Clone, Copy)]
enum MirrorAxis {
    Horizontal,
    Vertical,
}

fn mirror_shape(shape: &mut Shape, axis: f64, mirror_axis: MirrorAxis) {
    match shape {
        Shape::Line(_) | Shape::Arrow(_) | Shape::Freehand(_) => {
            let reflection = match mirror_axis {
                MirrorAxis::Horizontal => {
                    Affine::translate((axis, 0.0))
                        * Affine::scale_non_uniform(-1.0, 1.0)
                        * Affine::translate((-axis, 0.0))
                }
                MirrorAxis::Vertical => {
                    Affine::translate((0.0, axis))
                        * Affine::scale_non_uniform(1.0, -1.0)
                        * Affine::translate((0.0, -axis))
                }
            };
            shape.transform(reflection);
        }
        Shape::Group(group) => {
            for child in group.children_mut() {
                mirror_shape(child, axis, mirror_axis);
            }
            group.rotation = -group.rotation;
        }
        _ => {
            let center = visual_bounds(shape).center();
            let reflected = match mirror_axis {
                MirrorAxis::Horizontal => Point::new(2.0 * axis - center.x, center.y),
                MirrorAxis::Vertical => Point::new(center.x, 2.0 * axis - center.y),
            };
            shape.transform(Affine::translate(reflected - center));
            shape.set_rotation(-shape.rotation());
        }
    }
}

fn visual_bounds(shape: &Shape) -> Rect {
    let bounds = shape.bounds();
    let rotation = shape.rotation();
    if rotation.abs() <= 0.001 || matches!(shape, Shape::Math(_)) {
        return bounds;
    }
    let transform = Affine::rotate_about(rotation, bounds.center());
    let corners = [
        Point::new(bounds.x0, bounds.y0),
        Point::new(bounds.x1, bounds.y0),
        Point::new(bounds.x1, bounds.y1),
        Point::new(bounds.x0, bounds.y1),
    ];
    corners
        .into_iter()
        .map(|point| transform * point)
        .fold(None::<Rect>, |combined, point| {
            let point_bounds = Rect::new(point.x, point.y, point.x, point.y);
            Some(match combined {
                Some(bounds) => bounds.union(point_bounds),
                None => point_bounds,
            })
        })
        .unwrap_or(bounds)
}
