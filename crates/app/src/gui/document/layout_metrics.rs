pub const DEFAULT_DOCUMENT_PAGE_WIDTH_PX: f32 = 860.0;
pub const DEFAULT_DOCUMENT_CONTENT_WIDTH_PX: f32 = DEFAULT_DOCUMENT_PAGE_WIDTH_PX;
pub const DEFAULT_DOCUMENT_MIN_HEIGHT_PX: f32 = 640.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentLayoutMetrics {
    pub page_width_px: f32,
    pub content_width_px: f32,
    pub min_height_px: f32,
}

impl DocumentLayoutMetrics {
    pub const DEFAULT: Self = Self {
        page_width_px: DEFAULT_DOCUMENT_PAGE_WIDTH_PX,
        content_width_px: DEFAULT_DOCUMENT_CONTENT_WIDTH_PX,
        min_height_px: DEFAULT_DOCUMENT_MIN_HEIGHT_PX,
    };

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

        assert_eq!(metrics.page_width_px, 860.0);
        assert_eq!(metrics.content_width_px, 860.0);
        assert_eq!(metrics.content_inset_x_px(), 0.0);
        assert_eq!(metrics.min_height_px, 640.0);
    }
}
