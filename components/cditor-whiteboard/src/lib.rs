//! An infinite, pannable/zoomable whiteboard canvas for GPUI.
//!
//! Host-agnostic — depends only on `gpui`, `serde`, and `ttf-parser` (no
//! `gpui-component`, no native libraries). Two layers: a serializable scene model
//! ([`Scene`] / [`Element`]) the host persists as opaque JSON, and a
//! [`WhiteboardView`] entity that renders the board *and* its editing UI (toolbar,
//! color picker, flyouts, templates gallery, context menu) and drives all
//! interaction. The host supplies a theme ([`WhiteboardStyle`]) and optional
//! callbacks (persist on change, open a page, fetch an image bitmap, read/write the
//! clipboard, store templates); with none installed it's still a working board.
//!
//! Elements: freehand pen, rect / ellipse / diamond / triangle / rounded-rect /
//! hexagon / star, line, arrow, text, images, and page-cards — sharing one select /
//! move / resize / rotate / fill / z-order machinery, plus copy-paste, templates,
//! and undo/redo. Text renders as **vector outlines** (the `font` module, via
//! `ttf-parser`) rather than gpui overlay glyphs, so it rotates + scales with the
//! camera and a host can supply a custom face ([`Font`]). See `README.md` for the
//! full API and usage; design notes in `docs/whiteboard-architecture.md`.
//!
//! Perf note: element geometry is re-tessellated when painted (as GPUI's own
//! `painting`/`brush` examples do), but rendering is viewport-culled and text
//! glyph layouts are cached. A built-`Path` cache remains a further optimization
//! for extremely dense visible scenes.

mod font;
mod render_perf;

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

pub use font::Font;
use render_perf::WorldViewport;

use gpui::{
    AnyElement, AnyView, App, AppContext, Bounds, Context, CursorStyle, Div, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, GlobalElementId, Hsla,
    InspectorElementId, InteractiveElement, IntoElement, KeyDownEvent, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, ParentElement, PathBuilder,
    PinchEvent, Pixels, Point, Render, Rgba, ScrollDelta, ScrollWheelEvent, SharedString,
    StatefulInteractiveElement, Style, Styled, StyledImage, TransformationMatrix, UTF16Selection,
    Window, canvas, div, fill, hsla, linear_color_stop, linear_gradient, point, px, relative, rgba,
    size,
};
use serde::{Deserialize, Serialize};

/// Zoom is clamped to this range (also guards the world↔screen math against a
/// zero/negative factor from hand-edited JSON).
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
/// World-space distance between grid dots.
const GRID: f32 = 24.0;
/// Smallest on-screen dot spacing before the grid is coarsened (×4).
const MIN_DOT_SPACING: f32 = 16.0;
/// Dot size in screen px (constant — dots don't grow with zoom).
const DOT: f32 = 2.0;
/// Screen px per scroll "line" for inexact (`Lines`) scroll deltas.
const LINE_PX: f32 = 16.0;
const VIEWPORT_CULL_MARGIN_PX: f32 = 96.0;

fn accepts_wheel_input(read_only: bool) -> bool {
    !read_only
}
/// Pen nib in screen px. A stored width is world-space (`NIB / zoom` at draw
/// time) so strokes/shapes feel like a constant nib yet scale with the content.
/// Also the default of [`WhiteboardView::active_width`].
const NIB: f32 = 2.5;
/// Stroke-thickness presets (screen px) offered by the toolbar thickness flyout.
/// `NIB` (the default) is one of them.
const WIDTH_PRESETS: [f32; 5] = [1.0, 2.5, 4.0, 6.0, 9.0];
/// Range (screen px) of the custom-width slider in the thickness flyout.
const WIDTH_MIN: f32 = 1.0;
const WIDTH_MAX: f32 = 20.0;
/// Slider track width, px (matches the preset row: 5 × 30 + gaps).
const WIDTH_SLIDER_W: f32 = 156.0;
/// Minimum on-screen gap between recorded freehand points (input thinning).
const MIN_POINT_PX: f32 = 2.0;
/// Hit-test tolerance around an element's bounds, in screen px.
const SELECT_PAD: f32 = 6.0;
/// Most undo steps kept (bounds memory; each step is a scene snapshot).
const UNDO_CAP: usize = 50;
/// Half-size of a corner resize handle, screen px.
const HANDLE_HALF: f32 = 4.0;
/// Grab radius for a corner handle, screen px.
const HANDLE_GRAB: f32 = 10.0;
/// Distance in screen pixels from a selected shape edge to its connector button.
const CONNECTOR_BUTTON_GAP: f32 = 24.0;
const CONNECTOR_BUTTON_SIZE: f32 = 20.0;
/// Color picker: saturation/brightness square + hue strip dimensions, px.
const SV_W: f32 = 216.0;
const SV_H: f32 = 140.0;
const HUE_H: f32 = 14.0;
/// Below this absolute rotation (radians), a box is treated as upright — it
/// shows resize corners. Rotated past it, only the rotate handle is offered
/// (rotated-frame resize is intentionally out of scope; rotate back to resize).
const ROT_EPS: f32 = 0.05;
/// While rotating, an orientation within this many radians (~6°) of horizontal
/// or vertical snaps to it, so boxes square up to the grid easily.
const ROT_SNAP: f32 = 0.105;
/// Default text size at creation, screen px (stored world size is this / zoom).
const TEXT_SIZE: f32 = 18.0;

/// Inset (world units) kept between a shape's inscribed text rectangle and its
/// border, so the auto-shrunk label never touches the edge.
const LABEL_PAD: f32 = 8.0;

/// Default highlighter color (packed `0xRRGGBBAA`) for the highlight toggle —
/// translucent yellow so the text stays readable.
const HIGHLIGHT_DEFAULT: u32 = 0xffe06680;
/// Rough per-character advance and line height, as fractions of the font size,
/// for an approximate text bounding box (hit-testing / selection).
const TEXT_CHAR_W: f32 = 0.55;
const TEXT_LINE_H: f32 = 1.3;
/// Default page-card size at creation, screen px (stored world size is / zoom).
const EMBED_W: f32 = 210.0;
const EMBED_H: f32 = 76.0;
const MINDMAP_ROOT_W: f32 = 196.0;
const MINDMAP_ROOT_H: f32 = 60.0;
const MINDMAP_NODE_W: f32 = 164.0;
const MINDMAP_NODE_H: f32 = 48.0;
const MINDMAP_BRANCH_GAP_X: f32 = 120.0;
const MINDMAP_BRANCH_GAP_Y: f32 = 84.0;
const FLOWCHART_NODE_W: f32 = 180.0;
const FLOWCHART_NODE_H: f32 = 52.0;
const FLOWCHART_GAP_Y: f32 = 92.0;
const FLOWCHART_BRANCH_GAP_X: f32 = 240.0;
/// Longest edge of a freshly placed image, screen px (aspect preserved).
const IMAGE_PLACE_PX: f32 = 280.0;

include!("model.rs");
include!("view_state.rs");

include!("view/lifecycle.rs");
include!("view/diagram_seeds.rs");
include!("view/mindmap_hierarchy.rs");
include!("view/mindmap_layout.rs");
include!("view/history_templates.rs");
include!("view/style_hit_testing.rs");
include!("view/pointer_down.rs");
include!("view/pointer_motion.rs");
include!("view/text_input.rs");

include!("embedded_views.rs");
include!("geometry/base.rs");
include!("geometry/text.rs");
include!("paint/layout.rs");
include!("paint/elements.rs");
include!("input_bridge.rs");
include!("render/read_only.rs");
include!("render/editor.rs");

#[cfg(test)]
mod tests {
    use super::*;

    include!("tests/model_and_style.rs");
    include!("tests/geometry_and_input.rs");
}
