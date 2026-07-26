/// Paint one element at the current camera.
fn paint_element(
    kind: &ElementKind,
    mindmap_connector_style: Option<MindMapConnectorStyle>,
    cam: Camera,
    origin: Point<Pixels>,
    ink: Hsla,
    fill: Option<Hsla>,
    window: &mut Window,
) {
    match kind {
        ElementKind::Draw(s) => paint_stroke(&s.points, s.width, cam, origin, ink, window),
        ElementKind::Rect(b) => paint_rect(b, cam, origin, ink, fill, window),
        ElementKind::Ellipse(b) => paint_ellipse(b, cam, origin, ink, fill, window),
        ElementKind::Diamond(b) => {
            paint_box_polygon(b, &DIAMOND_UNIT, cam, origin, ink, fill, window)
        }
        ElementKind::Triangle(b) => {
            paint_box_polygon(b, &TRIANGLE_UNIT, cam, origin, ink, fill, window)
        }
        ElementKind::RoundRect(b) => paint_round_rect(b, cam, origin, ink, fill, window),
        ElementKind::Star(b) => paint_box_polygon(b, &star_unit(), cam, origin, ink, fill, window),
        ElementKind::Hexagon(b) => {
            paint_box_polygon(b, &hexagon_unit(), cam, origin, ink, fill, window)
        }
        ElementKind::Line(s) => paint_segment(
            s,
            false,
            mindmap_connector_style.unwrap_or(MindMapConnectorStyle::Straight),
            cam,
            origin,
            ink,
            window,
        ),
        ElementKind::Arrow(s) => paint_segment(
            s,
            true,
            mindmap_connector_style.unwrap_or(MindMapConnectorStyle::Straight),
            cam,
            origin,
            ink,
            window,
        ),
        // Text / cards / images are drawn as overlay elements in render(), not here.
        ElementKind::Text(_) | ElementKind::Embed(_) | ElementKind::Image(_) => {}
    }
}

fn paint_stroke(
    points: &[[f32; 2]],
    world_w: f32,
    cam: Camera,
    origin: Point<Pixels>,
    ink: Hsla,
    window: &mut Window,
) {
    if points.len() < 2 {
        return;
    }
    let z = cam.zoom.max(MIN_ZOOM);
    let mut pb = PathBuilder::stroke(px((world_w * z).max(0.5)));
    pb.move_to(to_screen(points[0][0], points[0][1], cam, origin));
    for p in &points[1..] {
        pb.line_to(to_screen(p[0], p[1], cam, origin));
    }
    if let Ok(path) = pb.build() {
        window.paint_path(path, ink);
    }
}

fn paint_rect(
    b: &BoxGeom,
    cam: Camera,
    origin: Point<Pixels>,
    ink: Hsla,
    fill: Option<Hsla>,
    window: &mut Window,
) {
    let z = cam.zoom.max(MIN_ZOOM);
    let c = box_padded_corners(b.x, b.y, b.w, b.h, b.rotation, 0.0);
    let trace = |pb: &mut PathBuilder| {
        pb.move_to(to_screen(c[0][0], c[0][1], cam, origin));
        pb.line_to(to_screen(c[1][0], c[1][1], cam, origin));
        pb.line_to(to_screen(c[2][0], c[2][1], cam, origin));
        pb.line_to(to_screen(c[3][0], c[3][1], cam, origin));
        pb.close();
    };
    if let Some(fill) = fill {
        let mut fb = PathBuilder::fill();
        trace(&mut fb);
        if let Ok(path) = fb.build() {
            window.paint_path(path, fill);
        }
    }
    let mut pb = PathBuilder::stroke(px((b.width * z).max(0.5)));
    trace(&mut pb);
    if let Ok(path) = pb.build() {
        window.paint_path(path, ink);
    }
}

fn paint_ellipse(
    b: &BoxGeom,
    cam: Camera,
    origin: Point<Pixels>,
    ink: Hsla,
    fill: Option<Hsla>,
    window: &mut Window,
) {
    let z = cam.zoom.max(MIN_ZOOM);
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let (rx, ry) = (b.w / 2.0, b.h / 2.0);
    const K: f32 = 0.552_284_8;
    let (kx, ky) = (rx * K, ry * K);
    // Every point is rotated about the box center before projection.
    let s = |wx: f32, wy: f32| {
        let (px_, py_) = rotate_pt(wx, wy, cx, cy, b.rotation);
        to_screen(px_, py_, cam, origin)
    };
    let trace = |pb: &mut PathBuilder| {
        pb.move_to(s(cx + rx, cy));
        pb.cubic_bezier_to(s(cx, cy + ry), s(cx + rx, cy + ky), s(cx + kx, cy + ry));
        pb.cubic_bezier_to(s(cx - rx, cy), s(cx - kx, cy + ry), s(cx - rx, cy + ky));
        pb.cubic_bezier_to(s(cx, cy - ry), s(cx - rx, cy - ky), s(cx - kx, cy - ry));
        pb.cubic_bezier_to(s(cx + rx, cy), s(cx + kx, cy - ry), s(cx + rx, cy - ky));
        pb.close();
    };
    if let Some(fill) = fill {
        let mut fb = PathBuilder::fill();
        trace(&mut fb);
        if let Ok(path) = fb.build() {
            window.paint_path(path, fill);
        }
    }
    let mut pb = PathBuilder::stroke(px((b.width * z).max(0.5)));
    trace(&mut pb);
    if let Ok(path) = pb.build() {
        window.paint_path(path, ink);
    }
}

/// Vertices of box-fitting polygons in box-relative coords: `(±1, ±1)` is the
/// box edge, `(0, 0)` the center. Scaled to the half-extents, rotated about the
/// center, and projected by [`paint_box_polygon`].
const DIAMOND_UNIT: [(f32, f32); 4] = [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)];
const TRIANGLE_UNIT: [(f32, f32); 3] = [(0.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

/// A 5-point star (outer radius 1, inner 0.382), point-up.
fn star_unit() -> [(f32, f32); 10] {
    use std::f32::consts::{FRAC_PI_2, PI};
    const INNER: f32 = 0.382;
    let mut pts = [(0.0, 0.0); 10];
    for (k, p) in pts.iter_mut().enumerate() {
        let a = -FRAC_PI_2 + k as f32 * (PI / 5.0);
        let r = if k % 2 == 0 { 1.0 } else { INNER };
        *p = (a.cos() * r, a.sin() * r);
    }
    pts
}

/// A pointy-top hexagon inscribed in the box's ellipse.
fn hexagon_unit() -> [(f32, f32); 6] {
    use std::f32::consts::{FRAC_PI_2, PI};
    let mut pts = [(0.0, 0.0); 6];
    for (k, p) in pts.iter_mut().enumerate() {
        let a = -FRAC_PI_2 + k as f32 * (PI / 3.0);
        *p = (a.cos(), a.sin());
    }
    pts
}

/// Stroke (and optionally fill) a closed polygon whose `unit` vertices are given
/// in box-relative coords (see [`DIAMOND_UNIT`]). Mirrors [`paint_rect`]: every
/// vertex is scaled to the half-extents, rotated about the box center, and
/// projected to screen.
fn paint_box_polygon(
    b: &BoxGeom,
    unit: &[(f32, f32)],
    cam: Camera,
    origin: Point<Pixels>,
    ink: Hsla,
    fill: Option<Hsla>,
    window: &mut Window,
) {
    let z = cam.zoom.max(MIN_ZOOM);
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let (rx, ry) = (b.w / 2.0, b.h / 2.0);
    let s = |u: &(f32, f32)| {
        let (wx, wy) = rotate_pt(cx + u.0 * rx, cy + u.1 * ry, cx, cy, b.rotation);
        to_screen(wx, wy, cam, origin)
    };
    let trace = |pb: &mut PathBuilder| {
        let mut it = unit.iter();
        if let Some(first) = it.next() {
            pb.move_to(s(first));
            for u in it {
                pb.line_to(s(u));
            }
            pb.close();
        }
    };
    if let Some(fill) = fill {
        let mut fb = PathBuilder::fill();
        trace(&mut fb);
        if let Ok(path) = fb.build() {
            window.paint_path(path, fill);
        }
    }
    let mut pb = PathBuilder::stroke(px((b.width * z).max(0.5)));
    trace(&mut pb);
    if let Ok(path) = pb.build() {
        window.paint_path(path, ink);
    }
}

/// A rounded rectangle: straight edges joined by quarter-circle corners (radius
/// = 20% of the shorter side), rotated about the center like [`paint_rect`].
fn paint_round_rect(
    b: &BoxGeom,
    cam: Camera,
    origin: Point<Pixels>,
    ink: Hsla,
    fill: Option<Hsla>,
    window: &mut Window,
) {
    let z = cam.zoom.max(MIN_ZOOM);
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let r = b.w.abs().min(b.h.abs()) * 0.2;
    let k = r * 0.552_284_8; // cubic control offset for a quarter circle
    let s = |wx: f32, wy: f32| {
        let (px_, py_) = rotate_pt(wx, wy, cx, cy, b.rotation);
        to_screen(px_, py_, cam, origin)
    };
    let (x0, y0, x1, y1) = (b.x, b.y, b.x + b.w, b.y + b.h);
    let trace = |pb: &mut PathBuilder| {
        // Clockwise from just past the top-left corner.
        pb.move_to(s(x0 + r, y0));
        pb.line_to(s(x1 - r, y0));
        pb.cubic_bezier_to(s(x1, y0 + r), s(x1 - r + k, y0), s(x1, y0 + r - k));
        pb.line_to(s(x1, y1 - r));
        pb.cubic_bezier_to(s(x1 - r, y1), s(x1, y1 - r + k), s(x1 - r + k, y1));
        pb.line_to(s(x0 + r, y1));
        pb.cubic_bezier_to(s(x0, y1 - r), s(x0 + r - k, y1), s(x0, y1 - r + k));
        pb.line_to(s(x0, y0 + r));
        pb.cubic_bezier_to(s(x0 + r, y0), s(x0, y0 + r - k), s(x0 + r - k, y0));
        pb.close();
    };
    if let Some(fill) = fill {
        let mut fb = PathBuilder::fill();
        trace(&mut fb);
        if let Ok(path) = fb.build() {
            window.paint_path(path, fill);
        }
    }
    let mut pb = PathBuilder::stroke(px((b.width * z).max(0.5)));
    trace(&mut pb);
    if let Ok(path) = pb.build() {
        window.paint_path(path, ink);
    }
}

fn paint_segment(
    seg: &SegGeom,
    arrow: bool,
    style: MindMapConnectorStyle,
    cam: Camera,
    origin: Point<Pixels>,
    ink: Hsla,
    window: &mut Window,
) {
    let z = cam.zoom.max(MIN_ZOOM);
    let p1 = to_screen(seg.x1, seg.y1, cam, origin);
    let p2 = to_screen(seg.x2, seg.y2, cam, origin);
    let p1f = [f32::from(p1.x), f32::from(p1.y)];
    let p2f = [f32::from(p2.x), f32::from(p2.y)];
    let stroke_px = (seg.width * z).max(0.5);
    let mut points = vec![p1f];
    let (dxw, _dyw) = (seg.x2 - seg.x1, seg.y2 - seg.y1);
    let (end_dx, end_dy) = match style {
        MindMapConnectorStyle::Straight => {
            points.push(p2f);
            (p2f[0] - p1f[0], p2f[1] - p1f[1])
        }
        MindMapConnectorStyle::Bezier => {
            let cx1 = seg.x1 + dxw * 0.35;
            let cy1 = seg.y1;
            let cx2 = seg.x2 - dxw * 0.35;
            let cy2 = seg.y2;
            let c1 = to_screen(cx1, cy1, cam, origin);
            let c2 = to_screen(cx2, cy2, cam, origin);
            let c1f = [f32::from(c1.x), f32::from(c1.y)];
            let c2f = [f32::from(c2.x), f32::from(c2.y)];
            for i in 1..=24 {
                let t = i as f32 / 24.0;
                points.push(cubic_point(p1f, c1f, c2f, p2f, t));
            }
            (3.0 * (p2f[0] - c2f[0]), 3.0 * (p2f[1] - c2f[1]))
        }
        MindMapConnectorStyle::Orthogonal => {
            let mid_x = seg.x1 + dxw * 0.5;
            let m1 = to_screen(mid_x, seg.y1, cam, origin);
            let m2 = to_screen(mid_x, seg.y2, cam, origin);
            let m1f = [f32::from(m1.x), f32::from(m1.y)];
            let m2f = [f32::from(m2.x), f32::from(m2.y)];
            points.push(m1f);
            points.push(m2f);
            points.push(p2f);
            (p2f[0] - m2f[0], p2f[1] - m2f[1])
        }
    };
    paint_polyline(points.as_slice(), stroke_px, seg.style, ink, window);
    if !arrow {
        return;
    }
    let (dx, dy) = (end_dx, end_dy);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let head = (seg.width * z * 6.0).max(8.0);
    let (bx, by) = (f32::from(p2.x), f32::from(p2.y));
    let barb = |a: f32| {
        let (c, s) = (a.cos(), a.sin());
        let rx = (-ux) * c - (-uy) * s;
        let ry = (-ux) * s + (-uy) * c;
        point(px(bx + head * rx), px(by + head * ry))
    };
    let mut hb = PathBuilder::fill();
    hb.move_to(p2);
    hb.line_to(barb(0.45));
    hb.line_to(barb(-0.45));
    hb.close();
    if let Ok(path) = hb.build() {
        window.paint_path(path, ink);
    }
}

fn cubic_point(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let mt = 1.0 - t;
    let a = mt * mt * mt;
    let b = 3.0 * mt * mt * t;
    let c = 3.0 * mt * t * t;
    let d = t * t * t;
    [
        a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
        a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
    ]
}

fn paint_polyline(
    points: &[[f32; 2]],
    stroke_px: f32,
    style: SegmentStyle,
    ink: Hsla,
    window: &mut Window,
) {
    if points.len() < 2 {
        return;
    }
    let mut pb = PathBuilder::stroke(px(stroke_px));
    match style {
        SegmentStyle::Solid => {
            pb.move_to(point(px(points[0][0]), px(points[0][1])));
            for p in &points[1..] {
                pb.line_to(point(px(p[0]), px(p[1])));
            }
        }
        SegmentStyle::Dashed => {
            let dash = (stroke_px * 4.5).max(10.0);
            let gap = (stroke_px * 2.5).max(6.0);
            let cycle = dash + gap;
            let mut traveled = 0.0;
            for seg in points.windows(2) {
                let a = seg[0];
                let b = seg[1];
                let dx = b[0] - a[0];
                let dy = b[1] - a[1];
                let len = (dx * dx + dy * dy).sqrt();
                if len <= 0.01 {
                    continue;
                }
                let ux = dx / len;
                let uy = dy / len;
                let mut local = 0.0;
                while local < len {
                    let at = traveled + local;
                    let phase = at % cycle;
                    let draw = if phase < dash { dash - phase } else { 0.0 };
                    if draw > 0.0 {
                        let s = local;
                        let e = (local + draw).min(len);
                        let p0 = [a[0] + ux * s, a[1] + uy * s];
                        let p1 = [a[0] + ux * e, a[1] + uy * e];
                        pb.move_to(point(px(p0[0]), px(p0[1])));
                        pb.line_to(point(px(p1[0]), px(p1[1])));
                        local = e;
                    } else {
                        local = (local + (cycle - phase)).min(len);
                    }
                }
                traveled += len;
            }
        }
    }
    if let Ok(path) = pb.build() {
        window.paint_path(path, ink);
    }
}

fn draw_filled_circle(hx: f32, hy: f32, radius: f32, color: Hsla, window: &mut Window) {
    const K: f32 = 0.552_284_8;
    let k = radius * K;
    let p = |x: f32, y: f32| point(px(x), px(y));
    let mut path = PathBuilder::fill();
    path.move_to(p(hx + radius, hy));
    path.cubic_bezier_to(
        p(hx, hy + radius),
        p(hx + radius, hy + k),
        p(hx + k, hy + radius),
    );
    path.cubic_bezier_to(
        p(hx - radius, hy),
        p(hx - k, hy + radius),
        p(hx - radius, hy + k),
    );
    path.cubic_bezier_to(
        p(hx, hy - radius),
        p(hx - radius, hy - k),
        p(hx - k, hy - radius),
    );
    path.cubic_bezier_to(
        p(hx + radius, hy),
        p(hx + k, hy - radius),
        p(hx + radius, hy - k),
    );
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// Compact circular resize handle matching the whiteboard connector controls.
fn draw_handle(hx: f32, hy: f32, color: Hsla, window: &mut Window) {
    draw_filled_circle(hx, hy, HANDLE_HALF + 1.0, hsla(0.0, 0.0, 1.0, 1.0), window);
    draw_filled_circle(hx, hy, HANDLE_HALF, color, window);
}

/// Screen-space centers for the top/right/bottom/left connector buttons. Each
/// button is pushed outward from the selected element while its line still
/// starts at the true edge connector point.
fn connector_button_centers(
    kind: &ElementKind,
    cam: Camera,
    origin: Point<Pixels>,
) -> [Point<Pixels>; 4] {
    let points = connector_points(kind);
    let edges = [
        to_screen(points[0][0], points[0][1], cam, origin),
        to_screen(points[1][0], points[1][1], cam, origin),
        to_screen(points[2][0], points[2][1], cam, origin),
        to_screen(points[3][0], points[3][1], cam, origin),
    ];
    let center = edges.iter().fold((0.0, 0.0), |(x, y), point| {
        (x + f32::from(point.x), y + f32::from(point.y))
    });
    let center = (center.0 / 4.0, center.1 / 4.0);
    edges.map(|edge| {
        let dx = f32::from(edge.x) - center.0;
        let dy = f32::from(edge.y) - center.1;
        let length = (dx * dx + dy * dy).sqrt().max(1.0);
        point(
            edge.x + px(dx / length * CONNECTOR_BUTTON_GAP),
            edge.y + px(dy / length * CONNECTOR_BUTTON_GAP),
        )
    })
}

fn paint_snap_points(
    kind: &ElementKind,
    active: usize,
    cam: Camera,
    origin: Point<Pixels>,
    color: Hsla,
    window: &mut Window,
) {
    for (index, point) in connector_points(kind).into_iter().enumerate() {
        let screen = to_screen(point[0], point[1], cam, origin);
        let radius = if index == active {
            HANDLE_HALF + 1.5
        } else {
            HANDLE_HALF
        };
        let (x, y) = (f32::from(screen.x), f32::from(screen.y));
        draw_filled_circle(x, y, radius + 1.0, hsla(0.0, 0.0, 1.0, 1.0), window);
        draw_filled_circle(x, y, radius, color, window);
    }
}

fn paint_selection(
    kind: &ElementKind,
    cam: Camera,
    origin: Point<Pixels>,
    color: Hsla,
    window: &mut Window,
) {
    // Lines/arrows: a handle at each endpoint (no box — its bbox is degenerate)
    // plus a rotate grip above.
    if let ElementKind::Line(s) | ElementKind::Arrow(s) = kind {
        for (wx, wy) in [(s.x1, s.y1), (s.x2, s.y2)] {
            let p = to_screen(wx, wy, cam, origin);
            draw_handle(f32::from(p.x), f32::from(p.y), color, window);
        }
        return;
    }
    // Box-like (rect/ellipse/text): the (possibly rotated) box outline, four
    // corner handles, and a rotate grip. Edge-midpoint handles (per-axis stretch)
    // show only when upright — a rotated box's edges aren't world-axis-aligned.
    if let Some((x, y, w, h, rot)) = box_like(kind) {
        let s =
            box_padded_corners(x, y, w, h, rot, 0.0).map(|p| to_screen(p[0], p[1], cam, origin));
        for p in &s {
            draw_handle(f32::from(p.x), f32::from(p.y), color, window);
        }
        if rot.abs() <= ROT_EPS && !matches!(kind, ElementKind::Text(_)) {
            let mid = |a: Point<Pixels>, b: Point<Pixels>| {
                (
                    (f32::from(a.x) + f32::from(b.x)) / 2.0,
                    (f32::from(a.y) + f32::from(b.y)) / 2.0,
                )
            };
            for (hx, hy) in [
                mid(s[0], s[1]),
                mid(s[1], s[2]),
                mid(s[2], s[3]),
                mid(s[3], s[0]),
            ] {
                draw_handle(hx, hy, color, window);
            }
        }
        return;
    }
    // Draw / Embed: a padded AABB box + four corner handles. Freehand strokes
    // (rotatable) also get a rotate grip; cards don't.
    let bb = bbox(kind);
    let tl = to_screen(bb.0, bb.1, cam, origin);
    let br = to_screen(bb.2, bb.3, cam, origin);
    let m = 0.0;
    let (x0, y0) = (f32::from(tl.x) - m, f32::from(tl.y) - m);
    let (x1, y1) = (f32::from(br.x) + m, f32::from(br.y) + m);
    let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    for (hx, hy) in [
        (x0, y0),
        (x1, y0),
        (x0, y1),
        (x1, y1),
        (mx, y0),
        (x1, my),
        (mx, y1),
        (x0, my),
    ] {
        draw_handle(hx, hy, color, window);
    }
}

/// The in-progress marquee box: a faint fill + thin outline.
fn paint_marquee(
    a: [f32; 2],
    b: [f32; 2],
    cam: Camera,
    origin: Point<Pixels>,
    color: Hsla,
    window: &mut Window,
) {
    let pa = to_screen(a[0], a[1], cam, origin);
    let pb = to_screen(b[0], b[1], cam, origin);
    let (x0, x1) = (
        f32::from(pa.x).min(f32::from(pb.x)),
        f32::from(pa.x).max(f32::from(pb.x)),
    );
    let (y0, y1) = (
        f32::from(pa.y).min(f32::from(pb.y)),
        f32::from(pa.y).max(f32::from(pb.y)),
    );
    let bounds = Bounds {
        origin: point(px(x0), px(y0)),
        size: size(px(x1 - x0), px(y1 - y0)),
    };
    let mut faint = color;
    faint.a *= 0.12;
    window.paint_quad(fill(bounds, faint));
    let mut pbld = PathBuilder::stroke(px(1.0));
    pbld.move_to(point(px(x0), px(y0)));
    pbld.line_to(point(px(x1), px(y0)));
    pbld.line_to(point(px(x1), px(y1)));
    pbld.line_to(point(px(x0), px(y1)));
    pbld.close();
    if let Ok(p) = pbld.build() {
        window.paint_path(p, color);
    }
}

fn paint_alignment_guides(
    guides: AlignmentGuides,
    bounds: Bounds<Pixels>,
    cam: Camera,
    color: Hsla,
    window: &mut Window,
) {
    let mut color = color;
    color.a = 0.8;
    if let Some(x) = guides.vertical {
        let screen = to_screen(x, 0.0, cam, bounds.origin);
        let mut path = PathBuilder::stroke(px(1.0));
        path.move_to(point(screen.x, bounds.top()));
        path.line_to(point(screen.x, bounds.bottom()));
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
    if let Some(y) = guides.horizontal {
        let screen = to_screen(0.0, y, cam, bounds.origin);
        let mut path = PathBuilder::stroke(px(1.0));
        path.move_to(point(bounds.left(), screen.y));
        path.line_to(point(bounds.right(), screen.y));
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}
