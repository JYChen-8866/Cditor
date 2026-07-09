use crate::gui::GuiTheme;
use cditor_core::rich_text::RichBlockKind;
use cditor_runtime::ViewBlockSnapshot;

pub const BLOCK_INDENT_STEP_PX: f32 = 24.0;
pub const BLOCK_GUTTER_WIDTH_PX: f32 = 24.0;
pub const BLOCK_GUTTER_HEIGHT_PX: f32 = 22.0;
pub const BLOCK_PREFIX_WIDTH_PX: f32 = 38.0;
pub const CALLOUT_PREFIX_WIDTH_PX: f32 = 34.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockChromeStyle {
    pub indent_px: f32,
    pub gutter_width_px: f32,
    pub gutter_height_px: f32,
    pub prefix_width_px: f32,
    pub content_min_height_px: f32,
    pub content_padding_y_px: f32,
    pub content_padding_left_px: f32,
    pub content_padding_right_px: f32,
    pub content_radius_px: f32,
    pub outer_background: u32,
    pub content_background: u32,
    pub content_border: u32,
    pub text_color: u32,
    pub quote_bar: Option<u32>,
}

impl BlockChromeStyle {
    pub fn from_snapshot(block: &ViewBlockSnapshot, theme: GuiTheme) -> Self {
        let kind_style = KindChromeStyle::from_kind(&block.kind, theme);
        let outer_background = theme.surface;
        Self {
            indent_px: block.chrome.list_info.depth as f32 * BLOCK_INDENT_STEP_PX,
            gutter_width_px: BLOCK_GUTTER_WIDTH_PX,
            gutter_height_px: BLOCK_GUTTER_HEIGHT_PX,
            prefix_width_px: match block.chrome.prefix {
                cditor_core::block::BlockPrefixSnapshot::Callout { .. } => CALLOUT_PREFIX_WIDTH_PX,
                cditor_core::block::BlockPrefixSnapshot::None => 0.0,
                _ => BLOCK_PREFIX_WIDTH_PX,
            },
            content_min_height_px: kind_style.min_height_px,
            content_padding_y_px: kind_style.padding_y_px,
            content_padding_left_px: kind_style.padding_left_px,
            content_padding_right_px: kind_style.padding_right_px,
            content_radius_px: kind_style.radius_px,
            outer_background,
            content_background: kind_style.background,
            content_border: kind_style.border,
            text_color: kind_style.text,
            quote_bar: kind_style.quote_bar,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct KindChromeStyle {
    background: u32,
    border: u32,
    text: u32,
    padding_y_px: f32,
    padding_left_px: f32,
    padding_right_px: f32,
    min_height_px: f32,
    radius_px: f32,
    quote_bar: Option<u32>,
}

impl KindChromeStyle {
    fn from_kind(kind: &RichBlockKind, theme: GuiTheme) -> Self {
        match kind {
            RichBlockKind::Heading { level } => Self::heading(*level, theme),
            RichBlockKind::Quote => Self::quote(theme),
            RichBlockKind::Callout { .. } => Self::callout(theme),
            RichBlockKind::Code { .. } => Self::code(theme),
            _ => Self::paragraph(theme),
        }
    }

    fn paragraph(theme: GuiTheme) -> Self {
        Self {
            background: theme.surface,
            border: theme.surface,
            text: theme.text,
            padding_y_px: 4.0,
            padding_left_px: 0.0,
            padding_right_px: 0.0,
            min_height_px: 28.0,
            radius_px: 6.0,
            quote_bar: None,
        }
    }

    fn heading(level: u8, theme: GuiTheme) -> Self {
        let (padding_y_px, min_height_px) = match level {
            1 => (10.0, 48.0),
            2 => (8.0, 42.0),
            3 => (6.0, 36.0),
            _ => (4.0, 32.0),
        };
        Self {
            padding_y_px,
            min_height_px,
            ..Self::paragraph(theme)
        }
    }

    fn quote(theme: GuiTheme) -> Self {
        Self {
            background: theme.surface,
            border: theme.surface,
            text: theme.quote_text,
            padding_y_px: 4.0,
            padding_left_px: 8.0,
            padding_right_px: 0.0,
            min_height_px: 28.0,
            radius_px: 0.0,
            quote_bar: Some(theme.quote_bar),
        }
    }

    fn callout(theme: GuiTheme) -> Self {
        Self {
            background: theme.callout_background,
            border: theme.callout_border,
            text: theme.text,
            padding_y_px: 10.0,
            padding_left_px: 10.0,
            padding_right_px: 10.0,
            min_height_px: 44.0,
            radius_px: 8.0,
            quote_bar: None,
        }
    }

    fn code(theme: GuiTheme) -> Self {
        // CodeBlock 的可见背景、圆角、toolbar、content padding 都由 code component 自己 1:1 绘制。
        // 外层 shell 只作为 gutter/prefix 行容器，不能叠加 padding/bg，避免真实高度与 core 估算错位。
        Self {
            background: theme.surface,
            border: theme.surface,
            text: theme.text,
            padding_y_px: 0.0,
            padding_left_px: 0.0,
            padding_right_px: 0.0,
            min_height_px: 92.0,
            radius_px: 8.0,
            quote_bar: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::block::{BlockChromeSnapshot, BlockListInfo, BlockPrefixSnapshot};
    use cditor_core::rich_text::{BlockAttrs, BlockPayloadView};
    use cditor_runtime::ViewBlockSnapshot;

    fn block(kind: RichBlockKind, chrome: BlockChromeSnapshot) -> ViewBlockSnapshot {
        ViewBlockSnapshot {
            block_id: 1,
            visible_index: 0,
            depth: chrome.list_info.depth as u16,
            chrome,
            kind,
            attrs: BlockAttrs::default(),
            payload: BlockPayloadView::Placeholder {
                estimated_height: 32.0,
            },
            layout: cditor_core::layout::BlockLayoutMeta::new(1, 32.0),
            selected: false,
            selection_range: None,
            focused: false,
            caret_offset: None,
            marked_range: None,
            table_view: None,
            focused_table_cell: None,
            focused_table_cell_offset: None,
            pinned: false,
            placeholder: false,
        }
    }

    #[test]
    fn chrome_style_uses_list_depth_for_indent_and_prefix_width() {
        let snapshot = block(
            RichBlockKind::NumberedList,
            BlockChromeSnapshot {
                list_info: BlockListInfo::with_depth(2).with_numbered_ordinal(3),
                prefix: BlockPrefixSnapshot::Number { ordinal: 3 },
                has_children: false,
                collapsed: false,
            },
        );

        let style = BlockChromeStyle::from_snapshot(&snapshot, GuiTheme::light());

        assert_eq!(style.indent_px, 48.0);
        assert_eq!(style.gutter_width_px, 24.0);
        assert_eq!(style.prefix_width_px, 38.0);
    }

    #[test]
    fn gui_chrome_padding_and_min_height_match_core_height_metrics() {
        for kind in [
            RichBlockKind::Paragraph,
            RichBlockKind::BulletedList,
            RichBlockKind::Todo { checked: false },
            RichBlockKind::Quote,
            RichBlockKind::Callout {
                variant: cditor_core::rich_text::CalloutVariant::Note,
            },
            RichBlockKind::Code { language: None },
        ] {
            let snapshot = block(kind.clone(), BlockChromeSnapshot::plain());
            let style = BlockChromeStyle::from_snapshot(&snapshot, GuiTheme::light());
            let metrics =
                cditor_core::layout::block_metrics::text_block_chrome_metrics_for_kind(&kind);

            assert_eq!(
                style.content_min_height_px as f64,
                metrics.content_min_height
            );
            assert_eq!(style.content_padding_y_px as f64, metrics.content_padding_y);
        }
    }

    #[test]
    fn v1_block_visual_colors_match_editor2_theme() {
        let theme = GuiTheme::light();
        let paragraph = BlockChromeStyle::from_snapshot(
            &block(RichBlockKind::Paragraph, BlockChromeSnapshot::plain()),
            theme,
        );
        assert_eq!(paragraph.outer_background, theme.surface);
        assert_eq!(paragraph.content_background, theme.surface);
        assert_eq!(paragraph.content_border, theme.surface);
        assert_eq!(paragraph.text_color, theme.text);

        for level in [1, 2, 3, 4] {
            let heading = BlockChromeStyle::from_snapshot(
                &block(
                    RichBlockKind::Heading { level },
                    BlockChromeSnapshot::plain(),
                ),
                theme,
            );
            assert_eq!(heading.content_background, theme.surface);
            assert_eq!(heading.content_border, theme.surface);
            assert_eq!(heading.text_color, theme.text);
        }

        let quote = BlockChromeStyle::from_snapshot(
            &block(RichBlockKind::Quote, BlockChromeSnapshot::plain()),
            theme,
        );
        assert_eq!(quote.content_background, theme.surface);
        assert_eq!(quote.content_border, theme.surface);
        assert_eq!(quote.text_color, theme.quote_text);
        assert_eq!(quote.quote_bar, Some(theme.quote_bar));

        let callout = BlockChromeStyle::from_snapshot(
            &block(
                RichBlockKind::Callout {
                    variant: cditor_core::rich_text::CalloutVariant::Note,
                },
                BlockChromeSnapshot::plain(),
            ),
            theme,
        );
        assert_eq!(callout.content_background, theme.callout_background);
        assert_eq!(callout.content_border, theme.callout_border);
        assert_eq!(callout.text_color, theme.text);

        let code = BlockChromeStyle::from_snapshot(
            &block(
                RichBlockKind::Code { language: None },
                BlockChromeSnapshot::plain(),
            ),
            theme,
        );
        // CodeBlock 外层只承载 gutter/prefix 行；code component 内部自绘 V1 code bg/text。
        assert_eq!(code.content_background, theme.surface);
        assert_eq!(code.content_border, theme.surface);
        assert_eq!(code.text_color, theme.text);
        assert_eq!(code.content_padding_y_px, 0.0);
        assert_eq!(code.content_min_height_px, 92.0);
    }

    #[test]
    fn quote_and_callout_have_distinct_content_surfaces() {
        let quote = block(RichBlockKind::Quote, BlockChromeSnapshot::plain());
        let callout = block(
            RichBlockKind::Callout {
                variant: cditor_core::rich_text::CalloutVariant::Note,
            },
            BlockChromeSnapshot::plain(),
        );

        let quote_style = BlockChromeStyle::from_snapshot(&quote, GuiTheme::light());
        let callout_style = BlockChromeStyle::from_snapshot(&callout, GuiTheme::light());

        assert_eq!(quote_style.quote_bar, Some(GuiTheme::light().quote_bar));
        assert_eq!(
            callout_style.content_background,
            GuiTheme::light().callout_background
        );
        assert_eq!(callout_style.content_radius_px, 8.0);
    }
}
