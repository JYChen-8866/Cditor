/// One line's caret stops: its top y and `(content byte offset, local x)` at each
/// caret position (before each char and after the last). See [`Font::line_stops`].
struct LineStops {
    top: f32,
    stops: Vec<(usize, f32)>,
}

/// The renderable style of one character — what [`Font::layout_styled`] needs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlyphStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Highlight color behind the glyphs, packed `0xRRGGBBAA`.
    pub highlight: Option<u32>,
}

/// A non-glyph text decoration in text-local space — transformed like the
/// glyphs at paint time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decoration {
    /// `[x, y, w, h]`.
    pub rect: [f32; 4],
    pub kind: DecoKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DecoKind {
    /// Filled rect behind the glyphs, in this color (`0xRRGGBBAA`).
    Highlight(u32),
    /// A bar drawn in the text color.
    Underline,
    Strike,
}

/// A laid-out styled block: glyph outlines (italic / bold baked in) plus
/// decoration rects, all text-local. Like [`TextLayout`] otherwise.
#[derive(Clone, Debug)]
pub struct StyledLayout {
    pub segs: Vec<Seg>,
    /// The bold glyphs' outlines, stroked over the fill for synthetic bold.
    pub bold_segs: Vec<Seg>,
    /// Local stroke width for `bold_segs` (scale to screen px by the zoom).
    pub bold_width: f32,
    pub decorations: Vec<Decoration>,
    pub width: f32,
    pub height: f32,
    pub line_height: f32,
    pub caret: [f32; 2],
}

/// Emit the highlight / underline / strikethrough rects for a decoration run
/// spanning local x `[x0, x1)` on one line.
// Each argument is a distinct glyph-run geometry/style value; bundling them into
// a struct would obscure more than it clarifies.
#[allow(clippy::too_many_arguments)]
fn flush_deco(
    out: &mut Vec<Decoration>,
    st: GlyphStyle,
    x0: f32,
    x1: f32,
    top: f32,
    line_height: f32,
    baseline: f32,
    bar: f32,
) {
    if x1 <= x0 {
        return;
    }
    let w = x1 - x0;
    if let Some(c) = st.highlight {
        out.push(Decoration {
            rect: [x0, top, w, line_height],
            kind: DecoKind::Highlight(c),
        });
    }
    if st.underline {
        out.push(Decoration {
            rect: [x0, baseline + bar, w, bar],
            kind: DecoKind::Underline,
        });
    }
    if st.strike {
        // Through the x-height (about halfway from the line top to the baseline).
        out.push(Decoration {
            rect: [x0, (top + baseline) * 0.5, w, bar],
            kind: DecoKind::Strike,
        });
    }
}

/// Accumulates a glyph's outline into local space as `ttf-parser` walks it.
struct Outliner<'a> {
    segs: &'a mut Vec<Seg>,
    pen: f32,
    baseline: f32,
    scale: f32,
    /// Synthetic-italic slant: local x shifts right with height above the
    /// baseline (`0.0` = upright).
    shear: f32,
}

impl Outliner<'_> {
    /// Font units (y-up, baseline origin) → text-local (y-down, top-left origin),
    /// applying the italic shear.
    fn pt(&self, x: f32, y: f32) -> [f32; 2] {
        [
            self.pen + (x + self.shear * y) * self.scale,
            self.baseline - y * self.scale,
        ]
    }
}

impl ttf_parser::OutlineBuilder for Outliner<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.segs.push(Seg::Move(p));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.segs.push(Seg::Line(p));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c = self.pt(x1, y1);
        let e = self.pt(x, y);
        self.segs.push(Seg::Quad(c, e));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.pt(x1, y1);
        let c2 = self.pt(x2, y2);
        let e = self.pt(x, y);
        self.segs.push(Seg::Cubic(c1, c2, e));
    }
    fn close(&mut self) {
        self.segs.push(Seg::Close);
    }
}
