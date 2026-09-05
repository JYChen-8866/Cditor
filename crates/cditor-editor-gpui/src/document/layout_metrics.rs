pub const DEFAULT_DOCUMENT_PAGE_WIDTH_PX: f32 = 1296.0;
pub const DEFAULT_DOCUMENT_CONTENT_WIDTH_PX: f32 = DEFAULT_DOCUMENT_PAGE_WIDTH_PX;
pub const DEFAULT_DOCUMENT_MIN_HEIGHT_PX: f32 = 640.0;
pub const DEFAULT_DOCUMENT_TOP_INSET_PX: f32 = 88.0;
pub const COMPACT_DOCUMENT_TOP_INSET_PX: f32 = 88.0;
pub const COMPACT_DOCUMENT_VIEWPORT_MAX_WIDTH_PX: f32 = 720.0;
/// Below this width the document's symmetric safe insets leave too little room
/// for a meaningful text layout. Split-pane resizing can briefly report these
/// widths even when the pane immediately expands again.
pub const MIN_DOCUMENT_LAYOUT_VIEWPORT_WIDTH_PX: f32 = 160.0;
pub const PAGE_COVER_HEIGHT_PX: f32 = 200.0;
pub const PAGE_COVER_DOCUMENT_TOP_INSET_PX: f32 = 248.0;
pub const PAGE_ICON_DOCUMENT_TOP_INSET_PX: f32 = 124.0;

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

    pub fn viewport_width_is_usable(viewport_width_px: f32) -> bool {
        viewport_width_px.is_finite() && viewport_width_px >= MIN_DOCUMENT_LAYOUT_VIEWPORT_WIDTH_PX
    }

    pub fn embedded_composer(viewport_width_px: f32) -> Self {
        let width = viewport_width_px.max(1.0);
        Self {
            page_width_px: width,
            content_width_px: width,
            min_height_px: 96.0,
            top_inset_px: 8.0,
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
        assert_eq!(metrics.top_inset_px, 88.0);

        let with_notice = metrics.with_additional_top_inset_px(32.0);
        assert_eq!(with_notice.top_inset_px, 120.0);
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
    fn transient_collapsed_pane_width_is_not_a_usable_document_viewport() {
        assert!(!DocumentLayoutMetrics::viewport_width_is_usable(0.0));
        assert!(!DocumentLayoutMetrics::viewport_width_is_usable(1.0));
        assert!(!DocumentLayoutMetrics::viewport_width_is_usable(159.0));
        assert!(!DocumentLayoutMetrics::viewport_width_is_usable(f32::NAN));
        assert!(DocumentLayoutMetrics::viewport_width_is_usable(160.0));
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

    #[test]
    fn embedded_composer_uses_the_viewport_without_page_chrome_spacing() {
        let metrics = DocumentLayoutMetrics::embedded_composer(522.0);

        assert_eq!(metrics.page_width_px, 522.0);
        assert_eq!(metrics.content_width_px, 522.0);
        assert_eq!(metrics.min_height_px, 96.0);
        assert_eq!(metrics.top_inset_px, 8.0);
    }
}
