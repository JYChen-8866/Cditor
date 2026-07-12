use gpui::{
    AnyElement, DefiniteLength, IntoElement, ParentElement, Styled, div, prelude::FluentBuilder,
    px, rgb,
};

use crate::gui::GuiTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkeletonVariant {
    #[default]
    Text,
    Heading,
    Circle,
    Square,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonItem {
    pub variant: SkeletonVariant,
    pub width: Option<DefiniteLength>,
    pub height_px: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonRows {
    pub rows: usize,
    pub width: DefiniteLength,
    pub last_width: DefiniteLength,
    pub row_height_px: f32,
    pub gap_px: f32,
}

impl SkeletonItem {
    pub fn new(variant: SkeletonVariant) -> Self {
        Self {
            variant,
            width: None,
            height_px: None,
        }
    }

    pub fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }

    pub fn height_px(mut self, height_px: f32) -> Self {
        self.height_px = Some(height_px.max(1.0));
        self
    }

    pub fn render(self, theme: GuiTheme) -> AnyElement {
        let height = self.height_px.unwrap_or_else(|| match self.variant {
            SkeletonVariant::Text => 14.0,
            SkeletonVariant::Heading => 22.0,
            SkeletonVariant::Circle => 20.0,
            SkeletonVariant::Square => 40.0,
            SkeletonVariant::Image => 180.0,
        });
        let radius = match self.variant {
            SkeletonVariant::Circle => height / 2.0,
            SkeletonVariant::Image | SkeletonVariant::Square => 8.0,
            SkeletonVariant::Heading | SkeletonVariant::Text => 4.0,
        };

        div()
            .h(px(height))
            .bg(rgb(skeleton_background(theme)))
            .rounded(px(radius))
            .when_some(self.width, |this, width| this.w(width))
            .when(self.width.is_none(), |this| this.w_full())
            .into_any_element()
    }
}

fn skeleton_background(theme: GuiTheme) -> u32 {
    theme.skeleton
}

impl SkeletonRows {
    pub fn new(rows: usize) -> Self {
        Self {
            rows: rows.max(1),
            width: gpui::relative(1.0),
            last_width: gpui::relative(0.62),
            row_height_px: 14.0,
            gap_px: 8.0,
        }
    }

    pub fn row_height_px(mut self, row_height_px: f32) -> Self {
        self.row_height_px = row_height_px.max(1.0);
        self
    }

    pub fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = width.into();
        self
    }

    pub fn last_width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.last_width = width.into();
        self
    }

    pub fn gap_px(mut self, gap_px: f32) -> Self {
        self.gap_px = gap_px.max(0.0);
        self
    }

    pub fn render(self, theme: GuiTheme) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(self.gap_px))
            .w_full()
            .children((0..self.rows).map(|index| {
                let width = if index + 1 == self.rows {
                    self.last_width
                } else {
                    self.width
                };
                SkeletonItem::new(SkeletonVariant::Text)
                    .height_px(self.row_height_px)
                    .width(width)
                    .render(theme)
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_rows_clamp_to_at_least_one_row() {
        assert_eq!(SkeletonRows::new(0).rows, 1);
    }

    #[test]
    fn skeleton_item_preserves_variant_and_size_options() {
        let item = SkeletonItem::new(SkeletonVariant::Image)
            .height_px(120.0)
            .width(gpui::relative(0.5));

        assert_eq!(item.variant, SkeletonVariant::Image);
        assert_eq!(item.height_px, Some(120.0));
        assert!(item.width.is_some());
    }

    #[test]
    fn skeleton_background_uses_theme_token() {
        let theme = GuiTheme::light();

        assert_eq!(skeleton_background(theme), theme.skeleton);
        assert_ne!(skeleton_background(theme), theme.surface);
    }
}
