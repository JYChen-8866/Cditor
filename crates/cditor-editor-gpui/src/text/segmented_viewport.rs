#[derive(Clone)]
pub(crate) struct SegmentedTextViewport {
    local_top_px: f32,
    height_px: f32,
}

impl SegmentedTextViewport {
    pub(crate) fn document(local_top_px: f32, height_px: f32) -> Self {
        Self {
            local_top_px: local_top_px.max(0.0),
            height_px: height_px.max(1.0),
        }
    }

    pub(crate) fn visible_range(&self) -> (f32, f32) {
        (self.local_top_px, self.height_px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_viewport_clamps_invalid_geometry() {
        let viewport = SegmentedTextViewport::document(-20.0, 0.0);
        assert_eq!(viewport.visible_range(), (0.0, 1.0));
    }
}
