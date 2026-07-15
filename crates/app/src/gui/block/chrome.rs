use crate::gui::GuiTheme;
use cditor_core::layout::block_metrics::{
    NOTION_BODY_LINE_HEIGHT_PX, NOTION_HEADING_1_LINE_HEIGHT_PX, NOTION_HEADING_2_LINE_HEIGHT_PX,
    NOTION_HEADING_3_LINE_HEIGHT_PX,
};
use cditor_core::rich_text::RichBlockKind;
use cditor_runtime::ViewBlockSnapshot;

pub const BLOCK_INDENT_STEP_PX: f32 = 24.0;
pub const BLOCK_GUTTER_WIDTH_PX: f32 = 48.0;
pub const BLOCK_GUTTER_HEIGHT_PX: f32 = 24.0;
pub const BLOCK_SHELL_OUTER_PADDING_X_PX: f32 = 8.0;
pub const BLOCK_ROW_GAP_PX: f32 = 8.0;
pub const BLOCK_SHELL_BORDER_WIDTH_PX: f32 = 1.0;
pub const BLOCK_CONTENT_BORDER_WIDTH_PX: f32 = 1.0;

pub const fn block_content_left_px(indent_px: f32) -> f32 {
    BLOCK_SHELL_OUTER_PADDING_X_PX + indent_px + BLOCK_GUTTER_WIDTH_PX + BLOCK_ROW_GAP_PX
}
pub const BLOCK_PREFIX_WIDTH_PX: f32 = 24.0;
pub const CALLOUT_PREFIX_WIDTH_PX: f32 = 32.0;
pub const NOTION_QUOTE_CONTENT_GAP_PX: f32 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockChromeStyle {
    pub indent_px: f32,
    pub gutter_width_px: f32,
    pub gutter_height_px: f32,
    pub marker_lane_width_px: f32,
    pub content_prefix_width_px: f32,
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
        let block_background = block
            .attrs
            .background_color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(kind_style.background);
        let block_text = block
            .attrs
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(kind_style.text);
        Self {
            indent_px: block.chrome.list_info.depth as f32 * BLOCK_INDENT_STEP_PX,
            gutter_width_px: BLOCK_GUTTER_WIDTH_PX,
            gutter_height_px: BLOCK_GUTTER_HEIGHT_PX,
            marker_lane_width_px: BLOCK_PREFIX_WIDTH_PX,
            content_prefix_width_px: block_content_prefix_width_px(block),
            content_min_height_px: kind_style.min_height_px,
            content_padding_y_px: kind_style.padding_y_px,
            content_padding_left_px: kind_style.padding_left_px,
            content_padding_right_px: kind_style.padding_right_px,
            content_radius_px: kind_style.radius_px,
            outer_background,
            content_background: block_background,
            content_border: kind_style.border,
            text_color: block_text,
            quote_bar: kind_style.quote_bar,
        }
    }
}

fn parse_hex_color(value: &str) -> Option<u32> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// Prefixes rendered inside the block surface rather than in the marker lane.
/// Callout icons belong to the callout card. A todo checkbox starts exactly at
/// the shared block surface origin and reserves one marker-width before text.
pub fn block_content_prefix_width_px(block: &ViewBlockSnapshot) -> f32 {
    use cditor_core::block::BlockPrefixSnapshot;

    match block.chrome.prefix {
        BlockPrefixSnapshot::Callout { .. } => CALLOUT_PREFIX_WIDTH_PX,
        BlockPrefixSnapshot::Todo { .. } => BLOCK_PREFIX_WIDTH_PX,
        _ => 0.0,
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
            // Media and table geometry is consumed by their stable-box/overlay paths.
            RichBlockKind::Image | RichBlockKind::Table => Self::legacy_complex(theme),
            _ => Self::paragraph(theme),
        }
    }

    fn paragraph(theme: GuiTheme) -> Self {
        Self {
            background: theme.surface,
            border: theme.surface,
            text: theme.text,
            padding_y_px: 0.0,
            padding_left_px: 0.0,
            padding_right_px: 0.0,
            min_height_px: NOTION_BODY_LINE_HEIGHT_PX as f32,
            radius_px: 0.0,
            quote_bar: None,
        }
    }

    fn heading(level: u8, theme: GuiTheme) -> Self {
        let min_height_px = match level {
            1 => NOTION_HEADING_1_LINE_HEIGHT_PX as f32,
            2 => NOTION_HEADING_2_LINE_HEIGHT_PX as f32,
            _ => NOTION_HEADING_3_LINE_HEIGHT_PX as f32,
        };
        Self {
            padding_y_px: 0.0,
            min_height_px,
            ..Self::paragraph(theme)
        }
    }

    fn quote(theme: GuiTheme) -> Self {
        Self {
            background: theme.surface,
            border: theme.surface,
            text: theme.quote_text,
            padding_y_px: 0.0,
            padding_left_px: NOTION_QUOTE_CONTENT_GAP_PX,
            padding_right_px: 0.0,
            min_height_px: NOTION_BODY_LINE_HEIGHT_PX as f32,
            radius_px: 0.0,
            quote_bar: Some(theme.quote_bar),
        }
    }

    fn callout(theme: GuiTheme) -> Self {
        Self {
            background: theme.callout_background,
            border: theme.callout_border,
            text: theme.text,
            padding_y_px: 12.0,
            padding_left_px: 12.0,
            padding_right_px: 12.0,
            min_height_px: 48.0,
            radius_px: 3.0,
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
            radius_px: 0.0,
            quote_bar: None,
        }
    }

    fn legacy_complex(theme: GuiTheme) -> Self {
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
            selection_overlay: false,
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
        assert_eq!(style.gutter_width_px, 48.0);
        assert_eq!(style.marker_lane_width_px, 24.0);
        assert_eq!(style.content_prefix_width_px, 0.0);
    }

    #[test]
    fn block_attrs_override_surface_and_base_text_colors() {
        let mut snapshot = block(RichBlockKind::Paragraph, BlockChromeSnapshot::plain());
        snapshot.attrs.color = Some("#d44c47".to_owned());
        snapshot.attrs.background_color = Some("#fdebec".to_owned());

        let style = BlockChromeStyle::from_snapshot(&snapshot, GuiTheme::light());

        assert_eq!(style.text_color, 0xd44c47);
        assert_eq!(style.content_background, 0xfdebec);
        assert_eq!(style.outer_background, GuiTheme::light().surface);
    }

    #[test]
    fn heading_and_root_paragraph_share_a_marker_lane_without_gutter_overlap() {
        let heading = block(
            RichBlockKind::Heading { level: 1 },
            BlockChromeSnapshot::from_kind(
                &RichBlockKind::Heading { level: 1 },
                BlockListInfo::root(),
                true,
                false,
            ),
        );
        let paragraph = block(RichBlockKind::Paragraph, BlockChromeSnapshot::plain());

        let heading_style = BlockChromeStyle::from_snapshot(&heading, GuiTheme::light());
        let paragraph_style = BlockChromeStyle::from_snapshot(&paragraph, GuiTheme::light());

        assert_eq!(heading_style.marker_lane_width_px, BLOCK_PREFIX_WIDTH_PX);
        assert_eq!(heading_style.indent_px, paragraph_style.indent_px);
        assert_eq!(paragraph_style.marker_lane_width_px, BLOCK_PREFIX_WIDTH_PX);
    }

    #[test]
    fn every_block_surface_starts_after_the_same_marker_lane() {
        for kind in [
            RichBlockKind::Code { language: None },
            RichBlockKind::Table,
            RichBlockKind::Image,
            RichBlockKind::Quote,
        ] {
            let style = BlockChromeStyle::from_snapshot(
                &block(kind, BlockChromeSnapshot::plain()),
                GuiTheme::light(),
            );
            assert_eq!(style.marker_lane_width_px, BLOCK_PREFIX_WIDTH_PX);
            assert_eq!(style.content_prefix_width_px, 0.0);
        }
    }

    #[test]
    fn callout_icon_is_an_internal_surface_prefix() {
        let kind = RichBlockKind::Callout {
            variant: cditor_core::rich_text::CalloutVariant::Note,
        };
        let style = BlockChromeStyle::from_snapshot(
            &block(
                kind.clone(),
                BlockChromeSnapshot::from_kind(&kind, BlockListInfo::root(), false, false),
            ),
            GuiTheme::light(),
        );

        assert_eq!(style.marker_lane_width_px, BLOCK_PREFIX_WIDTH_PX);
        assert_eq!(style.content_prefix_width_px, CALLOUT_PREFIX_WIDTH_PX);
    }

    #[test]
    fn todo_checkbox_is_an_internal_surface_prefix() {
        let kind = RichBlockKind::Todo { checked: false };
        let style = BlockChromeStyle::from_snapshot(
            &block(
                kind.clone(),
                BlockChromeSnapshot::from_kind(&kind, BlockListInfo::root(), false, false),
            ),
            GuiTheme::light(),
        );

        assert_eq!(style.marker_lane_width_px, BLOCK_PREFIX_WIDTH_PX);
        assert_eq!(style.content_prefix_width_px, BLOCK_PREFIX_WIDTH_PX);
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
    fn notion_block_visual_colors_use_semantic_theme_tokens() {
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
        assert_eq!(quote_style.content_padding_left_px, 14.0);
        assert_eq!(
            callout_style.content_background,
            GuiTheme::light().callout_background
        );
        assert_eq!(callout_style.content_radius_px, 3.0);
        assert_eq!(callout_style.content_padding_y_px, 12.0);
        assert_eq!(callout_style.content_padding_left_px, 12.0);
    }
}
