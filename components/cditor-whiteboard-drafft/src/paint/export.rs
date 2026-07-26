use drafftink_core::{Canvas, shapes::ShapeId};
use image::imageops::FilterType;
use kurbo::{PathEl, Rect, Size, Vec2};
use tiny_skia::{
    Color, FillRule, IntSize, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, StrokeDash,
    Transform,
};

use super::{
    image::ImagePaintEngine,
    plan::{GridStyle, PaintKind, PaintPlan},
    text_outline::TextOutlineEngine,
};

pub(crate) fn export_png(
    canvas: &Canvas,
    selection: &[ShapeId],
    scale: u8,
    text_engine: &mut TextOutlineEngine,
    image_engine: &mut ImagePaintEngine,
) -> Result<Vec<u8>, String> {
    let mut export_canvas = canvas.clone();
    if !selection.is_empty() {
        export_canvas
            .document
            .shapes
            .retain(|id, _| selection.contains(id));
        export_canvas
            .document
            .z_order
            .retain(|id| selection.contains(id));
    }
    export_canvas.clear_selection();
    let bounds = export_canvas
        .document
        .shapes_ordered()
        .map(|shape| shape.bounds())
        .reduce(|bounds, next| bounds.union(next))
        .ok_or_else(|| "Nothing to export".to_string())?
        .inflate(20.0, 20.0);
    let scale = f64::from(scale.clamp(1, 3));
    let width = (bounds.width() * scale).ceil().max(1.0) as u32;
    let height = (bounds.height() * scale).ceil().max(1.0) as u32;
    if u64::from(width) * u64::from(height) > 100_000_000 {
        return Err("Export is larger than 100 megapixels".into());
    }
    export_canvas.camera.zoom = scale;
    export_canvas.camera.offset = Vec2::new(-bounds.x0 * scale, -bounds.y0 * scale);
    let plan = PaintPlan::build_with_selection(
        &export_canvas,
        Size::new(f64::from(width), f64::from(height)),
        None,
        GridStyle::None,
        text_engine,
        image_engine,
    );
    rasterize(&plan, width, height)
}

fn rasterize(plan: &PaintPlan, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut pixmap = Pixmap::new(width, height).ok_or_else(|| "Invalid export size".to_string())?;
    pixmap.fill(Color::WHITE);
    for command in &plan.commands {
        match &command.kind {
            PaintKind::Fill => {
                let Some(path) = skia_path(&command.path) else {
                    continue;
                };
                pixmap.fill_path(
                    &path,
                    &paint(command.color),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
            PaintKind::Stroke { width, dash } => {
                let Some(path) = skia_path(&command.path) else {
                    continue;
                };
                let mut stroke = Stroke {
                    width: *width,
                    ..Stroke::default()
                };
                stroke.dash = dash
                    .and_then(|[on, off]| StrokeDash::new(vec![on.max(0.1), off.max(0.1)], 0.0));
                pixmap.stroke_path(
                    &path,
                    &paint(command.color),
                    &stroke,
                    Transform::identity(),
                    None,
                );
            }
            PaintKind::Image(image) => draw_image(&mut pixmap, image)?,
            PaintKind::Text(_) => {}
        }
    }
    pixmap.encode_png().map_err(|error| error.to_string())
}

fn skia_path(path: &kurbo::BezPath) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) => builder.move_to(point.x as f32, point.y as f32),
            PathEl::LineTo(point) => builder.line_to(point.x as f32, point.y as f32),
            PathEl::QuadTo(control, point) => builder.quad_to(
                control.x as f32,
                control.y as f32,
                point.x as f32,
                point.y as f32,
            ),
            PathEl::CurveTo(a, b, point) => builder.cubic_to(
                a.x as f32,
                a.y as f32,
                b.x as f32,
                b.y as f32,
                point.x as f32,
                point.y as f32,
            ),
            PathEl::ClosePath => builder.close(),
        }
    }
    builder.finish()
}

fn paint(rgba: u32) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        (rgba >> 24) as u8,
        (rgba >> 16) as u8,
        (rgba >> 8) as u8,
        rgba as u8,
    );
    paint.anti_alias = true;
    paint
}

fn draw_image(pixmap: &mut Pixmap, image: &super::plan::ImagePaint) -> Result<(), String> {
    let bounds = normalized(image.bounds);
    let width = bounds.width().round().max(1.0) as u32;
    let height = bounds.height().round().max(1.0) as u32;
    let decoded = image::load_from_memory(image.image.bytes())
        .map_err(|error| error.to_string())?
        .to_rgba8();
    let resized = image::imageops::resize(&decoded, width, height, FilterType::Lanczos3);
    let mut premultiplied = resized.into_raw();
    for pixel in premultiplied.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        pixel[0] = (u16::from(pixel[0]) * alpha / 255) as u8;
        pixel[1] = (u16::from(pixel[1]) * alpha / 255) as u8;
        pixel[2] = (u16::from(pixel[2]) * alpha / 255) as u8;
    }
    let size = IntSize::from_wh(width, height).ok_or_else(|| "Invalid image size".to_string())?;
    let source = Pixmap::from_vec(premultiplied, size)
        .ok_or_else(|| "Could not prepare image".to_string())?;
    pixmap.draw_pixmap(
        bounds.x0.round() as i32,
        bounds.y0.round() as i32,
        source.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Ok(())
}

fn normalized(rect: Rect) -> Rect {
    Rect::new(
        rect.x0.min(rect.x1),
        rect.y0.min(rect.y1),
        rect.x0.max(rect.x1),
        rect.y0.max(rect.y1),
    )
}
