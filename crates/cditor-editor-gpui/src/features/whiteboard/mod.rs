#[cfg(feature = "whiteboard")]
mod actions;
#[cfg(feature = "whiteboard")]
mod backend;
#[cfg(feature = "whiteboard")]
mod cache;
#[cfg(not(feature = "whiteboard"))]
mod disabled;
#[cfg(feature = "whiteboard")]
mod render;
#[cfg(feature = "whiteboard")]
mod style;

/// The runtime reserves 480 px for the complete block. The shell contributes
/// 8 px of outer padding and 4 px of inner padding on each side, leaving a
/// stable 464 px thumbnail surface whose bottom border remains visible.
#[cfg(feature = "whiteboard")]
pub(super) const WHITEBOARD_THUMBNAIL_HEIGHT_PX: f32 = 464.0;

#[cfg(feature = "whiteboard")]
pub(crate) use backend::WhiteboardBackendEntity;
#[cfg(feature = "whiteboard")]
pub(crate) use cache::WhiteboardThumbnailCache;
#[cfg(not(feature = "whiteboard"))]
pub(crate) use disabled::WhiteboardThumbnailCache;
#[cfg(feature = "whiteboard")]
pub(crate) use render::render_whiteboard_thumbnail;
#[cfg(feature = "whiteboard")]
pub(crate) use style::whiteboard_style_fn;
