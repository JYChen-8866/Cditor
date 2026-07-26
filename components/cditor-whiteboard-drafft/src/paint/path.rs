use gpui::{App, Bounds, Corners, Hsla, PathBuilder, Pixels, Point, Window, point, px, rgba, size};
use kurbo::{PathEl, Point as KurboPoint, Rect, Shape};

use super::plan::{PaintCommand, PaintKind, PaintPlan};

pub(crate) fn paint_plan(
    plan: &PaintPlan,
    origin: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    for command in &plan.commands {
        paint_command(command, origin, window, cx);
    }
}

fn paint_command(command: &PaintCommand, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
    if let PaintKind::Image(image) = &command.kind {
        paint_image(image, origin, window, cx);
        return;
    }
    if let PaintKind::Text(text) = &command.kind {
        paint_text_editing(text, origin, window);
        return;
    }
    let mut builder = match &command.kind {
        PaintKind::Fill => PathBuilder::fill(),
        PaintKind::Stroke { width, dash } => {
            let builder = PathBuilder::stroke(px(*width));
            match dash {
                Some([on, off]) => builder.dash_array(&[px(*on), px(*off)]),
                None => builder,
            }
        }
        PaintKind::Text(_) => unreachable!(),
        PaintKind::Image(_) => unreachable!(),
    };

    for element in command.path.elements() {
        match *element {
            PathEl::MoveTo(to) => builder.move_to(gpui_point(to, origin)),
            PathEl::LineTo(to) => builder.line_to(gpui_point(to, origin)),
            PathEl::QuadTo(control, to) => {
                builder.curve_to(gpui_point(to, origin), gpui_point(control, origin));
            }
            PathEl::CurveTo(control_a, control_b, to) => builder.cubic_bezier_to(
                gpui_point(to, origin),
                gpui_point(control_a, origin),
                gpui_point(control_b, origin),
            ),
            PathEl::ClosePath => builder.close(),
        }
    }

    if let Ok(path) = builder.build() {
        window.paint_path(path, color(command.color));
    }
}

fn paint_image(
    image: &super::plan::ImagePaint,
    canvas_origin: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(render_image) = image.image.clone().use_render_image(window, cx) else {
        return;
    };
    if render_image.frame_count() == 0 {
        return;
    }
    let bounds = Bounds::new(
        point(
            canvas_origin.x + px(image.bounds.x0 as f32),
            canvas_origin.y + px(image.bounds.y0 as f32),
        ),
        size(
            px(image.bounds.width().max(0.0) as f32),
            px(image.bounds.height().max(0.0) as f32),
        ),
    );
    let _ = window.paint_image(bounds, Corners::default(), render_image, 0, false);
}

fn paint_text_editing(
    text: &super::plan::TextPaint,
    canvas_origin: Point<Pixels>,
    window: &mut Window,
) {
    let Some(editing) = &text.editing else {
        return;
    };
    if editing.caret != editing.anchor {
        for rect in text.geometry.selection_rects(editing.anchor, editing.caret) {
            paint_transformed_rect(rect, text.transform, canvas_origin, 0x3b82f633, window);
        }
    }
    if let Some(marked) = &editing.marked_range {
        for rect in text.geometry.selection_rects(marked.start, marked.end) {
            let underline = Rect::new(rect.x0, rect.y1 - 1.0, rect.x1, rect.y1);
            paint_transformed_rect(underline, text.transform, canvas_origin, 0x3c3c3cff, window);
        }
    }
    if editing.caret_visible && editing.caret == editing.anchor {
        paint_transformed_rect(
            text.geometry.caret_rect(editing.caret, 2.0),
            text.transform,
            canvas_origin,
            0x2563ebff,
            window,
        );
    }
}

fn paint_transformed_rect(
    rect: Rect,
    transform: kurbo::Affine,
    origin: Point<Pixels>,
    color_value: u32,
    window: &mut Window,
) {
    let mut builder = PathBuilder::fill();
    for element in (transform * rect.to_path(0.1)).elements() {
        match *element {
            PathEl::MoveTo(to) => builder.move_to(gpui_point(to, origin)),
            PathEl::LineTo(to) => builder.line_to(gpui_point(to, origin)),
            PathEl::ClosePath => builder.close(),
            _ => unreachable!("rectangle paths contain only straight segments"),
        }
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color(color_value));
    }
}

fn gpui_point(value: KurboPoint, origin: Point<Pixels>) -> Point<Pixels> {
    point(origin.x + px(value.x as f32), origin.y + px(value.y as f32))
}

fn color(value: u32) -> Hsla {
    rgba(value).into()
}
