pub const DEFAULT_DOCUMENT_PAGE_WIDTH_PX: f32 = 1296.0;
pub const DEFAULT_DOCUMENT_CONTENT_WIDTH_PX: f32 = DEFAULT_DOCUMENT_PAGE_WIDTH_PX;
pub const DEFAULT_DOCUMENT_MIN_HEIGHT_PX: f32 = 640.0;
pub const DEFAULT_DOCUMENT_TOP_INSET_PX: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentLayoutMetrics {
    pub page_width_px: f32,
    pub content_width_px: f32,
    pub min_height_px: f32,
    pub top_inset_px: f32,
}

impl DocumentLayoutMetrics {
    pub const DEFAULT: Self = Self {
        page_width_px: DEFAULT_DOCUMENT_PAGE_WIDTH_PX,
        content_width_px: DEFAULT_DOCUMENT_CONTENT_WIDTH_PX,
        min_height_px: DEFAULT_DOCUMENT_MIN_HEIGHT_PX,
        top_inset_px: DEFAULT_DOCUMENT_TOP_INSET_PX,
    };

    pub fn for_viewport(viewport_width_px: f32) -> Self {
        let page_width_px = viewport_width_px.clamp(1.0, DEFAULT_DOCUMENT_PAGE_WIDTH_PX);
        Self {
            page_width_px,
            content_width_px: page_width_px,
            ..Self::DEFAULT
        }
    }

    #[cfg(test)]
    pub const fn content_inset_x_px(self) -> f32 {
        (self.page_width_px - self.content_width_px) / 2.0
    }
}

impl Default for DocumentLayoutMetrics {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_document_layout_is_a_stable_frameless_content_column() {
        let metrics = DocumentLayoutMetrics::default();

        assert_eq!(metrics.page_width_px, 1296.0);
        assert_eq!(metrics.content_width_px, 1296.0);
        assert_eq!(metrics.content_inset_x_px(), 0.0);
        assert_eq!(metrics.min_height_px, 640.0);
        assert_eq!(metrics.top_inset_px, 32.0);
    }

    #[test]
    fn document_width_caps_at_full_track_plus_symmetric_safe_insets() {
        assert_eq!(
            DocumentLayoutMetrics::for_viewport(1440.0).page_width_px,
            1296.0
        );
        assert_eq!(
            DocumentLayoutMetrics::for_viewport(700.0).page_width_px,
            700.0
        );
    }
}
