impl Font {
    fn system_cjk_fallback() -> Option<Self> {
        static SYSTEM_CJK: OnceLock<Option<(Arc<Vec<u8>>, u32)>> = OnceLock::new();

        SYSTEM_CJK
            .get_or_init(Self::load_system_cjk_fallback)
            .as_ref()
            .map(|(bytes, index)| Self {
                bytes: bytes.clone(),
                index: *index,
            })
    }

    fn load_system_cjk_fallback() -> Option<(Arc<Vec<u8>>, u32)> {
        // The editor uses GPUI's text stack, which gets system font fallback for
        // free. Whiteboard text is converted to vector outlines, so we need a
        // single face that actually contains CJK glyphs. Prefer broad Unicode / CJK
        // system fonts, then fall back to the bundled Latin-only JetBrains Mono.
        // This lookup is cached by `system_cjk_fallback`: CJK collections are often
        // tens of megabytes and must not be re-read for every whiteboard entity.
        let candidates: &[(&str, u32)] = &[
            // macOS
            ("/Library/Fonts/Arial Unicode.ttf", 0),
            ("/System/Library/Fonts/PingFang.ttc", 0),
            ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
            ("/System/Library/Fonts/STHeiti Medium.ttc", 0),
            ("/System/Library/Fonts/STHeiti Light.ttc", 0),
            // Windows
            ("C:/Windows/Fonts/msyh.ttc", 0),
            ("C:/Windows/Fonts/simhei.ttf", 0),
            ("C:/Windows/Fonts/simsun.ttc", 0),
            // Linux common CJK packages
            ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
            ("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc", 0),
            (
                "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
                0,
            ),
        ];

        candidates.iter().find_map(|(path, index)| {
            let bytes = std::fs::read(path).ok()?;
            ttf_parser::Face::parse(&bytes, *index).ok()?;
            Some((Arc::new(bytes), *index))
        })
    }

    /// Build a font from raw face bytes (e.g. a user-uploaded `.ttf`/`.otf`),
    /// returning `None` if they don't parse. `index` selects a face within a
    /// collection (`.ttc`); pass 0 otherwise.
    pub fn from_bytes(bytes: Vec<u8>, index: u32) -> Option<Self> {
        ttf_parser::Face::parse(&bytes, index).ok()?;
        Some(Self {
            bytes: Arc::new(bytes),
            index,
        })
    }

    fn face(&self) -> Option<ttf_parser::Face<'_>> {
        ttf_parser::Face::parse(&self.bytes, self.index).ok()
    }

    /// The line height (top-to-top spacing) at `font_size`, world units.
    fn line_height_of(face: &ttf_parser::Face, scale: f32) -> f32 {
        (face.ascender() as f32 - face.descender() as f32 + face.line_gap() as f32) * scale
    }

    /// Lay out `content` at `font_size` (world units) into local-space outlines.
    /// Newlines break lines; the block's origin is its top-left corner.
    pub fn layout(&self, content: &str, font_size: f32) -> TextLayout {
        self.layout_wrapped(content, font_size, None)
    }

    /// Like [`layout`](Self::layout) but word-wraps to `max_width` (world units)
    /// when `Some` — for fitting a label inside a shape. Glyph positions come
    /// from the same [`line_stops`](Self::line_stops) the caret uses, so the
    /// caret always lands exactly between the rendered glyphs.
    pub fn layout_wrapped(
        &self,
        content: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> TextLayout {
        let Some(face) = self.face() else {
            return TextLayout {
                segs: Vec::new(),
                width: 0.0,
                height: font_size,
                line_height: font_size,
                caret: [0.0, 0.0],
            };
        };
        let upem = face.units_per_em() as f32;
        let scale = if upem > 0.0 { font_size / upem } else { 0.0 };
        let ascent = face.ascender() as f32 * scale;
        let (lines, line_height) = self.line_stops(content, font_size, max_width);

        let mut segs = Vec::new();
        let mut width = 0.0_f32;
        // `line_stops` always yields ≥1 visual line, so "" is one empty line and
        // a trailing newline yields a final empty line holding the caret.
        for line in &lines {
            let lstart = line.stops.first().map_or(0, |&(b, _)| b);
            let lend = line.stops.last().map_or(lstart, |&(b, _)| b);
            let baseline = line.top + ascent;
            for (k, ch) in content.get(lstart..lend).unwrap_or("").chars().enumerate() {
                let pen = line.stops.get(k).map_or(0.0, |&(_, x)| x);
                let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
                let mut b = Outliner {
                    segs: &mut segs,
                    pen,
                    baseline,
                    scale,
                    shear: 0.0,
                };
                face.outline_glyph(gid, &mut b);
            }
            width = width.max(line.stops.last().map_or(0.0, |&(_, x)| x));
        }
        let caret = lines.last().map_or([0.0, 0.0], |l| {
            let &(_, x) = l.stops.last().unwrap();
            [x, l.top]
        });
        TextLayout {
            segs,
            width,
            height: lines.len().max(1) as f32 * line_height,
            line_height,
            caret,
        }
    }

    /// Like [`layout_wrapped`](Self::layout_wrapped) but applies a per-character
    /// [`GlyphStyle`] from `style_at` (keyed by byte offset): synthetic italic
    /// (shear) and bold (a doubled, offset pass) are baked into the glyph
    /// outlines, and underline / strikethrough / highlight runs become
    /// [`Decoration`]s. Wrapping — and therefore the caret geometry — is identical
    /// to the unstyled path (synthetic styling doesn't change advances).
    pub fn layout_styled(
        &self,
        content: &str,
        font_size: f32,
        max_width: Option<f32>,
        style_at: impl Fn(usize) -> GlyphStyle,
    ) -> StyledLayout {
        let Some(face) = self.face() else {
            return StyledLayout {
                segs: Vec::new(),
                bold_segs: Vec::new(),
                bold_width: 0.0,
                decorations: Vec::new(),
                width: 0.0,
                height: font_size,
                line_height: font_size,
                caret: [0.0, 0.0],
            };
        };
        let upem = face.units_per_em() as f32;
        let scale = if upem > 0.0 { font_size / upem } else { 0.0 };
        let ascent = face.ascender() as f32 * scale;
        let (lines, line_height) = self.line_stops(content, font_size, max_width);

        const SHEAR: f32 = 0.22; // ~12° oblique
        let bar = (font_size * 0.07).max(1.0); // underline / strike thickness

        let mut segs = Vec::new();
        // Bold glyphs' outlines, stroked over the solid fill (`bold_width`). A
        // doubled fill would cancel under even-odd winding → hollow glyphs.
        let mut bold_segs = Vec::new();
        let mut decorations = Vec::new();
        let mut width = 0.0_f32;
        for line in &lines {
            let lstart = line.stops.first().map_or(0, |&(b, _)| b);
            let lend = line.stops.last().map_or(lstart, |&(b, _)| b);
            let baseline = line.top + ascent;
            // The current decoration run: its style and starting local x. Only the
            // underline/strike/highlight bits matter (bold/italic are glyph-baked).
            let mut run: Option<(GlyphStyle, f32)> = None;
            let mut byte = lstart;
            for (k, ch) in content.get(lstart..lend).unwrap_or("").chars().enumerate() {
                let st = style_at(byte);
                let pen = line.stops.get(k).map_or(0.0, |&(_, x)| x);
                let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
                let shear = if st.italic { SHEAR } else { 0.0 };
                let start = segs.len();
                {
                    let mut b = Outliner {
                        segs: &mut segs,
                        pen,
                        baseline,
                        scale,
                        shear,
                    };
                    face.outline_glyph(gid, &mut b);
                }
                if st.bold {
                    bold_segs.extend_from_slice(&segs[start..]);
                }
                // Track the decoration run; flush when underline/strike/highlight change.
                let deco = GlyphStyle {
                    bold: false,
                    italic: false,
                    ..st
                };
                let has_deco = deco.underline || deco.strike || deco.highlight.is_some();
                let same = matches!(&run, Some((rs, _)) if *rs == deco);
                if !same {
                    if let Some((rs, x0)) = run.take() {
                        flush_deco(
                            &mut decorations,
                            rs,
                            x0,
                            pen,
                            line.top,
                            line_height,
                            baseline,
                            bar,
                        );
                    }
                    run = has_deco.then_some((deco, pen));
                }
                byte += ch.len_utf8();
            }
            if let Some((rs, x0)) = run.take() {
                let x1 = line.stops.last().map_or(x0, |&(_, x)| x);
                flush_deco(
                    &mut decorations,
                    rs,
                    x0,
                    x1,
                    line.top,
                    line_height,
                    baseline,
                    bar,
                );
            }
            width = width.max(line.stops.last().map_or(0.0, |&(_, x)| x));
        }
        let caret = lines.last().map_or([0.0, 0.0], |l| {
            let &(_, x) = l.stops.last().unwrap();
            [x, l.top]
        });
        StyledLayout {
            segs,
            bold_segs,
            bold_width: font_size * 0.06,
            decorations,
            width,
            height: lines.len().max(1) as f32 * line_height,
            line_height,
            caret,
        }
    }

    /// The block's `(width, height)` at `font_size` without building outlines —
    /// for selection bounds and hit-testing (no `Window` needed).
    pub fn measure(&self, content: &str, font_size: f32) -> (f32, f32) {
        self.measure_wrapped(content, font_size, None)
    }

    /// Like [`measure`](Self::measure) but wraps to `max_width` (world units)
    /// when `Some` — the height then reflects the wrapped line count.
    pub fn measure_wrapped(
        &self,
        content: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        let (lines, line_height) = self.line_stops(content, font_size, max_width);
        let width = lines.iter().fold(0.0_f32, |m, l| {
            m.max(l.stops.last().map_or(0.0, |&(_, x)| x))
        });
        (width, lines.len().max(1) as f32 * line_height)
    }

    /// The largest font size in `(0, max_size]` at which `content`, wrapped to
    /// `max_w`, fits within a `max_w × max_h` box (world units) — so a shape
    /// label can shrink until it no longer crosses the shape's border. Returns
    /// `max_size` for empty content or a non-positive box, and never goes below
    /// [`MIN_LABEL_SIZE`] (a tiny box may then still be overflowed).
    pub fn fit_size(&self, content: &str, max_w: f32, max_h: f32, max_size: f32) -> f32 {
        if content.is_empty() || max_w <= 0.0 || max_h <= 0.0 {
            return max_size;
        }
        let fits = |size: f32| {
            let (w, h) = self.measure_wrapped(content, size, Some(max_w));
            w <= max_w + 0.5 && h <= max_h + 0.5
        };
        if fits(max_size) {
            return max_size;
        }
        let (mut lo, mut hi) = (MIN_LABEL_SIZE, max_size.max(MIN_LABEL_SIZE));
        for _ in 0..20 {
            let mid = 0.5 * (lo + hi);
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// Per-visual-line caret stops for `content`. Lines break on `'\n'` always
    /// and, when `max_width` is `Some(w)` (world units), also wrap greedily at
    /// word boundaries so each visual line stays within `w`; a single word wider
    /// than `w` is split between characters so it never overflows. For every
    /// visual line: the content **byte offset** and local x of each caret
    /// position (before each char and after the last), plus the line's top y.
    /// Soft wraps consume no byte, so offsets stay contiguous across visual
    /// lines. The backbone of caret placement, click hit-testing, and selection
    /// rects. Byte offsets index into `content`.
    fn line_stops(
        &self,
        content: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> (Vec<LineStops>, f32) {
        let Some(face) = self.face() else {
            return (
                vec![LineStops {
                    top: 0.0,
                    stops: vec![(0, 0.0)],
                }],
                font_size,
            );
        };
        let upem = face.units_per_em() as f32;
        let scale = if upem > 0.0 { font_size / upem } else { 0.0 };
        let line_height = Self::line_height_of(&face, scale);
        let wrap = max_width.filter(|w| *w > 0.0);

        let mut lines = Vec::new();
        let mut byte = 0usize;
        let mut top = 0.0_f32;
        for line in content.split('\n') {
            // Flat caret stops for the whole hard line (x cumulative from its
            // start), plus whether each char is a space (a wrap opportunity).
            let mut flat: Vec<(usize, f32)> = Vec::with_capacity(line.len() + 1);
            let mut space_after: Vec<bool> = Vec::with_capacity(line.len());
            let mut pen = 0.0_f32;
            let mut b = byte;
            flat.push((b, 0.0));
            for ch in line.chars() {
                let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
                pen += face.glyph_hor_advance(gid).unwrap_or(0) as f32 * scale;
                b += ch.len_utf8();
                flat.push((b, pen));
                space_after.push(ch == ' ');
            }

            match wrap {
                // No wrapping: the whole hard line is one visual line.
                None => {
                    lines.push(LineStops { top, stops: flat });
                    top += line_height;
                }
                // Greedy word-wrap of `flat` into visual segments ≤ `w`.
                Some(w) => {
                    let n = flat.len();
                    let mut s = 0usize; // segment start index into `flat`
                    let mut e = 1usize; // current end index
                    let mut last_break: Option<usize> = None; // index just past a space
                    while e < n {
                        if flat[e].1 - flat[s].1 > w && e - s > 1 {
                            // Break at the last space if any, else split the word.
                            let brk = match last_break {
                                Some(lb) if lb > s => lb,
                                _ => e - 1,
                            };
                            let base = flat[s].1;
                            lines.push(LineStops {
                                top,
                                stops: flat[s..=brk]
                                    .iter()
                                    .map(|&(by, x)| (by, x - base))
                                    .collect(),
                            });
                            top += line_height;
                            s = brk;
                            e = s + 1;
                            last_break = None;
                            continue;
                        }
                        if space_after.get(e - 1).copied().unwrap_or(false) {
                            last_break = Some(e);
                        }
                        e += 1;
                    }
                    let base = flat[s].1;
                    lines.push(LineStops {
                        top,
                        stops: flat[s..n].iter().map(|&(by, x)| (by, x - base)).collect(),
                    });
                    top += line_height;
                }
            }
            byte += line.len() + 1; // +1 for the '\n' (harmless past the last line)
        }
        (lines, line_height)
    }

    /// Local-space top-left of the caret at content byte offset `at`. Out-of-range
    /// offsets clamp to the end of the text.
    pub fn caret_pos(&self, content: &str, font_size: f32, at: usize) -> [f32; 2] {
        self.caret_pos_wrapped(content, font_size, None, at)
    }

    /// [`caret_pos`](Self::caret_pos) honoring a `max_width` wrap (label editing).
    pub fn caret_pos_wrapped(
        &self,
        content: &str,
        font_size: f32,
        max_width: Option<f32>,
        at: usize,
    ) -> [f32; 2] {
        let (lines, _) = self.line_stops(content, font_size, max_width);
        for line in &lines {
            // A boundary byte belongs to exactly one line (the '\n' separates
            // line i's end from line i+1's start), so the first match is correct.
            for &(b, x) in &line.stops {
                if b == at {
                    return [x, line.top];
                }
            }
        }
        lines
            .last()
            .map(|l| {
                let &(_, x) = l.stops.last().unwrap();
                [x, l.top]
            })
            .unwrap_or([0.0, 0.0])
    }

    /// The content byte offset whose caret sits nearest the local point `p`
    /// (text-local space). Picks the line by y, then the closest caret stop by x —
    /// so a click lands the caret between letters like a real text field.
    pub fn index_at(&self, content: &str, font_size: f32, p: [f32; 2]) -> usize {
        self.index_at_wrapped(content, font_size, None, p)
    }

    /// [`index_at`](Self::index_at) honoring a `max_width` wrap (label editing).
    pub fn index_at_wrapped(
        &self,
        content: &str,
        font_size: f32,
        max_width: Option<f32>,
        p: [f32; 2],
    ) -> usize {
        let (lines, line_height) = self.line_stops(content, font_size, max_width);
        if lines.is_empty() {
            return 0;
        }
        let li = if line_height > 0.0 {
            (p[1] / line_height)
                .floor()
                .clamp(0.0, (lines.len() - 1) as f32) as usize
        } else {
            0
        };
        let line = &lines[li];
        let mut best = line.stops[0];
        for &(b, x) in &line.stops {
            if (x - p[0]).abs() < (best.1 - p[0]).abs() {
                best = (b, x);
            }
        }
        best.0
    }

    /// Local-space highlight rectangles `[x, y, w, h]` for the selection
    /// `[start, end)` (byte offsets), one per line it covers. Lines fully inside a
    /// multi-line selection get a small trailing stub so the selected newline reads.
    pub fn selection_rects(
        &self,
        content: &str,
        font_size: f32,
        start: usize,
        end: usize,
    ) -> Vec<[f32; 4]> {
        self.selection_rects_wrapped(content, font_size, None, start, end)
    }

    /// [`selection_rects`](Self::selection_rects) honoring a `max_width` wrap.
    pub fn selection_rects_wrapped(
        &self,
        content: &str,
        font_size: f32,
        max_width: Option<f32>,
        start: usize,
        end: usize,
    ) -> Vec<[f32; 4]> {
        if start >= end {
            return Vec::new();
        }
        let (lines, line_height) = self.line_stops(content, font_size, max_width);
        let stub = font_size * 0.3; // trailing width shown for a selected newline
        let mut rects = Vec::new();
        for line in &lines {
            let lstart = line.stops[0].0;
            let lend = line.stops[line.stops.len() - 1].0;
            if end <= lstart || start > lend {
                continue; // selection doesn't touch this line
            }
            let x_at = |byte: usize| {
                line.stops
                    .iter()
                    .find(|(b, _)| *b == byte)
                    .map_or(0.0, |(_, x)| *x)
            };
            let x0 = x_at(start.max(lstart));
            let x1 = x_at(end.min(lend));
            // The newline (and anything below) is selected when `end` runs past
            // this line — show a stub so an end-of-line/empty-line selection reads.
            let w = (x1 - x0).max(if end > lend { stub } else { 0.0 });
            if w > 0.0 {
                rects.push([x0, line.top, w, line_height]);
            }
        }
        rects
    }
}
