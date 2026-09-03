use std::path::PathBuf;

use crate::VideoError;

pub const DEFAULT_VIDEO_MAX_WIDTH: u32 = 1280;
pub const DEFAULT_VIDEO_MAX_HEIGHT: u32 = 720;
pub const DEFAULT_VIDEO_FPS: u32 = 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoPixelFormat {
    Bgra,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub timestamp_us: u64,
    pub pixel_format: VideoPixelFormat,
    data: Vec<u8>,
}

impl VideoFrame {
    pub fn bgra(
        width: u32,
        height: u32,
        stride: u32,
        timestamp_us: u64,
        data: Vec<u8>,
    ) -> Result<Self, VideoError> {
        validate_frame_layout(width, height, stride, data.len())?;
        Ok(Self {
            width,
            height,
            stride,
            timestamp_us,
            pixel_format: VideoPixelFormat::Bgra,
            data,
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    pub(crate) fn into_tight_bgra_bytes(self) -> Result<Vec<u8>, VideoError> {
        let row_len = usize::try_from(self.width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or_else(|| VideoError::UnsupportedFrameLayout("row length overflow".into()))?;
        let height = usize::try_from(self.height)
            .map_err(|_| VideoError::UnsupportedFrameLayout("frame height overflow".into()))?;
        let tight_len = row_len
            .checked_mul(height)
            .ok_or_else(|| VideoError::UnsupportedFrameLayout("frame length overflow".into()))?;
        if usize::try_from(self.stride).ok() == Some(row_len) {
            let mut data = self.data;
            data.truncate(tight_len);
            return Ok(data);
        }
        compact_bgra_rows(
            &self.data,
            usize::try_from(self.stride).unwrap_or(usize::MAX),
            row_len,
            height,
        )
    }
}

#[derive(Clone, Debug)]
pub struct VideoSessionConfig {
    pub source: PathBuf,
    pub max_width: u32,
    pub max_height: u32,
    pub fps: u32,
}

impl Default for VideoSessionConfig {
    fn default() -> Self {
        Self {
            source: PathBuf::new(),
            max_width: DEFAULT_VIDEO_MAX_WIDTH,
            max_height: DEFAULT_VIDEO_MAX_HEIGHT,
            fps: DEFAULT_VIDEO_FPS,
        }
    }
}

impl VideoSessionConfig {
    pub fn validate(&self) -> Result<(), VideoError> {
        if self.source.as_os_str().is_empty() {
            return Err(VideoError::InvalidInput("video source is empty".into()));
        }
        if !self.source.exists() {
            return Err(VideoError::MissingSource(self.source.clone()));
        }
        if !(16..=3840).contains(&self.max_width) || !(16..=3840).contains(&self.max_height) {
            return Err(VideoError::InvalidInput(
                "video bounds must be between 16 and 3840 pixels".into(),
            ));
        }
        if !(1..=60).contains(&self.fps) {
            return Err(VideoError::InvalidInput(
                "video fps must be between 1 and 60".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoPlaybackSnapshot {
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub playing: bool,
    pub ended: bool,
    pub volume: f32,
    pub muted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VideoCommand {
    Play,
    Pause,
    Seek(f64),
    SetVolume(f32),
    SetMuted(bool),
}

pub(crate) fn validate_frame_layout(
    width: u32,
    height: u32,
    stride: u32,
    byte_len: usize,
) -> Result<(), VideoError> {
    if width == 0 || height == 0 {
        return Err(VideoError::UnsupportedFrameLayout(
            "dimensions are zero".into(),
        ));
    }
    let row_len = width
        .checked_mul(4)
        .ok_or_else(|| VideoError::UnsupportedFrameLayout("row length overflow".into()))?;
    if stride < row_len {
        return Err(VideoError::UnsupportedFrameLayout(
            "stride is too small".into(),
        ));
    }
    let expected = usize::try_from(stride)
        .ok()
        .and_then(|stride| {
            usize::try_from(height)
                .ok()
                .and_then(|height| stride.checked_mul(height))
        })
        .ok_or_else(|| VideoError::UnsupportedFrameLayout("frame length overflow".into()))?;
    if byte_len < expected {
        return Err(VideoError::UnsupportedFrameLayout(
            "frame data is incomplete".into(),
        ));
    }
    Ok(())
}

pub(crate) fn tight_bgra_bytes(frame: &VideoFrame) -> Result<Vec<u8>, VideoError> {
    let row_len = usize::try_from(frame.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| VideoError::UnsupportedFrameLayout("row length overflow".into()))?;
    if frame.stride as usize == row_len {
        let tight_len = row_len
            .checked_mul(frame.height as usize)
            .ok_or_else(|| VideoError::UnsupportedFrameLayout("frame length overflow".into()))?;
        return Ok(frame.data[..tight_len].to_vec());
    }
    compact_bgra_rows(
        &frame.data,
        frame.stride as usize,
        row_len,
        frame.height as usize,
    )
}

fn compact_bgra_rows(
    data: &[u8],
    stride: usize,
    row_len: usize,
    height: usize,
) -> Result<Vec<u8>, VideoError> {
    let tight_len = row_len
        .checked_mul(height)
        .ok_or_else(|| VideoError::UnsupportedFrameLayout("frame length overflow".into()))?;
    let mut output = Vec::with_capacity(tight_len);
    for row in 0..height {
        let start = row
            .checked_mul(stride)
            .ok_or_else(|| VideoError::UnsupportedFrameLayout("row offset overflow".into()))?;
        let end = start
            .checked_add(row_len)
            .ok_or_else(|| VideoError::UnsupportedFrameLayout("row end overflow".into()))?;
        let row = data
            .get(start..end)
            .ok_or_else(|| VideoError::UnsupportedFrameLayout("frame data is incomplete".into()))?;
        output.extend_from_slice(row);
    }
    Ok(output)
}

pub(crate) fn fit_dimensions(
    source_width: u32,
    source_height: u32,
    max_width: u32,
    max_height: u32,
) -> VideoDimensions {
    let scale = (f64::from(max_width) / f64::from(source_width))
        .min(f64::from(max_height) / f64::from(source_height))
        .min(1.0);
    VideoDimensions {
        width: ((f64::from(source_width) * scale) as u32).max(2) & !1,
        height: ((f64::from(source_height) * scale) as u32).max(2) & !1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_rejects_short_rows_and_payloads() {
        assert!(VideoFrame::bgra(2, 2, 4, 0, vec![0; 16]).is_err());
        assert!(VideoFrame::bgra(2, 2, 8, 0, vec![0; 15]).is_err());
        assert!(VideoFrame::bgra(0, 2, 8, 0, vec![0; 16]).is_err());
    }

    #[test]
    fn padded_rows_are_compacted_without_padding_bytes() {
        let frame = VideoFrame::bgra(
            1,
            2,
            8,
            0,
            vec![1, 2, 3, 4, 99, 99, 99, 99, 5, 6, 7, 8, 88, 88, 88, 88],
        )
        .unwrap();
        assert_eq!(tight_bgra_bytes(&frame).unwrap(), [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn owned_tight_frame_reuses_the_decoder_allocation() {
        let frame = VideoFrame::bgra(2, 2, 8, 0, vec![7; 16]).unwrap();
        let allocation = frame.bytes().as_ptr();

        let bytes = frame.into_tight_bgra_bytes().unwrap();

        assert_eq!(bytes.as_ptr(), allocation);
        assert_eq!(bytes, vec![7; 16]);
    }

    #[test]
    fn owned_padded_frame_compacts_rows() {
        let frame = VideoFrame::bgra(
            1,
            2,
            8,
            0,
            vec![1, 2, 3, 4, 99, 99, 99, 99, 5, 6, 7, 8, 88, 88, 88, 88],
        )
        .unwrap();

        assert_eq!(
            frame.into_tight_bgra_bytes().unwrap(),
            [1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[test]
    fn fitted_dimensions_preserve_aspect_and_are_even() {
        assert_eq!(
            fit_dimensions(1920, 1080, 1280, 720),
            VideoDimensions {
                width: 1280,
                height: 720,
            }
        );
        let dimensions = fit_dimensions(1001, 777, 600, 600);
        assert_eq!(dimensions.width % 2, 0);
        assert_eq!(dimensions.height % 2, 0);
        assert!(dimensions.width <= 600 && dimensions.height <= 600);
    }

    #[test]
    fn config_validation_rejects_empty_source_before_process_start() {
        let error = VideoSessionConfig::default().validate().unwrap_err();
        assert!(matches!(error, VideoError::InvalidInput(_)));
    }
}
