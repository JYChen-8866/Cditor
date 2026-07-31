use super::*;

#[test]
fn rectangle_tool_commits_upstream_shape() {
    let mut board = DrafftBoard::new();
    board.set_tool(ToolKind::Rectangle);
    board.pointer_down(Point::new(10.0, 20.0), false, false);
    board.pointer_move(Point::new(110.0, 70.0), false);
    board.pointer_up(Point::new(110.0, 70.0), false);
    assert_eq!(board.canvas.document.len(), 1);
}

#[test]
fn pan_uses_upstream_camera() {
    let mut board = DrafftBoard::new();
    board.pointer_down(Point::new(10.0, 10.0), true, false);
    board.pointer_move(Point::new(25.0, 35.0), false);
    board.pointer_up(Point::new(25.0, 35.0), false);
    assert_eq!(board.canvas.camera.offset, Vec2::new(15.0, 25.0));
}

#[test]
fn selected_shape_moves_through_upstream_transform() {
    let mut board = DrafftBoard::new();
    board.canvas.camera.zoom = 1.0;
    board.set_tool(ToolKind::Rectangle);
    board.pointer_down(Point::new(10.0, 20.0), false, false);
    board.pointer_up(Point::new(110.0, 70.0), false);
    board.set_tool(ToolKind::Select);
    board.pointer_down(Point::new(20.0, 30.0), false, false);
    board.pointer_move(Point::new(40.0, 60.0), false);
    board.pointer_up(Point::new(40.0, 60.0), false);
    let bounds = board
        .canvas
        .document
        .shapes_ordered()
        .next()
        .unwrap()
        .bounds();
    assert_eq!(Point::new(bounds.x0, bounds.y0), Point::new(30.0, 50.0));
}

#[test]
fn selected_corner_uses_upstream_resize_manipulation() {
    let mut board = DrafftBoard::new();
    board.canvas.camera.zoom = 1.0;
    board.set_tool(ToolKind::Rectangle);
    board.pointer_down(Point::new(10.0, 20.0), false, false);
    board.pointer_up(Point::new(110.0, 70.0), false);
    board.set_tool(ToolKind::Select);
    board.pointer_down(Point::new(30.0, 30.0), false, false);
    board.pointer_up(Point::new(30.0, 30.0), false);
    board.pointer_down(Point::new(110.0, 70.0), false, false);
    board.pointer_move(Point::new(160.0, 100.0), false);
    board.pointer_up(Point::new(160.0, 100.0), false);
    let bounds = board
        .canvas
        .document
        .shapes_ordered()
        .next()
        .unwrap()
        .bounds();
    assert_eq!(bounds.width(), 150.0);
    assert_eq!(bounds.height(), 80.0);
}

#[test]
fn marquee_uses_upstream_shape_intersection_query() {
    let mut board = DrafftBoard::new();
    board.set_tool(ToolKind::Rectangle);
    board.pointer_down(Point::new(40.0, 40.0), false, false);
    board.pointer_up(Point::new(140.0, 100.0), false);
    board.set_tool(ToolKind::Select);
    board.pointer_down(Point::new(20.0, 20.0), false, false);
    board.pointer_move(Point::new(160.0, 120.0), false);
    board.pointer_up(Point::new(160.0, 120.0), false);
    assert_eq!(board.selected().len(), 1);
}

#[test]
fn reset_zoom_keeps_the_viewport_anchor_stable() {
    let mut board = DrafftBoard::new();
    board.canvas.camera.zoom = 3.0;
    board.canvas.camera.offset = Vec2::new(40.0, -20.0);
    let anchor = Point::new(400.0, 300.0);
    let world_before = board.canvas.camera.screen_to_world(anchor);
    board.zoom_reset_at(anchor);
    assert_eq!(board.zoom_percent(), 100);
    let world_after = board.canvas.camera.screen_to_world(anchor);
    assert!((world_before.x - world_after.x).abs() < 0.001);
    assert!((world_before.y - world_after.y).abs() < 0.001);
}

#[test]
fn new_linear_shapes_inherit_current_path_and_stroke_style() {
    let mut board = DrafftBoard::new();
    board.set_path_style(PathStyle::Flowing);
    board.set_stroke_style(StrokeStyle::Dashed);
    board.set_tool(ToolKind::Line);
    board.pointer_down(Point::new(10.0, 10.0), false, false);
    board.pointer_move(Point::new(100.0, 60.0), false);
    board.pointer_up(Point::new(100.0, 60.0), false);
    let Some(Shape::Line(line)) = board.canvas.document.shapes_ordered().next() else {
        panic!("line tool should commit a line");
    };
    assert_eq!(line.path_style, PathStyle::Flowing);
    assert_eq!(line.stroke_style, StrokeStyle::Dashed);
}

#[test]
fn layer_actions_preserve_selected_relative_order() {
    use crate::shapes::Rectangle;
    let mut board = DrafftBoard::new();
    let first = Shape::Rectangle(Rectangle::new(Point::new(0.0, 0.0), 20.0, 20.0));
    let second = Shape::Rectangle(Rectangle::new(Point::new(30.0, 0.0), 20.0, 20.0));
    let third = Shape::Rectangle(Rectangle::new(Point::new(60.0, 0.0), 20.0, 20.0));
    let first_id = first.id();
    let second_id = second.id();
    let third_id = third.id();
    board.canvas.document.add_shape(first);
    board.canvas.document.add_shape(second);
    board.canvas.document.add_shape(third);
    board.canvas.add_to_selection(second_id);
    board.canvas.add_to_selection(third_id);
    board.send_to_back();
    assert_eq!(
        board.canvas.document.z_order,
        vec![second_id, third_id, first_id]
    );
}

#[test]
fn alignment_uses_combined_selection_bounds() {
    use crate::shapes::Rectangle;
    let mut board = DrafftBoard::new();
    let first = Shape::Rectangle(Rectangle::new(Point::new(10.0, 20.0), 40.0, 30.0));
    let second = Shape::Rectangle(Rectangle::new(Point::new(80.0, 60.0), 20.0, 20.0));
    let first_id = first.id();
    let second_id = second.id();
    board.canvas.document.add_shape(first);
    board.canvas.document.add_shape(second);
    board.canvas.add_to_selection(first_id);
    board.canvas.add_to_selection(second_id);
    board.align_left();
    let first_x = board
        .canvas
        .document
        .get_shape(first_id)
        .unwrap()
        .bounds()
        .x0;
    let second_x = board
        .canvas
        .document
        .get_shape(second_id)
        .unwrap()
        .bounds()
        .x0;
    assert_eq!(first_x, 10.0);
    assert_eq!(second_x, 10.0);
}

#[test]
fn eraser_drag_removes_every_hit_shape_with_one_undo_snapshot() {
    use crate::shapes::Rectangle;

    let mut board = DrafftBoard::new();
    for x in [10.0, 50.0, 140.0] {
        board
            .canvas
            .document
            .add_shape(Shape::Rectangle(Rectangle::new(
                Point::new(x, 10.0),
                20.0,
                20.0,
            )));
    }
    board.set_tool(ToolKind::Eraser);
    board.pointer_down(Point::new(15.0, 20.0), false, false);
    board.pointer_move(Point::new(65.0, 20.0), false);
    board.pointer_up(Point::new(65.0, 20.0), false);

    assert_eq!(board.canvas.document.len(), 1);
    assert!(board.undo());
    assert_eq!(board.canvas.document.len(), 3);
    assert!(!board.undo());
}

#[test]
fn laser_trail_is_bounded_and_fades_without_touching_document() {
    let mut board = DrafftBoard::new();
    board.set_tool(ToolKind::LaserPointer);
    for x in 0..80 {
        board.pointer_hover(Point::new(x as f64, 20.0));
    }

    assert_eq!(board.laser_trail().len(), 50);
    assert_eq!(board.canvas.document.len(), 0);
    assert!(!board.fade_laser_trail(1.0));
    assert!(board.laser_trail().is_empty());
}

#[test]
fn text_tool_creates_editable_shape_and_preserves_utf8_boundaries() {
    let mut board = DrafftBoard::new();
    board.set_tool(ToolKind::Text);
    board.pointer_down(Point::new(40.0, 50.0), false, false);
    let PointerOutcome::BeginTextEdit(id) = board.pointer_up(Point::new(40.0, 50.0), false) else {
        panic!("text creation should enter edit mode");
    };
    assert!(board.replace_text_range(id, 0..0, "白板 text"));
    assert_eq!(board.text_content(id), Some("白板 text"));
    assert!(!board.replace_text_range(id, 1..2, "invalid"));
}

#[test]
fn visible_rotation_handle_has_priority_over_the_text_tool() {
    use crate::{selection::get_handles, shapes::Text};

    let mut board = DrafftBoard::new();
    let text = Shape::Text(Text::new(Point::new(40.0, 50.0), "中文".to_string()));
    let id = text.id();
    board.canvas.document.add_shape(text);
    board.canvas.add_to_selection(id);
    board.set_tool(ToolKind::Text);
    let handle = get_handles(board.canvas.document.get_shape(id).unwrap())[0].position;

    board.pointer_down(handle, false, false);
    board.pointer_move(Point::new(200.0, 60.0), false);
    board.pointer_up(Point::new(200.0, 60.0), false);

    assert!(
        board
            .canvas
            .document
            .get_shape(id)
            .unwrap()
            .rotation()
            .abs()
            > 0.1
    );
}

#[test]
fn rotated_text_hit_testing_uses_local_shape_coordinates() {
    use crate::shapes::Text;

    let mut text = Shape::Text(Text::new(Point::new(40.0, 50.0), "rotation".to_string()));
    text.set_rotation(std::f64::consts::FRAC_PI_2);
    let center = text.bounds().center();
    let local_point = Point::new(text.bounds().x0 + 2.0, center.y);
    let rotated_point = Affine::rotate_about(text.rotation(), center) * local_point;

    assert!(rotation_aware_hit_test(&text, rotated_point, 0.0));
}

#[test]
fn text_property_actions_update_all_selected_text_and_are_undoable() {
    use crate::shapes::{FontFamily, FontWeight, Text};

    let mut board = DrafftBoard::new();
    let first = Shape::Text(Text::new(Point::new(20.0, 30.0), "first".to_string()));
    let second = Shape::Text(Text::new(Point::new(80.0, 30.0), "second".to_string()));
    let first_id = first.id();
    let second_id = second.id();
    board.canvas.document.add_shape(first);
    board.canvas.document.add_shape(second);
    board.canvas.add_to_selection(first_id);
    board.canvas.add_to_selection(second_id);

    board.set_text_font_family(FontFamily::VanillaExtract);
    board.set_text_font_weight(FontWeight::Heavy);
    board.set_text_font_size(36.0);

    for id in [first_id, second_id] {
        let Some(Shape::Text(text)) = board.canvas.document.get_shape(id) else {
            panic!("selected shape should remain text");
        };
        assert_eq!(text.font_family, FontFamily::VanillaExtract);
        assert_eq!(text.font_weight, FontWeight::Heavy);
        assert_eq!(text.font_size, 36.0);
    }
    assert!(board.undo());
    assert!(board.undo());
    assert!(board.undo());
}

#[test]
fn math_font_size_action_invalidates_cached_layout() {
    use crate::shapes::Math;

    let mut board = DrafftBoard::new();
    let math = Math::new(Point::new(20.0, 30.0), "x^2".to_string());
    math.set_cached_size(100.0, 20.0, 5.0);
    let shape = Shape::Math(math);
    let id = shape.id();
    board.canvas.document.add_shape(shape);
    board.canvas.add_to_selection(id);

    board.set_math_font_size(28.0);

    let Some(Shape::Math(math)) = board.canvas.document.get_shape(id) else {
        panic!("selected shape should remain math");
    };
    assert_eq!(math.font_size, 28.0);
    assert!(math.cached_size().is_none());
}

#[test]
fn layer_extremes_use_document_order_not_selection_order() {
    use crate::shapes::Rectangle;

    let mut board = DrafftBoard::new();
    let shapes = [0.0, 30.0, 60.0, 90.0]
        .map(|x| Shape::Rectangle(Rectangle::new(Point::new(x, 0.0), 20.0, 20.0)));
    let ids = shapes.each_ref().map(Shape::id);
    for shape in shapes {
        board.canvas.document.add_shape(shape);
    }
    board.canvas.add_to_selection(ids[2]);
    board.canvas.add_to_selection(ids[0]);

    board.bring_to_front();
    assert_eq!(
        board.canvas.document.z_order,
        vec![ids[1], ids[3], ids[0], ids[2]]
    );
    board.send_to_back();
    assert_eq!(
        board.canvas.document.z_order,
        vec![ids[0], ids[2], ids[1], ids[3]]
    );
}

#[test]
fn horizontal_flip_preserves_single_rectangle_bounds() {
    use crate::shapes::Rectangle;

    let mut board = DrafftBoard::new();
    let shape = Shape::Rectangle(Rectangle::new(Point::new(10.0, 20.0), 80.0, 40.0));
    let id = shape.id();
    let before = shape.bounds();
    board.canvas.document.add_shape(shape);
    board.canvas.add_to_selection(id);

    board.flip_horizontal();

    assert_eq!(
        board.canvas.document.get_shape(id).unwrap().bounds(),
        before
    );
}

#[test]
fn alignment_matches_upstream_shape_bounds() {
    use crate::shapes::Rectangle;

    let mut board = DrafftBoard::new();
    let mut rotated = Rectangle::new(Point::new(10.0, 20.0), 80.0, 20.0);
    rotated.rotation = std::f64::consts::FRAC_PI_2;
    let rotated = Shape::Rectangle(rotated);
    let fixed = Shape::Rectangle(Rectangle::new(Point::new(120.0, 10.0), 30.0, 30.0));
    let rotated_id = rotated.id();
    let fixed_id = fixed.id();
    board.canvas.document.add_shape(rotated);
    board.canvas.document.add_shape(fixed);
    board.canvas.add_to_selection(rotated_id);
    board.canvas.add_to_selection(fixed_id);

    board.align_left();

    let left_a = board
        .canvas
        .document
        .get_shape(rotated_id)
        .unwrap()
        .bounds()
        .x0;
    let left_b = board
        .canvas
        .document
        .get_shape(fixed_id)
        .unwrap()
        .bounds()
        .x0;
    assert!((left_a - left_b).abs() < 0.001);
}

#[test]
fn group_and_ungroup_keep_selection_and_z_order_authoritative() {
    use crate::shapes::Rectangle;

    let mut board = DrafftBoard::new();
    for x in [10.0, 50.0] {
        let shape = Shape::Rectangle(Rectangle::new(Point::new(x, 20.0), 20.0, 20.0));
        let id = shape.id();
        board.canvas.document.add_shape(shape);
        board.canvas.add_to_selection(id);
    }

    assert!(board.group_selected());
    assert_eq!(board.selected().len(), 1);
    assert!(matches!(
        board.canvas.document.get_shape(board.selected()[0]),
        Some(Shape::Group(_))
    ));

    assert!(board.ungroup_selected());
    assert_eq!(board.selected().len(), 2);
    assert_eq!(board.canvas.document.len(), 2);
}

#[test]
fn grid_snap_applies_to_shape_creation_and_selected_move() {
    let mut board = DrafftBoard::new();
    board.canvas.camera.zoom = 1.0;
    board.toggle_grid_snap();
    board.set_tool(ToolKind::Rectangle);
    board.pointer_down(Point::new(13.0, 17.0), false, false);
    board.pointer_move(Point::new(87.0, 73.0), false);
    board.pointer_up(Point::new(87.0, 73.0), false);

    let id = board.canvas.document.shapes_ordered().next().unwrap().id();
    assert_eq!(
        board.canvas.document.get_shape(id).unwrap().bounds(),
        kurbo::Rect::new(20.0, 20.0, 80.0, 80.0)
    );

    board.set_tool(ToolKind::Select);
    board.pointer_down(Point::new(30.0, 30.0), false, false);
    board.pointer_move(Point::new(43.0, 51.0), false);
    board.pointer_up(Point::new(43.0, 51.0), false);
    assert_eq!(
        board.canvas.document.get_shape(id).unwrap().bounds().x0,
        40.0
    );
}

#[test]
fn angle_snap_constrains_new_lines_to_fifteen_degree_steps() {
    let mut board = DrafftBoard::new();
    board.canvas.camera.zoom = 1.0;
    board.toggle_angle_snap();
    board.set_tool(ToolKind::Line);
    board.pointer_down(Point::new(20.0, 20.0), false, false);
    board.pointer_move(Point::new(100.0, 47.0), false);
    board.pointer_up(Point::new(100.0, 47.0), false);

    let Some(Shape::Line(line)) = board.canvas.document.shapes_ordered().next() else {
        panic!("line should be created");
    };
    let angle = (line.end.y - line.start.y)
        .atan2(line.end.x - line.start.x)
        .to_degrees();
    assert!((angle / 15.0 - (angle / 15.0).round()).abs() < 0.001);
}
