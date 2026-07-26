/// World point → text-local space (origin at the block's top-left), undoing the
/// text's rotation about its center so a click maps to the right glyph.
/// The text currently being edited, captured for the caret math + click
/// hit-testing. See [`WhiteboardView::edit_target`].
struct EditTarget {
    content: String,
    size: f32,
    wrap: Option<f32>,
    x: f32,
    y: f32,
    rotation: f32,
    /// Rotation pivot (world) for click → local mapping — the shape's center.
    pivot: [f32; 2],
}

/// A closed shape's label block: world top-left `(x, y)`, the auto-shrunk font
/// size, and the wrap width.
struct LabelBlock {
    x: f32,
    y: f32,
    size: f32,
    wrap: f32,
}

/// Map a model [`RunStyle`] to the renderer's [`font::GlyphStyle`].
fn glyph_style(s: RunStyle) -> font::GlyphStyle {
    font::GlyphStyle {
        bold: s.bold,
        italic: s.italic,
        underline: s.underline,
        strike: s.strike,
        highlight: s.highlight,
    }
}

/// Lay out a closed shape's label inside its box (minus [`LABEL_PAD`]): the
/// auto-shrunk font size, the wrap width, and the block's world placement,
/// centered. Shared by the paint path and the editor so the caret matches the
/// rendered glyphs exactly.
fn shape_label_block(
    font: &Font,
    kind: &ElementKind,
    bx: f32,
    by: f32,
    bw: f32,
    bh: f32,
    label: &str,
) -> LabelBlock {
    // The label wraps + shrinks to fit the shape's *inscribed rectangle* (a
    // fraction of the bounding box), not the box itself — so text never crosses a
    // slanted / round outline. Largest centered inscribed rect: ellipse 1/√2 each
    // axis, diamond ½. A triangle narrows toward its apex, so its band is ½×½
    // sitting on the base (text anchored low, not vertically centered). Star /
    // pointy-top hexagon use a safe central band. (Rect / round-rect = the box.)
    let (wf, hf, bottom) = match kind {
        ElementKind::Ellipse(_) => (
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
            false,
        ),
        ElementKind::Diamond(_) => (0.5, 0.5, false),
        ElementKind::Triangle(_) => (0.5, 0.5, true),
        ElementKind::Star(_) => (0.5, 0.4, false),
        ElementKind::Hexagon(_) => (0.8, 0.5, false),
        _ => (1.0, 1.0, false),
    };
    let wrap = (bw * wf - 2.0 * LABEL_PAD).max(1.0);
    let ih = (bh * hf - 2.0 * LABEL_PAD).max(1.0);
    let size = font.fit_size(label, wrap, ih, TEXT_SIZE);
    let (w, h) = font.measure_wrapped(label, size, Some(wrap));
    // Always horizontally centered; the triangle's band sits on the base, every
    // other shape is vertically centered too.
    let x = bx + (bw - w) / 2.0;
    let y = if bottom {
        by + bh - LABEL_PAD - h
    } else {
        by + (bh - h) / 2.0
    };
    LabelBlock { x, y, size, wrap }
}

/// World point `p` → block-local space (origin = the block's top-left `(x, y)`),
/// undoing rotation about `pivot` (the shape's center) — maps a click to a caret.
fn block_local(x: f32, y: f32, rotation: f32, pivot: [f32; 2], p: [f32; 2]) -> [f32; 2] {
    let (rx, ry) = rotate_pt(p[0], p[1], pivot[0], pivot[1], -rotation);
    [rx - x, ry - y]
}

/// Block-local point → world space, applying rotation about the block/shape pivot.
fn block_world(x: f32, y: f32, rotation: f32, pivot: [f32; 2], p: [f32; 2]) -> (f32, f32) {
    rotate_pt(x + p[0], y + p[1], pivot[0], pivot[1], rotation)
}

/// Snap target `(tx, ty)` so its angle from `(ox, oy)` is a multiple of 45°,
/// preserving the distance (the line-drawing constraint for endpoint drags).
fn snap_45(ox: f32, oy: f32, tx: f32, ty: f32) -> (f32, f32) {
    let (dx, dy) = (tx - ox, ty - oy);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 {
        return (tx, ty);
    }
    let step = std::f32::consts::FRAC_PI_4;
    let ang = (dy.atan2(dx) / step).round() * step;
    (ox + len * ang.cos(), oy + len * ang.sin())
}

/// Round a world coordinate to the nearest [`GRID`] line. Used while the snap
/// modifier (Option) is held during create / move / resize so geometry lands on
/// the visible dot grid — handy for aligning template layouts.
fn snap_grid(v: f32) -> f32 {
    (v / GRID).round() * GRID
}

/// Round an angle (radians) to the nearest quarter turn. Images rotate only in
/// 90° steps — gpui can't transform a raster sprite, so the host re-rotates the
/// pixels, and quarter turns keep that exact (no resampling) and cheap.
fn snap_quarter(rad: f32) -> f32 {
    let q = std::f32::consts::FRAC_PI_2;
    (rad / q).round() * q
}

/// Where a move-drag's primary element should sit: its grab-time top-left
/// (`origin`) plus the *total* cursor delta since the grab `anchor`, optionally
/// snapped to the grid. Driving an absolute target from the total delta (rather
/// than snapping each frame's increment) keeps the shape under the cursor and
/// lets sub-grid motion accumulate across frames instead of sticking.
fn move_target(origin: [f32; 2], anchor: [f32; 2], cursor: [f32; 2], snap: bool) -> [f32; 2] {
    let t = [
        origin[0] + (cursor[0] - anchor[0]),
        origin[1] + (cursor[1] - anchor[1]),
    ];
    if snap {
        [snap_grid(t[0]), snap_grid(t[1])]
    } else {
        t
    }
}

/// Approximate world-space (width, height) of a text element — enough for
/// hit-testing and the selection box (real shaping happens at paint time).
fn text_extent(t: &TextGeom) -> (f32, f32) {
    // Once a render has laid the text out, use the real extent. Before that
    // (e.g. a freshly loaded board, pre-first-paint), fall back to a rough
    // character-count estimate so hit-test/bounds aren't degenerate.
    if t.measured_h > 0.0 {
        return (t.measured_w, t.measured_h);
    }
    let rows = t.content.split('\n').count().max(1) as f32;
    let cols = t
        .content
        .split('\n')
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
        .max(1) as f32;
    (cols * t.size * TEXT_CHAR_W, rows * t.size * TEXT_LINE_H)
}

/// Scale an element's geometry about `(ax, ay)` by `(sx, sy)` (world space).
/// Stroke width is left unchanged.
fn resize_about(kind: &mut ElementKind, ax: f32, ay: f32, sx: f32, sy: f32) {
    let fx = |x: f32| ax + (x - ax) * sx;
    let fy = |y: f32| ay + (y - ay) * sy;
    match kind {
        ElementKind::Draw(s) => {
            for p in &mut s.points {
                p[0] = fx(p[0]);
                p[1] = fy(p[1]);
            }
        }
        ElementKind::Rect(b)
        | ElementKind::Ellipse(b)
        | ElementKind::Diamond(b)
        | ElementKind::Triangle(b)
        | ElementKind::RoundRect(b)
        | ElementKind::Star(b)
        | ElementKind::Hexagon(b) => {
            let (x0, x1) = (fx(b.x), fx(b.x + b.w));
            let (y0, y1) = (fy(b.y), fy(b.y + b.h));
            b.x = x0.min(x1);
            b.w = (x1 - x0).abs();
            b.y = y0.min(y1);
            b.h = (y1 - y0).abs();
        }
        ElementKind::Line(s) | ElementKind::Arrow(s) => {
            s.x1 = fx(s.x1);
            s.x2 = fx(s.x2);
            s.y1 = fy(s.y1);
            s.y2 = fy(s.y2);
        }
        ElementKind::Text(t) => {
            // Position follows the (possibly per-axis) scale, but a glyph has a
            // single size — never stretched. The geometric mean keeps a
            // proportional resize (sx == sy) exact and an edge drag uniform
            // (scaling by the average of the two factors).
            t.x = fx(t.x);
            t.y = fy(t.y);
            t.size = (t.size * (sx.abs() * sy.abs()).sqrt()).max(0.5);
        }
        ElementKind::Embed(em) => {
            let (x0, x1) = (fx(em.x), fx(em.x + em.w));
            let (y0, y1) = (fy(em.y), fy(em.y + em.h));
            em.x = x0.min(x1);
            em.w = (x1 - x0).abs();
            em.y = y0.min(y1);
            em.h = (y1 - y0).abs();
        }
        ElementKind::Image(im) => {
            let (x0, x1) = (fx(im.x), fx(im.x + im.w));
            let (y0, y1) = (fy(im.y), fy(im.y + im.h));
            im.x = x0.min(x1);
            im.w = (x1 - x0).abs();
            im.y = y0.min(y1);
            im.h = (y1 - y0).abs();
        }
    }
}
