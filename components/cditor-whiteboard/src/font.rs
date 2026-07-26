//! On-canvas text rendering for the whiteboard.
//!
//! gpui paints text as glyph sprites whose transform is fixed to the identity,
//! so native text can't rotate or follow the camera the way the board needs.
//! Instead we render text as **vector outlines**: `ttf-parser` gives glyph
//! contours, which we lay out into board-local space and (in `lib.rs`) feed to a
//! `PathBuilder` fill — so text rotates, scales, and z-orders exactly like the
//! shapes. A face is just bytes, so a host can swap in a user-uploaded font; the
//! default (JetBrains Mono, OFL — see `assets/JetBrainsMono-OFL.txt`) is bundled
//! so the crate works standalone.

use std::sync::{Arc, OnceLock};

/// The bundled default face.
const DEFAULT_FONT: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");

/// Floor for shape-label auto-shrink (world units): a label never shrinks below
/// this, even if it then slightly overflows a very small box. See [`Font::fit_size`].
const MIN_LABEL_SIZE: f32 = 1.0;

/// One glyph-outline command in text-local space: origin at the block's
/// top-left, x to the right, y *down* (screen-like), in the same world units as
/// `font_size`. Curves keep their control points so they transform under
/// rotation / the camera before being flattened by the path builder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move([f32; 2]),
    Line([f32; 2]),
    /// Quadratic Bézier: control, end.
    Quad([f32; 2], [f32; 2]),
    /// Cubic Bézier: control1, control2, end.
    Cubic([f32; 2], [f32; 2], [f32; 2]),
    Close,
}

/// A laid-out block of text: glyph outline segments plus metrics, all in
/// text-local space (origin = the block's top-left corner).
#[derive(Clone, Debug)]
pub struct TextLayout {
    pub segs: Vec<Seg>,
    /// Width of the widest line.
    pub width: f32,
    /// Total height (line count × line height).
    pub height: f32,
    /// Distance between successive line tops.
    pub line_height: f32,
    /// Top-left of the caret (just past the content), for the editing cursor.
    pub caret: [f32; 2],
}

/// A font backing whiteboard text. Holds raw TTF/OTF bytes (parsed on demand) so
/// it's cheap to clone and a host can supply its own face.
#[derive(Clone)]
pub struct Font {
    bytes: Arc<Vec<u8>>,
    index: u32,
}

impl Default for Font {
    fn default() -> Self {
        Self::system_cjk_fallback().unwrap_or_else(|| Self {
            bytes: Arc::new(DEFAULT_FONT.to_vec()),
            index: 0,
        })
    }
}

include!("font/layout_engine.rs");
include!("font/layout_types.rs");
include!("font/tests.rs");
