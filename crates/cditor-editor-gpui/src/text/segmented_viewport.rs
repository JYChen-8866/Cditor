use gpui::{ScrollHandle, px};

const VIEWPORT_FALLBACK_HEIGHT_PX: f32 = 320.0;

#[derive(Clone)]
pub(crate) enum SegmentedTextViewport {
    InternalScroll(ScrollHandle),
    Document { local_top_px: f32, height_px: f32 },
}

impl SegmentedTextViewport {
    pub(crate) fn internal_scroll(handle: ScrollHandle) -> Self {
        Self::InternalScroll(handle)
    }

    pub(crate) fn document(local_top_px: f32, height_px: f32) -> Self {
        Self::Document {
            local_top_px: local_top_px.max(0.0),
            height_px: height_px.max(1.0),
        }
    }

    pub(crate) fn visible_range(&self) -> (f32, f32) {
        match self {
            Self::InternalScroll(handle) => (
                (-f32::from(handle.offset().y)).max(0.0),
                f32::from(handle.bounds().size.height).max(VIEWPORT_FALLBACK_HEIGHT_PX),
            ),
            Self::Document {
                local_top_px,
                height_px,
            } => (*local_top_px, *height_px),
        }
    }

    pub(crate) fn restore_internal_top(&self, top_px: f32) {
        if let Self::InternalScroll(handle) = self {
            let current = handle.offset();
            handle.set_offset(gpui::point(current.x, px(-top_px.max(0.0))));
        }
    }

    pub(crate) fn internal_scroll_handle(&self) -> Option<&ScrollHandle> {
        match self {
            Self::InternalScroll(handle) => Some(handle),
            Self::Document { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_viewport_clamps_invalid_geometry() {
        let viewport = SegmentedTextViewport::document(-20.0, 0.0);
        assert_eq!(viewport.visible_range(), (0.0, 1.0));
        assert!(viewport.internal_scroll_handle().is_none());
    }
}
