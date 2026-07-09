use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, prelude::FluentBuilder, px, rgb};

use crate::gui::GuiTheme;
use crate::gui::document::skeleton_window::render_document_skeleton_window;

pub const DEFAULT_DOCUMENT_PAGE_WIDTH_PX: f32 = 860.0;
pub const DEFAULT_DOCUMENT_MIN_HEIGHT_PX: f32 = 640.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentSurface {
    pub page_width_px: f32,
    pub min_height_px: f32,
    pub before_window_height: f64,
    pub placeholder_window_height: Option<f64>,
    pub after_window_height: f64,
    pub scroll_top: f64,
}

impl DocumentSurface {
    pub fn new(before_window_height: f64, after_window_height: f64) -> Self {
        Self::with_placeholder(before_window_height, None, after_window_height)
    }

    pub fn with_placeholder(
        before_window_height: f64,
        placeholder_window_height: Option<f64>,
        after_window_height: f64,
    ) -> Self {
        Self::with_scroll(
            before_window_height,
            placeholder_window_height,
            after_window_height,
            0.0,
        )
    }

    pub fn with_scroll(
        before_window_height: f64,
        placeholder_window_height: Option<f64>,
        after_window_height: f64,
        scroll_top: f64,
    ) -> Self {
        Self {
            page_width_px: DEFAULT_DOCUMENT_PAGE_WIDTH_PX,
            min_height_px: DEFAULT_DOCUMENT_MIN_HEIGHT_PX,
            before_window_height,
            placeholder_window_height,
            after_window_height,
            scroll_top,
        }
    }

    pub fn render(
        self,
        theme: GuiTheme,
        block_elements: Vec<AnyElement>,
        overlay: Option<AnyElement>,
    ) -> AnyElement {
        div()
            .flex_1()
            .overflow_hidden()
            .child(
                div()
                    .relative()
                    .mx_auto()
                    .my_6()
                    .p_6()
                    .w(px(self.page_width_px))
                    .h_full()
                    .min_h(px(self.min_height_px))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.page))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px((self.before_window_height - self.scroll_top) as f32))
                            .when_some(self.placeholder_window_height, |this, height| {
                                this.child(render_document_skeleton_window(height, theme))
                            })
                            .children(block_elements),
                    )
                    .when_some(overlay, |this, overlay| {
                        this.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .top(px(-(self.scroll_top as f32)))
                                .child(overlay),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_surface_uses_stable_default_page_metrics() {
        let surface = DocumentSurface::new(10.0, 20.0);

        assert_eq!(surface.page_width_px, DEFAULT_DOCUMENT_PAGE_WIDTH_PX);
        assert_eq!(surface.min_height_px, DEFAULT_DOCUMENT_MIN_HEIGHT_PX);
        assert_eq!(surface.before_window_height, 10.0);
        assert_eq!(surface.placeholder_window_height, None);
        assert_eq!(surface.after_window_height, 20.0);
        assert_eq!(surface.scroll_top, 0.0);

        let placeholder_surface = DocumentSurface::with_placeholder(10.0, Some(30.0), 20.0);
        assert_eq!(placeholder_surface.placeholder_window_height, Some(30.0));

        let scrolled_surface = DocumentSurface::with_scroll(10.0, None, 20.0, 128.0);
        assert_eq!(scrolled_surface.scroll_top, 128.0);
    }
}
