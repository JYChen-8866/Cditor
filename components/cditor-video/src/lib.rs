//! A reusable video playback component extracted from Frame's preview
//! pipeline and adapted to Cditor's pinned GPUI version.
//!
//! The component owns video decoding, audio output, playback state, and GPUI
//! frame conversion. It does not depend on
//! Frame's conversion configuration or application state. FFmpeg is discovered
//! from an environment override, bundled `resources/binaries`, and finally `ffmpeg` on `PATH`.

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod audio;
mod error;
mod frame_store;
mod renderer;
mod session;
mod types;

mod runtime_binaries;

pub use error::VideoError;
pub use frame_store::{LatestVideoFrame, VideoFrameStats, VideoFrameStore};
pub use renderer::render_image_from_frame;
pub use session::{VideoCancellationToken, VideoSession};
pub use types::{
    VideoCommand, VideoDimensions, VideoFrame, VideoPlaybackSnapshot, VideoSessionConfig,
};

pub(crate) use runtime_binaries::{ffmpeg_executable, ffprobe_executable};
