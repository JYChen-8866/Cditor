mod cache;
mod render;
mod style;

/// The runtime reserves 480 px for the complete block. The shell contributes
/// 8 px of vertical padding, leaving a stable 472 px thumbnail surface.
pub(super) const WHITEBOARD_THUMBNAIL_HEIGHT_PX: f32 = 472.0;

pub(crate) use cache::WhiteboardThumbnailCache;
pub(crate) use render::render_whiteboard_thumbnail;
pub(crate) use style::whiteboard_style_fn;
