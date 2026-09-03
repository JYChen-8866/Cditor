use std::sync::Arc;

use gpui::RenderImage;

#[cfg(feature = "gpui-dynamic-image")]
use gpui::DynamicImage;

#[cfg(feature = "gpui-dynamic-image")]
pub(super) type VideoRenderImage = DynamicImage;
#[cfg(not(feature = "gpui-dynamic-image"))]
pub(super) type VideoRenderImage = RenderImage;

/// Owns exactly one current CPU frame. Hosts using Aurin's GPUI fork also get
/// one stable dynamic-image identity backed by completion-tracked GPU slots;
/// upstream GPUI builds use the immutable compatibility path.
#[derive(Default)]
pub(super) struct CachedVideoImage {
    presented_generation: u64,
    #[cfg(feature = "gpui-dynamic-image")]
    image: Option<Arc<DynamicImage>>,
    #[cfg(not(feature = "gpui-dynamic-image"))]
    image: Option<Arc<RenderImage>>,
}

impl CachedVideoImage {
    pub(super) fn presented_generation(&self) -> u64 {
        self.presented_generation
    }

    pub(super) fn is_some(&self) -> bool {
        self.image.is_some()
    }

    pub(super) fn stable_slot_capacity(&self) -> usize {
        #[cfg(feature = "gpui-dynamic-image")]
        {
            usize::from(self.image.is_some()).saturating_mul(2)
        }
        #[cfg(not(feature = "gpui-dynamic-image"))]
        {
            usize::from(self.image.is_some())
        }
    }
}

#[cfg(feature = "gpui-dynamic-image")]
impl CachedVideoImage {
    pub(super) fn current(&self) -> Option<Arc<DynamicImage>> {
        self.image.clone()
    }

    pub(super) fn replace(
        &mut self,
        presented_generation: u64,
        image: Arc<RenderImage>,
    ) -> Option<Arc<RenderImage>> {
        debug_assert!(presented_generation > self.presented_generation);
        self.presented_generation = presented_generation;
        match &self.image {
            Some(dynamic) => Some(dynamic.replace(image)),
            None => {
                self.image = Some(Arc::new(DynamicImage::new(image)));
                None
            }
        }
    }

    pub(super) fn take(&mut self) -> Option<RetiredVideoImage> {
        let dynamic = self.image.take()?;
        let fallback = dynamic.snapshot().image;
        Some(RetiredVideoImage { dynamic, fallback })
    }
}

#[cfg(not(feature = "gpui-dynamic-image"))]
impl CachedVideoImage {
    pub(super) fn current(&self) -> Option<Arc<RenderImage>> {
        self.image.clone()
    }

    pub(super) fn replace(
        &mut self,
        presented_generation: u64,
        image: Arc<RenderImage>,
    ) -> Option<Arc<RenderImage>> {
        debug_assert!(presented_generation > self.presented_generation);
        self.presented_generation = presented_generation;
        self.image.replace(image)
    }

    pub(super) fn take(&mut self) -> Option<RetiredVideoImage> {
        self.image.take().map(RetiredVideoImage)
    }
}

#[cfg(feature = "gpui-dynamic-image")]
pub(crate) struct RetiredVideoImage {
    dynamic: Arc<DynamicImage>,
    fallback: Arc<RenderImage>,
}

#[cfg(feature = "gpui-dynamic-image")]
impl RetiredVideoImage {
    pub(crate) fn into_parts(self) -> (Arc<DynamicImage>, Arc<RenderImage>) {
        (self.dynamic, self.fallback)
    }
}

#[cfg(not(feature = "gpui-dynamic-image"))]
pub(crate) struct RetiredVideoImage(Arc<RenderImage>);

#[cfg(not(feature = "gpui-dynamic-image"))]
impl RetiredVideoImage {
    pub(crate) fn into_parts(self) -> Arc<RenderImage> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(value: u8) -> Arc<RenderImage> {
        Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::from_pixel(1, 1, image::Rgba([value, 0, 0, 255])),
        )]))
    }

    #[test]
    fn replacement_keeps_one_current_cpu_frame() {
        let first = test_image(1);
        let second = test_image(2);
        let mut cached = CachedVideoImage::default();
        assert!(cached.replace(1, first.clone()).is_none());
        let retired = cached.replace(2, second.clone()).expect("old frame");
        assert!(Arc::ptr_eq(&retired, &first));
        assert_eq!(cached.presented_generation(), 2);

        #[cfg(feature = "gpui-dynamic-image")]
        assert!(Arc::ptr_eq(
            &cached.current().unwrap().snapshot().image,
            &second
        ));
        #[cfg(not(feature = "gpui-dynamic-image"))]
        assert!(Arc::ptr_eq(&cached.current().unwrap(), &second));
    }

    #[cfg(not(feature = "gpui-dynamic-image"))]
    #[test]
    fn compatibility_eviction_returns_current_frame_without_copying() {
        let frame = test_image(3);
        let mut cached = CachedVideoImage::default();
        cached.replace(1, frame.clone());
        assert!(Arc::ptr_eq(&cached.take().unwrap().into_parts(), &frame));
        assert!(cached.current().is_none());
    }

    #[cfg(feature = "gpui-dynamic-image")]
    #[test]
    fn dynamic_eviction_returns_stable_identity_and_fallback() {
        let frame = test_image(3);
        let mut cached = CachedVideoImage::default();
        cached.replace(1, frame.clone());
        let dynamic = cached.current().unwrap();
        let (retired_dynamic, fallback) = cached.take().unwrap().into_parts();
        assert!(Arc::ptr_eq(&retired_dynamic, &dynamic));
        assert!(Arc::ptr_eq(&fallback, &frame));
    }
}
