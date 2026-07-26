use cditor_whiteboard_drafft::{
    core::{
        Canvas,
        shapes::{Freehand, Rectangle, Shape},
    },
    render::{RenderContext, Renderer, VelloRenderer},
};
use kurbo::{Point, Size, Vec2};

fn main() {
    let mut renderer = VelloRenderer::new();
    let mut canvas = Canvas::new();

    let rect = Rectangle::new(Point::new(100.0, 100.0), 200.0, 150.0);
    canvas.document.add_shape(Shape::Rectangle(rect));

    let points = (0..1_000)
        .map(|index| {
            let x = index as f64 * 0.75;
            Point::new(x, 320.0 + (x / 24.0).sin() * 48.0)
        })
        .collect();
    canvas
        .document
        .add_shape(Shape::Freehand(Freehand::from_points(points)));

    canvas.camera.pan(Vec2::new(32.0, 24.0));
    canvas.camera.zoom_at(Point::new(400.0, 300.0), 1.25);

    let ctx = RenderContext::new(&canvas, Size::new(800.0, 600.0)).with_scale_factor(2.0);
    renderer.build_scene(&ctx);

    assert!(!renderer.scene().encoding().is_empty());
}
