pub const DEFAULT_DOCUMENT_PAGE_WIDTH_PX: f32 = 1296.0;
pub const DEFAULT_DOCUMENT_CONTENT_WIDTH_PX: f32 = DEFAULT_DOCUMENT_PAGE_WIDTH_PX;
pub const DEFAULT_DOCUMENT_MIN_HEIGHT_PX: f32 = 640.0;
pub const DEFAULT_DOCUMENT_TOP_INSET_PX: f32 = 96.0;
pub const COMPACT_DOCUMENT_TOP_INSET_PX: f32 = 48.0;
pub const COMPACT_DOCUMENT_VIEWPORT_MAX_WIDTH_PX: f32 = 720.0;
pub const PAGE_COVER_HEIGHT_PX: f32 = 220.0;
pub const PAGE_COVER_DOCUMENT_TOP_INSET_PX: f32 = 300.0;
pub const PAGE_ICON_DOCUMENT_TOP_INSET_PX: f32 = 168.0;

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
        let top_inset_px = if viewport_width_px <= COMPACT_DOCUMENT_VIEWPORT_MAX_WIDTH_PX {
            COMPACT_DOCUMENT_TOP_INSET_PX
        } else {
            DEFAULT_DOCUMENT_TOP_INSET_PX
        };
        Self {
            page_width_px,
            content_width_px: page_width_px,
            top_inset_px,
            ..Self::DEFAULT
        }
    }

    pub const fn with_additional_top_inset_px(mut self, additional_top_inset_px: f32) -> Self {
        self.top_inset_px += additional_top_inset_px;
        self
    }

    pub const fn with_page_decorations(mut self, has_cover: bool, has_icon: bool) -> Self {
        self.top_inset_px = if has_cover {
            PAGE_COVER_DOCUMENT_TOP_INSET_PX
        } else if has_icon {
            PAGE_ICON_DOCUMENT_TOP_INSET_PX
        } else {
            self.top_inset_px
        };
        self
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
        assert_eq!(metrics.top_inset_px, 96.0);

        let with_notice = metrics.with_additional_top_inset_px(32.0);
        assert_eq!(with_notice.top_inset_px, 128.0);
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

    #[test]
    fn document_header_space_is_compact_only_for_narrow_viewports() {
        assert_eq!(
            DocumentLayoutMetrics::for_viewport(1_200.0).top_inset_px,
            DEFAULT_DOCUMENT_TOP_INSET_PX
        );
        assert_eq!(
            DocumentLayoutMetrics::for_viewport(COMPACT_DOCUMENT_VIEWPORT_MAX_WIDTH_PX)
                .top_inset_px,
            COMPACT_DOCUMENT_TOP_INSET_PX
        );
        assert_eq!(
            DocumentLayoutMetrics::for_viewport(COMPACT_DOCUMENT_VIEWPORT_MAX_WIDTH_PX + 1.0)
                .top_inset_px,
            DEFAULT_DOCUMENT_TOP_INSET_PX
        );
    }

    #[test]
    fn page_decorations_expand_chrome_without_changing_document_width() {
        let base = DocumentLayoutMetrics::for_viewport(1_200.0);
        let icon = base.with_page_decorations(false, true);
        let cover = base.with_page_decorations(true, false);
        let cover_and_icon = base.with_page_decorations(true, true);

        assert_eq!(icon.top_inset_px, PAGE_ICON_DOCUMENT_TOP_INSET_PX);
        assert_eq!(cover.top_inset_px, PAGE_COVER_DOCUMENT_TOP_INSET_PX);
        assert_eq!(cover_and_icon.top_inset_px, cover.top_inset_px);
        assert_eq!(cover.page_width_px, base.page_width_px);
    }
}
