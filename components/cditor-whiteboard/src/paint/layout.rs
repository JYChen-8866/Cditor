/// World to absolute screen point at the current camera.
fn to_screen(wx: f32, wy: f32, cam: Camera, origin: Point<Pixels>) -> Point<Pixels> {
    let z = cam.zoom.max(MIN_ZOOM);
    point(
        px(f32::from(origin.x) + (wx - cam.x) * z),
        px(f32::from(origin.y) + (wy - cam.y) * z),
    )
}

/// Paint the board background + the world-space dot grid into `bounds`.
fn paint_board(bounds: Bounds<Pixels>, cam: Camera, bg: Hsla, grid: Hsla, window: &mut Window) {
    window.paint_quad(fill(bounds, bg));

    let z = cam.zoom.max(MIN_ZOOM);
    let mut step = GRID;
    while step * z < MIN_DOT_SPACING {
        step *= 4.0;
    }

    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let (left, top) = (cam.x, cam.y);
    let mut wx = (left / step).ceil() * step;
    while (wx - left) * z <= w {
        let sx = ox + (wx - left) * z;
        let mut wy = (top / step).ceil() * step;
        while (wy - top) * z <= h {
            let sy = oy + (wy - top) * z;
            window.paint_quad(fill(
                Bounds {
                    origin: point(px(sx - DOT / 2.0), px(sy - DOT / 2.0)),
                    size: size(px(DOT), px(DOT)),
                },
                grid,
            ));
            wy += step;
        }
        wx += step;
    }
}

/// One element prepared for the paint closure: its geometry + resolved colors,
/// plus pre-laid-out text outlines for Text elements (the layout needs the font,
/// which the paint closure can't reach, so `render` builds it up front).
struct ElemPaint {
    kind: ElementKind,
    stroke: Hsla,
    fill: Option<Hsla>,
    text: Option<TextOutline>,
    mindmap_connector_style: Option<MindMapConnectorStyle>,
}

/// One slice of the board's z-order paint stack. Canvas-drawn elements collect
/// into a `Band` (one canvas); an image or page-card is an `Overlay` div between
/// bands. `render` builds these in `elements` order so paint order = z-order,
/// which lets a shape sit above or below an image. See [`band_canvas`].
enum Layer {
    Band(Vec<ElemPaint>),
    Overlay(gpui::AnyElement),
}

/// A transparent, full-size canvas painting one run of canvas-drawn elements
/// (shapes / lines / pen / text) in order. Stacked between [`Layer::Overlay`]
/// divs so paint order follows the element list.
fn band_canvas(elems: Vec<ElemPaint>, cam: Camera) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            for ep in &elems {
                // Shapes / lines / pen paint here; Text elements are a no-op in
                // `paint_element`. Any text outline — a Text element's content or
                // a shape's label — then paints on top.
                paint_element(
                    &ep.kind,
                    ep.mindmap_connector_style,
                    cam,
                    bounds.origin,
                    ep.stroke,
                    ep.fill,
                    window,
                );
                if let Some(t) = &ep.text {
                    paint_text(t, cam, bounds.origin, window);
                }
            }
        },
    )
    .absolute()
    .size_full()
}

/// Thumbnail rendering colors grouped to keep call signatures small.
#[derive(Clone, Copy)]
struct ThumbnailPalette {
    ink: Hsla,
    text: Hsla,
    grid: Hsla,
    panel: Hsla,
}

fn build_thumbnail_layers(
    scene: &Scene,
    font: &Font,
    cam: Camera,
    palette: ThumbnailPalette,
    viewport: Option<WorldViewport>,
    mut text_layout_cache: Option<&mut HashMap<u64, CachedTextLayout>>,
    mut label_layout_cache: Option<&mut HashMap<u64, CachedLabelLayout>>,
) -> Vec<Layer> {
    let ThumbnailPalette {
        ink,
        text,
        grid,
        panel,
    } = palette;
    let mindmap_connector_styles: HashMap<u64, MindMapConnectorStyle> = scene
        .elements
        .iter()
        .filter_map(|element| {
            thumbnail_mindmap_connector_style_for_element(scene, &element.kind)
                .map(|style| (element.id, style))
        })
        .collect();
    let mut layers: Vec<Layer> = Vec::new();
    let mut band: Vec<ElemPaint> = Vec::new();
    for e in &scene.elements {
        if viewport.is_some_and(|viewport| !viewport.intersects(bbox(&e.kind))) {
            continue;
        }
        let id = e.id;
        let stroke = e.stroke.map_or(ink, u32_to_hsla);
        let fill = e.fill.map(u32_to_hsla);
        let label = e.label.as_deref();
        let label_color = e.label_color;
        let styles = e.styles.as_slice();
        match &e.kind {
            ElementKind::Embed(em) => {
                if !band.is_empty() {
                    layers.push(Layer::Band(std::mem::take(&mut band)));
                }
                layers.push(Layer::Overlay(
                    div()
                        .absolute()
                        .left(px((em.x - cam.x) * cam.zoom.max(MIN_ZOOM)))
                        .top(px((em.y - cam.y) * cam.zoom.max(MIN_ZOOM)))
                        .w(px(em.w * cam.zoom.max(MIN_ZOOM)))
                        .h(px(em.h * cam.zoom.max(MIN_ZOOM)))
                        .bg(panel)
                        .border_1()
                        .border_color(grid)
                        .rounded(px(8.0))
                        .overflow_hidden()
                        .p(px(10.0 * cam.zoom.max(MIN_ZOOM)))
                        .flex()
                        .flex_col()
                        .gap(px(3.0 * cam.zoom.max(MIN_ZOOM)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0 * cam.zoom.max(MIN_ZOOM)))
                                .text_size(px(14.0 * cam.zoom.max(MIN_ZOOM)))
                                .text_color(ink)
                                .child(div().child("▤"))
                                .child(SharedString::from(em.title.clone())),
                        )
                        .child(
                            div()
                                .text_size(px(11.0 * cam.zoom.max(MIN_ZOOM)))
                                .text_color(text)
                                .child("Page"),
                        )
                        .into_any_element(),
                ));
            }
            ElementKind::Image(im) => {
                if !band.is_empty() {
                    layers.push(Layer::Band(std::mem::take(&mut band)));
                }
                let rot = snap_quarter(im.rotation);
                let (bx, by, bw, bh) = if rot.abs() < ROT_EPS {
                    (im.x, im.y, im.w, im.h)
                } else {
                    let c = box_padded_corners(im.x, im.y, im.w, im.h, rot, 0.0);
                    let (x0, y0, x1, y1) = aabb(&c);
                    (x0, y0, x1 - x0, y1 - y0)
                };
                let zoom = cam.zoom.max(MIN_ZOOM);
                layers.push(Layer::Overlay(
                    div()
                        .absolute()
                        .left(px((bx - cam.x) * zoom))
                        .top(px((by - cam.y) * zoom))
                        .w(px(bw * zoom))
                        .h(px(bh * zoom))
                        .rounded(px(2.0))
                        .bg(panel)
                        .border_1()
                        .border_color(grid)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_size(px(11.0 * zoom))
                                .text_color(text)
                                .child("Image"),
                        )
                        .into_any_element(),
                ));
            }
            kind => {
                let text_outline = thumbnail_text_outline(
                    font,
                    kind,
                    stroke,
                    ThumbnailLabelSpec {
                        label,
                        label_color,
                        styles,
                    },
                    id,
                    text_layout_cache.as_deref_mut(),
                    label_layout_cache.as_deref_mut(),
                );
                band.push(ElemPaint {
                    kind: kind.clone(),
                    stroke,
                    fill,
                    text: text_outline,
                    mindmap_connector_style: mindmap_connector_styles.get(&id).copied(),
                });
            }
        }
    }
    if !band.is_empty() {
        layers.push(Layer::Band(band));
    }
    layers
}

/// Label inputs for thumbnail text outlines, grouped to keep signatures small.
struct ThumbnailLabelSpec<'a> {
    label: Option<&'a str>,
    label_color: Option<u32>,
    styles: &'a [StyleSpan],
}

fn thumbnail_text_outline(
    font: &Font,
    kind: &ElementKind,
    stroke: Hsla,
    label_spec: ThumbnailLabelSpec<'_>,
    element_id: u64,
    mut text_layout_cache: Option<&mut HashMap<u64, CachedTextLayout>>,
    label_layout_cache: Option<&mut HashMap<u64, CachedLabelLayout>>,
) -> Option<TextOutline> {
    let ThumbnailLabelSpec {
        label,
        label_color,
        styles,
    } = label_spec;
    if let ElementKind::Text(t) = kind {
        let layout = match text_layout_cache.as_deref_mut() {
            Some(cache) => {
                cached_text_layout(cache, font, element_id, &t.content, t.size, None, styles)
            }
            None => prepare_text_layout(font, &t.content, t.size, None, styles, 0),
        };
        return Some(TextOutline {
            segs: layout.segs.clone(),
            bold_segs: layout.bold_segs.clone(),
            bold_width: layout.bold_width,
            color: stroke,
            x: t.x,
            y: t.y,
            rotation: t.rotation,
            pivot: [t.x + layout.width / 2.0, t.y + layout.height / 2.0],
            line_height: layout.line_height,
            caret: None,
            selection: Vec::new(),
            sel_color: hsla(0.0, 0.0, 0.0, 0.0),
            decorations: layout.decorations,
        });
    }
    if is_closed_shape(kind)
        && let Some((bx, by, bw, bh, rot)) = box_like(kind)
        && label.is_some_and(|s| !s.trim().is_empty())
    {
        let text = label.map_or("", str::trim);
        let cached_label = label_layout_cache.map(|cache| {
            cached_label_layout(
                cache,
                font,
                element_id,
                kind,
                LabelBox {
                    x: bx,
                    y: by,
                    w: bw,
                    h: bh,
                },
                text,
                styles,
            )
        });
        let (x, y, layout) = if let Some(label) = cached_label {
            (bx + label.offset_x, by + label.offset_y, label.text)
        } else {
            let block = shape_label_block(font, kind, bx, by, bw, bh, text);
            let layout = match text_layout_cache {
                Some(cache) => cached_text_layout(
                    cache,
                    font,
                    element_id,
                    text,
                    block.size,
                    Some(block.wrap),
                    styles,
                ),
                None => prepare_text_layout(font, text, block.size, Some(block.wrap), styles, 0),
            };
            (block.x, block.y, layout)
        };
        return Some(TextOutline {
            segs: layout.segs.clone(),
            bold_segs: layout.bold_segs.clone(),
            bold_width: layout.bold_width,
            color: label_color.map_or(stroke, u32_to_hsla),
            x,
            y,
            rotation: rot,
            pivot: [bx + bw / 2.0, by + bh / 2.0],
            line_height: layout.line_height,
            caret: None,
            selection: Vec::new(),
            sel_color: hsla(0.0, 0.0, 0.0, 0.0),
            decorations: layout.decorations,
        });
    }
    None
}

fn thumbnail_mindmap_meta(scene: &Scene, id: u64) -> Option<MindMapNodeMeta> {
    scene
        .elements
        .iter()
        .find(|e| e.id == id)
        .and_then(|e| e.mindmap)
}

fn thumbnail_mindmap_root_of(scene: &Scene, id: u64) -> Option<u64> {
    let mut current = id;
    loop {
        let meta = thumbnail_mindmap_meta(scene, current)?;
        match meta.parent {
            Some(parent) => current = parent,
            None => return Some(current),
        }
    }
}

fn thumbnail_mindmap_connector_style_for_root(
    scene: &Scene,
    root_id: u64,
) -> MindMapConnectorStyle {
    thumbnail_mindmap_meta(scene, root_id)
        .map(|meta| meta.connector_style)
        .unwrap_or_default()
}

fn thumbnail_mindmap_connector_style_for_element(
    scene: &Scene,
    kind: &ElementKind,
) -> Option<MindMapConnectorStyle> {
    let seg = match kind {
        ElementKind::Line(seg) | ElementKind::Arrow(seg) => seg,
        _ => return None,
    };
    let start_root = seg
        .start_anchor
        .and_then(|anchor| thumbnail_mindmap_root_of(scene, anchor.element_id));
    let end_root = seg
        .end_anchor
        .and_then(|anchor| thumbnail_mindmap_root_of(scene, anchor.element_id));
    match (start_root, end_root) {
        (Some(a), Some(b)) if a == b => Some(thumbnail_mindmap_connector_style_for_root(scene, a)),
        _ => None,
    }
}

/// A text element's glyph outlines (text-local space) plus placement, captured
/// for the paint closure to transform (camera + rotation) and fill.
#[derive(Clone)]
struct CachedTextLayout {
    signature: u64,
    segs: Arc<[font::Seg]>,
    bold_segs: Arc<[font::Seg]>,
    bold_width: f32,
    decorations: Arc<[font::Decoration]>,
    width: f32,
    height: f32,
    line_height: f32,
}

#[derive(Clone)]
struct CachedLabelLayout {
    signature: u64,
    offset_x: f32,
    offset_y: f32,
    size: f32,
    wrap: f32,
    text: CachedTextLayout,
}

fn hash_text_styles(styles: &[StyleSpan], hasher: &mut impl Hasher) {
    for span in styles {
        span.start.hash(hasher);
        span.end.hash(hasher);
        span.style.bold.hash(hasher);
        span.style.italic.hash(hasher);
        span.style.underline.hash(hasher);
        span.style.strike.hash(hasher);
        span.style.highlight.hash(hasher);
    }
}

fn text_layout_signature(
    content: &str,
    size: f32,
    max_width: Option<f32>,
    styles: &[StyleSpan],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    size.to_bits().hash(&mut hasher);
    max_width.map(f32::to_bits).hash(&mut hasher);
    hash_text_styles(styles, &mut hasher);
    hasher.finish()
}

/// Label bounding box in world coordinates.
#[derive(Clone, Copy)]
struct LabelBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn cached_label_layout(
    cache: &mut HashMap<u64, CachedLabelLayout>,
    font: &Font,
    element_id: u64,
    kind: &ElementKind,
    bounds: LabelBox,
    content: &str,
    styles: &[StyleSpan],
) -> CachedLabelLayout {
    let LabelBox {
        x: bx,
        y: by,
        w: bw,
        h: bh,
    } = bounds;
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    bw.to_bits().hash(&mut hasher);
    bh.to_bits().hash(&mut hasher);
    std::mem::discriminant(kind).hash(&mut hasher);
    hash_text_styles(styles, &mut hasher);
    let signature = hasher.finish();
    if let Some(layout) = cache
        .get(&element_id)
        .filter(|layout| layout.signature == signature)
    {
        return layout.clone();
    }

    let block = shape_label_block(font, kind, bx, by, bw, bh, content);
    let text = prepare_text_layout(
        font,
        content,
        block.size,
        Some(block.wrap),
        styles,
        signature,
    );
    let cached = CachedLabelLayout {
        signature,
        offset_x: block.x - bx,
        offset_y: block.y - by,
        size: block.size,
        wrap: block.wrap,
        text,
    };
    cache.insert(element_id, cached.clone());
    cached
}

fn cached_text_layout(
    cache: &mut HashMap<u64, CachedTextLayout>,
    font: &Font,
    element_id: u64,
    content: &str,
    size: f32,
    max_width: Option<f32>,
    styles: &[StyleSpan],
) -> CachedTextLayout {
    let signature = text_layout_signature(content, size, max_width, styles);
    if let Some(layout) = cache
        .get(&element_id)
        .filter(|layout| layout.signature == signature)
    {
        return layout.clone();
    }
    let cached = prepare_text_layout(font, content, size, max_width, styles, signature);
    cache.insert(element_id, cached.clone());
    cached
}

fn prepare_text_layout(
    font: &Font,
    content: &str,
    size: f32,
    max_width: Option<f32>,
    styles: &[StyleSpan],
    signature: u64,
) -> CachedTextLayout {
    let layout = font.layout_styled(content, size, max_width, |byte| {
        glyph_style(style_at(styles, byte))
    });
    CachedTextLayout {
        signature,
        segs: layout.segs.into(),
        bold_segs: layout.bold_segs.into(),
        bold_width: layout.bold_width,
        decorations: layout.decorations.into(),
        width: layout.width,
        height: layout.height,
        line_height: layout.line_height,
    }
}

struct TextOutline {
    segs: Arc<[font::Seg]>,
    /// Bold glyphs' outlines, stroked over the fill (synthetic bold), + the
    /// local stroke width.
    bold_segs: Arc<[font::Seg]>,
    bold_width: f32,
    /// Glyph fill color — a Text element's ink, or a shape label's color.
    color: Hsla,
    x: f32,
    y: f32,
    rotation: f32,
    /// Rotation pivot (world): the shape's center, so an off-center label (a
    /// triangle's base-anchored text) still rotates with the shape.
    pivot: [f32; 2],
    line_height: f32,
    /// Caret's text-local top, when this text is being edited.
    caret: Option<[f32; 2]>,
    /// Selection highlight rects (text-local `[x, y, w, h]`), when editing.
    selection: Vec<[f32; 4]>,
    /// Fill color for the selection highlight.
    sel_color: Hsla,
    /// Underline / strikethrough / highlight runs (text-local), from the styling.
    decorations: Arc<[font::Decoration]>,
}

/// Paint a text element's vector outlines (and, when editing, its caret). Local
/// glyph points are placed at `(x, y)`, rotated about the block's center, then
/// projected to the screen — so text rotates and scales like the shapes.
fn paint_text(t: &TextOutline, cam: Camera, origin: Point<Pixels>, window: &mut Window) {
    let color = t.color;
    let (cx, cy) = (t.pivot[0], t.pivot[1]);
    let tf = |p: [f32; 2]| {
        let (rx, ry) = rotate_pt(t.x + p[0], t.y + p[1], cx, cy, t.rotation);
        to_screen(rx, ry, cam, origin)
    };
    // Convert the two-thirds-toward-the-control-point so a quadratic Bézier
    // becomes the equivalent cubic the path builder accepts.
    let two_thirds = |a: Point<Pixels>, b: Point<Pixels>| {
        point(
            px(f32::from(a.x) + (f32::from(b.x) - f32::from(a.x)) * 2.0 / 3.0),
            px(f32::from(a.y) + (f32::from(b.y) - f32::from(a.y)) * 2.0 / 3.0),
        )
    };
    // A text-local `[x, y, w, h]` rect → a screen-space fill path (rotated like
    // the glyphs). Shared by highlights, the selection, and under/strike bars.
    let rect_path = |r: [f32; 4]| {
        let (x, y, w, h) = (r[0], r[1], r[2], r[3]);
        let mut pb = PathBuilder::fill();
        pb.move_to(tf([x, y]));
        pb.line_to(tf([x + w, y]));
        pb.line_to(tf([x + w, y + h]));
        pb.line_to(tf([x, y + h]));
        pb.close();
        pb.build().ok()
    };
    // Highlights, then the editing selection — both behind the glyphs.
    for d in t.decorations.iter() {
        if let font::DecoKind::Highlight(c) = d.kind
            && let Some(path) = rect_path(d.rect)
        {
            window.paint_path(path, u32_to_hsla(c));
        }
    }
    for r in &t.selection {
        if let Some(path) = rect_path(*r) {
            window.paint_path(path, t.sel_color);
        }
    }
    // Walk glyph segments into `pb` (a fill or a stroke path).
    let emit = |pb: &mut PathBuilder, segs: &[font::Seg]| {
        let mut cur = point(px(0.0), px(0.0));
        for seg in segs {
            match *seg {
                font::Seg::Move(p) => {
                    cur = tf(p);
                    pb.move_to(cur);
                }
                font::Seg::Line(p) => {
                    cur = tf(p);
                    pb.line_to(cur);
                }
                font::Seg::Quad(c, e) => {
                    let (sc, se) = (tf(c), tf(e));
                    pb.cubic_bezier_to(se, two_thirds(cur, sc), two_thirds(se, sc));
                    cur = se;
                }
                font::Seg::Cubic(c1, c2, e) => {
                    let se = tf(e);
                    pb.cubic_bezier_to(se, tf(c1), tf(c2));
                    cur = se;
                }
                font::Seg::Close => pb.close(),
            }
        }
    };
    if !t.segs.is_empty() {
        let mut pb = PathBuilder::fill();
        emit(&mut pb, &t.segs);
        if let Ok(path) = pb.build() {
            window.paint_path(path, color);
        }
    }
    // Synthetic bold: stroke the bold glyphs' outlines over the solid fill (a
    // doubled fill would cancel under even-odd winding and read as hollow).
    if !t.bold_segs.is_empty() {
        let zoom = cam.zoom.max(MIN_ZOOM);
        let mut pb = PathBuilder::stroke(px((t.bold_width * zoom).max(0.5)));
        emit(&mut pb, &t.bold_segs);
        if let Ok(path) = pb.build() {
            window.paint_path(path, color);
        }
    }
    // Underline / strikethrough bars, in the text color, over the glyphs.
    for d in t.decorations.iter() {
        if matches!(d.kind, font::DecoKind::Underline | font::DecoKind::Strike)
            && let Some(path) = rect_path(d.rect)
        {
            window.paint_path(path, color);
        }
    }
    if let Some(cp) = t.caret {
        let mut pb = PathBuilder::stroke(px(1.5));
        pb.move_to(tf(cp));
        pb.line_to(tf([cp[0], cp[1] + t.line_height]));
        if let Ok(path) = pb.build() {
            window.paint_path(path, color);
        }
    }
}
