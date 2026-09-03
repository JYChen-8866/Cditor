use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame, RgbaImage};
use smallvec::SmallVec;

use crate::{
    VideoError,
    types::{VideoFrame, VideoPixelFormat, tight_bgra_bytes, validate_frame_layout},
};

#[derive(Debug)]
pub struct OwnedFrameRenderError {
    error: VideoError,
    frame: Arc<VideoFrame>,
}

impl OwnedFrameRenderError {
    pub fn error(&self) -> &VideoError {
        &self.error
    }

    pub fn into_parts(self) -> (VideoError, Arc<VideoFrame>) {
        (self.error, self.frame)
    }
}

pub fn render_image_from_frame(frame: &VideoFrame) -> Result<Arc<RenderImage>, VideoError> {
    if frame.pixel_format != VideoPixelFormat::Bgra {
        return Err(VideoError::UnsupportedFrameLayout(
            "only BGRA is supported".into(),
        ));
    }
    render_image_from_bgra_bytes(frame.width, frame.height, tight_bgra_bytes(frame)?)
}

/// Converts a frame claimed from [`crate::VideoFrameStore`] into a GPUI image.
///
/// The normal playback path has unique ownership of the frame after the
/// single-slot mailbox is consumed, so its decoder allocation becomes the
/// `RenderImage` backing allocation. External diagnostic readers may still
/// hold an `Arc`; only that exceptional path falls back to one defensive copy.
pub fn render_image_from_owned_frame(
    frame: Arc<VideoFrame>,
) -> Result<Arc<RenderImage>, VideoError> {
    render_image_from_owned_frame_recoverable(frame).map_err(|error| error.error)
}

/// Like [`render_image_from_owned_frame`], but returns the claimed frame with
/// conversion errors so a [`VideoFrameLease`](crate::VideoFrameLease) can put
/// it back into the mailbox. Validation is completed before unique ownership
/// is consumed; a valid decoder frame therefore has no fallible operation
/// after its pixel allocation is moved into the `RenderImage`.
pub fn render_image_from_owned_frame_recoverable(
    frame: Arc<VideoFrame>,
) -> Result<Arc<RenderImage>, OwnedFrameRenderError> {
    if frame.pixel_format != VideoPixelFormat::Bgra {
        return Err(OwnedFrameRenderError {
            error: VideoError::UnsupportedFrameLayout("only BGRA is supported".into()),
            frame,
        });
    }
    if let Err(error) =
        validate_frame_layout(frame.width, frame.height, frame.stride, frame.bytes().len())
    {
        return Err(OwnedFrameRenderError { error, frame });
    }
    let width = frame.width;
    let height = frame.height;
    let bytes = match Arc::try_unwrap(frame) {
        Ok(frame) => frame
            .into_tight_bgra_bytes()
            .expect("validated video frame must have a renderable layout"),
        Err(frame) => match tight_bgra_bytes(&frame) {
            Ok(bytes) => bytes,
            Err(error) => return Err(OwnedFrameRenderError { error, frame }),
        },
    };
    Ok(render_image_from_bgra_bytes_unchecked(width, height, bytes))
}

fn render_image_from_bgra_bytes(
    width: u32,
    height: u32,
    bytes: Vec<u8>,
) -> Result<Arc<RenderImage>, VideoError> {
    let image = RgbaImage::from_raw(width, height, bytes)
        .ok_or_else(|| VideoError::RenderImage("invalid image dimensions".into()))?;
    let mut frames = SmallVec::<[Frame; 1]>::new();
    frames.push(Frame::new(image));
    Ok(Arc::new(RenderImage::new(frames)))
}

fn render_image_from_bgra_bytes_unchecked(
    width: u32,
    height: u32,
    bytes: Vec<u8>,
) -> Arc<RenderImage> {
    let image = RgbaImage::from_raw(width, height, bytes)
        .expect("validated video dimensions must match the tight pixel buffer");
    let mut frames = SmallVec::<[Frame; 1]>::new();
    frames.push(Frame::new(image));
    Arc::new(RenderImage::new(frames))
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

    #[test]
    fn owned_unique_frame_moves_its_allocation_into_render_image() {
        let frame = Arc::new(VideoFrame::bgra(2, 2, 8, 0, vec![9; 16]).unwrap());
        let allocation = frame.bytes().as_ptr();

        let image = render_image_from_owned_frame(frame).unwrap();

        assert_eq!(image.as_bytes(0).unwrap().as_ptr(), allocation);
        assert_eq!(image.as_bytes(0), Some([9; 16].as_slice()));
    }

    #[test]
    fn mailbox_to_render_image_path_has_single_cpu_pixel_owner() {
        let store = crate::VideoFrameStore::default();
        let published = store.publish(VideoFrame::bgra(2, 2, 8, 0, vec![5; 16]).unwrap());
        let source_ptr = published.frame.bytes().as_ptr();
        drop(published);

        let claimed = store.take_latest_for_presentation().unwrap();
        let image = render_image_from_owned_frame(claimed.frame).unwrap();

        assert_eq!(image.as_bytes(0).unwrap().as_ptr(), source_ptr);
        assert_eq!(store.resident_bytes(), 0);
    }

    #[test]
    fn owned_shared_frame_falls_back_without_invalidating_readers() {
        let frame = Arc::new(VideoFrame::bgra(1, 1, 4, 0, vec![3, 2, 1, 255]).unwrap());
        let reader = Arc::clone(&frame);

        let image = render_image_from_owned_frame(frame).unwrap();

        assert_eq!(reader.bytes(), [3, 2, 1, 255]);
        assert_eq!(image.as_bytes(0), Some(reader.bytes()));
        assert_ne!(image.as_bytes(0).unwrap().as_ptr(), reader.bytes().as_ptr());
    }
}
