use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VideoError {
    #[error("video operation was cancelled")]
    Cancelled,
    #[error("invalid video input: {0}")]
    InvalidInput(String),
    #[error("unsupported video frame layout: {0}")]
    UnsupportedFrameLayout(String),
    #[error("video process failed: {0}")]
    Process(String),
    #[error("video audio output failed: {0}")]
    Audio(String),
    #[error("video I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to create GPUI render image: {0}")]
    RenderImage(String),
    #[error("video source does not exist: {0}")]
    MissingSource(PathBuf),
}
