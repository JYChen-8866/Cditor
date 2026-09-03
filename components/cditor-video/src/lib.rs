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
mod stderr;
mod types;

mod runtime_binaries;

pub use error::VideoError;
pub use frame_store::{LatestVideoFrame, VideoFrameLease, VideoFrameStats, VideoFrameStore};
pub use renderer::{
    OwnedFrameRenderError, render_image_from_frame, render_image_from_owned_frame,
    render_image_from_owned_frame_recoverable,
};
pub use session::{VideoCancellationToken, VideoSession};
pub use types::{
    VideoCommand, VideoDimensions, VideoFrame, VideoPlaybackSnapshot, VideoSessionConfig,
};

pub(crate) use runtime_binaries::{ffmpeg_executable, ffprobe_executable};

// Media workers spend almost all of their lifetime blocked on process pipes or
// copying bounded frame/audio buffers. Rust's platform-default thread stack is
// disproportionate when several videos are active at once; 512 KiB still
// leaves ample room for these shallow I/O loops while bounding per-session
// virtual-memory and commit exposure.
pub(crate) const MEDIA_IO_THREAD_STACK_BYTES: usize = 512 * 1024;
