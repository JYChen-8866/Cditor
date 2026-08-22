use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame, RgbaImage};
use smallvec::SmallVec;

use crate::{
    VideoError,
    types::{VideoFrame, VideoPixelFormat, tight_bgra_bytes},
};

pub fn render_image_from_frame(frame: &VideoFrame) -> Result<Arc<RenderImage>, VideoError> {
    if frame.pixel_format != VideoPixelFormat::Bgra {
        return Err(VideoError::UnsupportedFrameLayout(
            "only BGRA is supported".into(),
        ));
    }
    let bytes = tight_bgra_bytes(frame)?;
    let image = RgbaImage::from_raw(frame.width, frame.height, bytes)
        .ok_or_else(|| VideoError::RenderImage("invalid image dimensions".into()))?;
    let mut frames = SmallVec::<[Frame; 1]>::new();
    frames.push(Frame::new(image));
    Ok(Arc::new(RenderImage::new(frames)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_image_preserves_frame_dimensions_and_bytes() {
        let frame = VideoFrame::bgra(1, 1, 4, 0, vec![3, 2, 1, 255]).unwrap();
        let image = render_image_from_frame(&frame).unwrap();
        assert_eq!(image.size(0).width.0, 1);
        assert_eq!(image.size(0).height.0, 1);
        assert_eq!(image.as_bytes(0), Some([3, 2, 1, 255].as_slice()));
    }
}
