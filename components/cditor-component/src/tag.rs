//! Tag/chip component ported from the Liora tag design contract.
//!
//! `Tag` renders a compact, theme-agnostic pill built from an explicit
//! `TagStyle`. `TagFlow` lays out a collection of tags with wrapping,
//! alignment, and optional overflow collapsing.

use gpui::{
    AnyElement, App, Component, ElementId, Hsla, InteractiveElement, IntoElement, Pixels,
    RenderOnce, SharedString, Styled, Window, div, prelude::*, px,
};

use crate::SvgIcon;

const CLOSE_ICON: &[u8] = include_bytes!("../../../assets/icons/circle-x.svg");

/// Options that control tag size behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagSize {
    /// Uses compact sizing metrics.
    Small,
    /// Uses the default neutral treatment.
    #[default]
    Default,
    /// Uses expanded sizing metrics.
    Large,
}

/// Options that control semantic tag color behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagType {
    /// Uses the informational semantic color.
    #[default]
    Info,
    /// Uses the success semantic color.
    Success,
    /// Uses the warning semantic color.
    Warning,
    /// Uses the danger semantic color.
    Danger,
}

/// Options that control tag color treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagEffect {
    /// Uses a translucent tint with the semantic color for text.
    #[default]
    Light,
    /// Uses a solid semantic color with inverted text.
    Dark,
    /// Uses a neutral surface with the semantic color for text and border.
    Plain,
}

/// Semantic color tokens used by the liora-style tag color system.
#[derive(Debug, Clone, Copy)]
pub struct TagPalette {
    pub info: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
}

impl Default for TagPalette {
    fn default() -> Self {
        Self {
            info: Hsla {
                h: 0.59,
                s: 0.78,
                l: 0.51,
                a: 1.0,
            },
            success: Hsla {
                h: 0.36,
                s: 0.72,
                l: 0.44,
                a: 1.0,
            },
            warning: Hsla {
                h: 0.1,
                s: 0.95,
                l: 0.56,
                a: 1.0,
            },
            danger: Hsla {
                h: 0.0,
                s: 0.8,
                l: 0.58,
                a: 1.0,
            },
        }
    }
}

/// Explicit color metrics for a tag chip.
#[derive(Debug, Clone, Copy)]
pub struct TagStyle {
    pub background: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub close_icon: Hsla,
    pub close_hover_background: Hsla,
    pub radius: Pixels,
}

impl Default for TagStyle {
    fn default() -> Self {
        Self {
            background: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.94,
                a: 1.0,
            },
            border: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.86,
                a: 1.0,
            },
            text: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.22,
                a: 1.0,
            },
            close_icon: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.45,
                a: 1.0,
            },
            close_hover_background: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.84,
                a: 1.0,
            },
            radius: px(4.0),
        }
    }
}

impl TagStyle {
    /// Builds a liora-style style from a semantic color and effect.
    pub fn semantic(color: Hsla, effect: TagEffect) -> Self {
        let white = Hsla {
            h: 0.0,
            s: 0.0,
            l: 1.0,
            a: 1.0,
        };
        let neutral = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.97,
            a: 1.0,
        };
        let (background, border, text) = match effect {
            TagEffect::Light => (color.opacity(0.1), color.opacity(0.2), color),
            TagEffect::Dark => (color, color, white),
            TagEffect::Plain => (neutral, color.opacity(0.4), color),
        };
        Self {
            background,
            border,
            text,
            close_icon: text,
            close_hover_background: text.opacity(0.2),
            radius: px(4.0),
        }
    }
}

/// Fluent GPUI component for rendering a tag/chip.
pub struct Tag {
    label: SharedString,
    style: Option<TagStyle>,
    tag_type: TagType,
    effect: TagEffect,
    palette: TagPalette,
    size: TagSize,
    round: bool,
    closable: bool,
    on_close: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
}

impl Tag {
    /// Creates a tag from the supplied label.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            style: None,
            tag_type: TagType::Info,
            effect: TagEffect::Light,
            palette: TagPalette::default(),
            size: TagSize::Default,
            round: false,
            closable: false,
            on_close: None,
        }
    }

    /// Applies explicit color and radius metrics.
    pub fn style(mut self, style: TagStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Sets the semantic tag type.
    pub fn tag_type(mut self, tag_type: TagType) -> Self {
        self.tag_type = tag_type;
        self
    }

    /// Applies the informational semantic variant.
    pub fn info(self) -> Self {
        self.tag_type(TagType::Info)
    }

    /// Applies the success semantic variant.
    pub fn success(self) -> Self {
        self.tag_type(TagType::Success)
    }

    /// Applies the warning semantic variant.
    pub fn warning(self) -> Self {
        self.tag_type(TagType::Warning)
    }

    /// Applies the danger semantic variant.
    pub fn danger(self) -> Self {
        self.tag_type(TagType::Danger)
    }

    /// Sets the color treatment used by the component.
    pub fn effect(mut self, effect: TagEffect) -> Self {
        self.effect = effect;
        self
    }

    /// Applies the solid dark color treatment.
    pub fn dark(self) -> Self {
        self.effect(TagEffect::Dark)
    }

    /// Applies the plain color treatment.
    pub fn plain(self) -> Self {
        self.effect(TagEffect::Plain)
    }

    /// Applies a custom semantic color palette.
    pub fn palette(mut self, palette: TagPalette) -> Self {
        self.palette = palette;
        self
    }

    /// Sets the size used by the component.
    pub fn size(mut self, size: TagSize) -> Self {
        self.size = size;
        self
    }

    /// Uses the compact size preset.
    pub fn small(mut self) -> Self {
        self.size = TagSize::Small;
        self
    }

    /// Uses the large size preset.
    pub fn large(mut self) -> Self {
        self.size = TagSize::Large;
        self
    }

    /// Toggles round behavior.
    pub fn round(mut self, round: bool) -> Self {
        self.round = round;
        self
    }

    /// Shows the close affordance.
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    /// Registers a callback that runs when close occurs.
    pub fn on_close(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Box::new(f));
        self
    }
}

impl IntoElement for Tag {
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl RenderOnce for Tag {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let color = match self.tag_type {
            TagType::Info => self.palette.info,
            TagType::Success => self.palette.success,
            TagType::Warning => self.palette.warning,
            TagType::Danger => self.palette.danger,
        };
        let style = self
            .style
            .unwrap_or_else(|| TagStyle::semantic(color, self.effect));
        let on_close = self.on_close;
        let (padding_x, height, text_size) = match self.size {
            TagSize::Small => (px(8.0), px(20.0), px(11.0)),
            TagSize::Default => (px(10.0), px(24.0), px(12.0)),
            TagSize::Large => (px(12.0), px(32.0), px(14.0)),
        };
        let radius = if self.round {
            height / 2.0
        } else {
            style.radius
        };
        let close_id = ElementId::from(format!("cditor-tag-close-{}", self.label));

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(height)
            .px(padding_x)
            .bg(style.background)
            .border_1()
            .border_color(style.border)
            .rounded(radius)
            .text_size(text_size)
            .text_color(style.text)
            .child(div().child(self.label.clone()))
            .when(self.closable, |tag| {
                tag.child(
                    div()
                        .id(close_id)
                        .ml_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(
                            SvgIcon::new("cditor-tag-close-icon", CLOSE_ICON)
                                .size(px(10.0))
                                .color(style.close_icon),
                        )
                        .hover(move |hover| hover.bg(style.close_hover_background).rounded(px(2.0)))
                        .on_click(move |_event, window, cx| {
                            if let Some(ref f) = on_close {
                                f(window, cx);
                            }
                        }),
                )
            })
    }
}

/// Options that control tag flow alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagFlowAlign {
    /// Aligns content using the start position.
    #[default]
    Start,
    /// Aligns content using the center position.
    Center,
    /// Aligns content using the end position.
    End,
}

/// Fluent GPUI component for rendering a flow of tags.
pub struct TagFlow {
    tags: Vec<AnyElement>,
    gap: Pixels,
    align: TagFlowAlign,
    max_rows: Option<usize>,
    estimated_items_per_row: usize,
    collapsed: bool,
    overflow_indicator: Option<SharedString>,
}

impl TagFlow {
    /// Creates a tag flow from `Tag` values.
    pub fn new(tags: impl IntoIterator<Item = Tag>) -> Self {
        Self {
            tags: tags.into_iter().map(|tag| tag.into_any_element()).collect(),
            gap: px(8.0),
            align: TagFlowAlign::Start,
            max_rows: None,
            estimated_items_per_row: 4,
            collapsed: false,
            overflow_indicator: None,
        }
    }

    /// Creates a tag flow from arbitrary elements.
    pub fn from_elements(tags: impl IntoIterator<Item = impl IntoElement>) -> Self {
        Self {
            tags: tags.into_iter().map(|tag| tag.into_any_element()).collect(),
            gap: px(8.0),
            align: TagFlowAlign::Start,
            max_rows: None,
            estimated_items_per_row: 4,
            collapsed: false,
            overflow_indicator: None,
        }
    }

    /// Sets the spacing between child elements.
    pub fn gap(mut self, gap: impl Into<Pixels>) -> Self {
        self.gap = gap.into().max(px(0.0));
        self
    }

    /// Sets cross-axis alignment for child content.
    pub fn align(mut self, align: TagFlowAlign) -> Self {
        self.align = align;
        self
    }

    /// Centers content on both layout axes.
    pub fn center(self) -> Self {
        self.align(TagFlowAlign::Center)
    }

    /// Aligns content to the end.
    pub fn end(self) -> Self {
        self.align(TagFlowAlign::End)
    }

    /// Sets the maximum row count and collapses overflow.
    pub fn max_rows(mut self, rows: usize) -> Self {
        self.max_rows = Some(rows.max(1));
        self.collapsed = true;
        self
    }

    /// Sets the estimated items per row used for overflow collapse.
    pub fn estimated_items_per_row(mut self, count: usize) -> Self {
        self.estimated_items_per_row = count.max(1);
        self
    }

    /// Sets the collapsed flag explicitly.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Disables overflow collapsing.
    pub fn expanded(self) -> Self {
        self.collapsed(false)
    }

    /// Sets the overflow indicator label.
    pub fn overflow_indicator(mut self, label: impl Into<SharedString>) -> Self {
        self.overflow_indicator = Some(label.into());
        self
    }

    fn visible_count(&self) -> usize {
        if !self.collapsed {
            return self.tags.len();
        }
        self.max_rows
            .map(|rows| rows.saturating_mul(self.estimated_items_per_row))
            .unwrap_or(self.tags.len())
            .min(self.tags.len())
    }
}

impl IntoElement for TagFlow {
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl RenderOnce for TagFlow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let visible_count = self.visible_count();
        let hidden_count = self.tags.len().saturating_sub(visible_count);
        let overflow_label = self
            .overflow_indicator
            .clone()
            .unwrap_or_else(|| format!("+{hidden_count}").into());
        let tags = self.tags.into_iter().take(visible_count).chain(
            (hidden_count > 0).then(|| Tag::new(overflow_label).round(true).into_any_element()),
        );

        div()
            .flex()
            .flex_wrap()
            .gap(self.gap)
            .when(self.align == TagFlowAlign::Center, |flow| {
                flow.justify_center()
            })
            .when(self.align == TagFlowAlign::End, |flow| flow.justify_end())
            .children(tags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_flow_tracks_gap_and_alignment() {
        let flow = TagFlow::new([Tag::new("A"), Tag::new("B")])
            .gap(px(12.0))
            .center();

        assert_eq!(flow.gap, px(12.0));
        assert_eq!(flow.align, TagFlowAlign::Center);
        assert_eq!(flow.tags.len(), 2);
    }

    #[test]
    fn tag_flow_tracks_collapse_options() {
        let flow = TagFlow::new([
            Tag::new("A"),
            Tag::new("B"),
            Tag::new("C"),
            Tag::new("D"),
            Tag::new("E"),
        ])
        .max_rows(2)
        .estimated_items_per_row(2)
        .overflow_indicator("more");

        assert_eq!(flow.visible_count(), 4);
        assert_eq!(flow.max_rows, Some(2));
        assert_eq!(flow.estimated_items_per_row, 2);
        assert!(flow.collapsed);
    }

    #[test]
    fn tag_sizes_override_the_default() {
        assert_eq!(Tag::new("A").small().size, TagSize::Small);
        assert_eq!(Tag::new("A").large().size, TagSize::Large);
        assert_eq!(Tag::new("A").size, TagSize::Default);
    }

    #[test]
    fn tag_semantic_methods_select_types_and_effects() {
        assert_eq!(Tag::new("A").tag_type, TagType::Info);
        assert_eq!(Tag::new("A").effect, TagEffect::Light);

        assert_eq!(Tag::new("A").success().tag_type, TagType::Success);
        assert_eq!(Tag::new("A").warning().tag_type, TagType::Warning);
        assert_eq!(Tag::new("A").danger().tag_type, TagType::Danger);
        assert_eq!(Tag::new("A").dark().effect, TagEffect::Dark);
        assert_eq!(Tag::new("A").plain().effect, TagEffect::Plain);
    }

    #[test]
    fn semantic_style_matches_liora_light_dark_and_plain_treatments() {
        let color = Hsla {
            h: 0.0,
            s: 0.8,
            l: 0.58,
            a: 1.0,
        };
        let light = TagStyle::semantic(color, TagEffect::Light);
        assert_eq!(light.background.a, 0.1);
        assert_eq!(light.border.a, 0.2);
        assert_eq!(light.text, color);

        let dark = TagStyle::semantic(color, TagEffect::Dark);
        assert_eq!(dark.background, color);
        assert_eq!(dark.text.l, 1.0);

        let plain = TagStyle::semantic(color, TagEffect::Plain);
        assert_eq!(plain.background.l, 0.97);
        assert_eq!(plain.text, color);
    }
}
