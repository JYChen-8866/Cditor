use crate::layout::{HeightConfidence, HeightEstimate};
use crate::rich_text::{BlockPayload, RichBlockKind, plain_text_from_spans};

pub const DEFAULT_LAYOUT_WIDTH_PX: f64 = 812.0;
pub const COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX: f64 = 16.0;
pub const TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX: f64 = 14.0;
pub const NOTION_TABLE_DEFAULT_ROW_HEIGHT_PX: f64 = 36.0;
pub const NOTION_TABLE_CELL_PADDING_Y_PX: f64 = 7.0;
pub const NOTION_TABLE_CELL_LINE_HEIGHT_PX: f64 = 14.0 * 1.45;

pub const BLOCK_SHELL_PADDING_Y_PX: f64 = 4.0;
pub const NOTION_BODY_LINE_HEIGHT_PX: f64 = 24.0;
pub const NOTION_HEADING_1_LINE_HEIGHT_PX: f64 = 39.0;
pub const NOTION_HEADING_2_LINE_HEIGHT_PX: f64 = 32.0;
pub const NOTION_HEADING_3_LINE_HEIGHT_PX: f64 = 26.0;
pub const V1_CODE_TEXT_LINE_HEIGHT_PX: f64 = 24.0;
pub const V1_CODE_BASE_HEIGHT_PX: f64 = 48.0;
pub const V1_CODE_INNER_MIN_HEIGHT_PX: f64 = 92.0;
pub const IMAGE_BLOCK_ESTIMATED_HEIGHT_PX: f64 = 260.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextBlockChromeMetrics {
    pub content_min_height: f64,
    pub content_padding_y: f64,
    pub extra_inner_chrome_y: f64,
}

impl TextBlockChromeMetrics {
    pub const fn outer_chrome_y(self) -> f64 {
        BLOCK_SHELL_PADDING_Y_PX * 2.0 + self.content_padding_y * 2.0 + self.extra_inner_chrome_y
    }

    pub const fn outer_min_height(self) -> f64 {
        BLOCK_SHELL_PADDING_Y_PX * 2.0 + self.content_min_height
    }
}

pub fn text_block_chrome_metrics_for_kind(kind: &RichBlockKind) -> TextBlockChromeMetrics {
    match kind {
        RichBlockKind::Heading { level: 1 } => TextBlockChromeMetrics {
            content_min_height: NOTION_HEADING_1_LINE_HEIGHT_PX,
            content_padding_y: 0.0,
            extra_inner_chrome_y: 0.0,
        },
        RichBlockKind::Heading { level: 2 } => TextBlockChromeMetrics {
            content_min_height: NOTION_HEADING_2_LINE_HEIGHT_PX,
            content_padding_y: 0.0,
            extra_inner_chrome_y: 0.0,
        },
        RichBlockKind::Heading { level: 3 } => TextBlockChromeMetrics {
            content_min_height: NOTION_HEADING_3_LINE_HEIGHT_PX,
            content_padding_y: 0.0,
            extra_inner_chrome_y: 0.0,
        },
        RichBlockKind::Heading { .. } => TextBlockChromeMetrics {
            content_min_height: NOTION_HEADING_3_LINE_HEIGHT_PX,
            content_padding_y: 0.0,
            extra_inner_chrome_y: 0.0,
        },
        RichBlockKind::Callout { .. } => TextBlockChromeMetrics {
            content_min_height: 48.0,
            content_padding_y: 12.0,
            extra_inner_chrome_y: 0.0,
        },
        RichBlockKind::Code { .. } => TextBlockChromeMetrics {
            // 外层 shell 只承载 gutter/prefix 行；内层 code component 自己绘制 V1 min_h(92)、toolbar 和 padding。
            content_min_height: V1_CODE_INNER_MIN_HEIGHT_PX,
            content_padding_y: 0.0,
            extra_inner_chrome_y: 0.0,
        },
        _ => TextBlockChromeMetrics {
            content_min_height: NOTION_BODY_LINE_HEIGHT_PX,
            content_padding_y: 0.0,
            extra_inner_chrome_y: 0.0,
        },
    }
}

const fn text_metrics(
    chrome: TextBlockChromeMetrics,
    line_height: f64,
    avg_char_width_px: f64,
) -> TextLikeMetrics {
    TextLikeMetrics::new(
        chrome.outer_min_height(),
        line_height,
        chrome.outer_chrome_y(),
        avg_char_width_px,
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLikeMetrics {
    pub min_height: f64,
    pub line_height: f64,
    pub chrome_y: f64,
    pub avg_char_width_px: f64,
    pub max_height: Option<f64>,
    pub confidence: HeightConfidence,
}

impl TextLikeMetrics {
    pub const fn new(
        min_height: f64,
        line_height: f64,
        chrome_y: f64,
        avg_char_width_px: f64,
    ) -> Self {
        Self {
            min_height,
            line_height,
            chrome_y,
            avg_char_width_px,
            max_height: None,
            confidence: HeightConfidence::Predictive,
        }
    }

    pub const fn with_max_height(mut self, max_height: f64) -> Self {
        self.max_height = Some(max_height);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockHeightRule {
    TextLike(TextLikeMetrics),
    Fixed(f64),
    StableBox {
        estimated_height: f64,
        max_error_hint: f64,
    },
    Table,
    Media,
    Database,
}

pub fn height_rule_for_kind(kind: &RichBlockKind) -> BlockHeightRule {
    match kind {
        RichBlockKind::Paragraph => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_BODY_LINE_HEIGHT_PX,
            9.0,
        )),
        RichBlockKind::Heading { level: 1 } => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_HEADING_1_LINE_HEIGHT_PX,
            16.0,
        )),
        RichBlockKind::Heading { level: 2 } => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_HEADING_2_LINE_HEIGHT_PX,
            14.0,
        )),
        RichBlockKind::Heading { .. } => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_HEADING_3_LINE_HEIGHT_PX,
            12.0,
        )),
        RichBlockKind::Quote => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_BODY_LINE_HEIGHT_PX,
            9.0,
        )),
        RichBlockKind::Callout { .. } => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_BODY_LINE_HEIGHT_PX,
            9.0,
        )),
        RichBlockKind::Todo { .. } => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_BODY_LINE_HEIGHT_PX,
            9.0,
        )),
        RichBlockKind::BulletedList | RichBlockKind::NumberedList => {
            BlockHeightRule::TextLike(text_metrics(
                text_block_chrome_metrics_for_kind(kind),
                NOTION_BODY_LINE_HEIGHT_PX,
                9.0,
            ))
        }
        RichBlockKind::Toggle => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_BODY_LINE_HEIGHT_PX,
            9.0,
        )),
        RichBlockKind::Code { .. } => BlockHeightRule::TextLike(
            text_metrics(
                text_block_chrome_metrics_for_kind(kind),
                V1_CODE_TEXT_LINE_HEIGHT_PX,
                8.0,
            )
            .with_max_height(640.0),
        ),
        RichBlockKind::Math => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            24.0,
            10.0,
        )),
        RichBlockKind::Mermaid => BlockHeightRule::StableBox {
            estimated_height: 232.0,
            max_error_hint: 1028.0,
        },
        RichBlockKind::Html => BlockHeightRule::TextLike(
            text_metrics(
                text_block_chrome_metrics_for_kind(kind),
                NOTION_BODY_LINE_HEIGHT_PX,
                9.0,
            )
            .with_max_height(640.0),
        ),
        RichBlockKind::Table => BlockHeightRule::Table,
        RichBlockKind::Image => BlockHeightRule::Media,
        RichBlockKind::File => BlockHeightRule::Fixed(56.0),
        RichBlockKind::Attachment => BlockHeightRule::Fixed(64.0),
        RichBlockKind::Whiteboard => BlockHeightRule::StableBox {
            estimated_height: 480.0,
            max_error_hint: 160.0,
        },
        RichBlockKind::MindMap => BlockHeightRule::StableBox {
            estimated_height: 360.0,
            max_error_hint: 120.0,
        },
        RichBlockKind::Embed => BlockHeightRule::StableBox {
            estimated_height: 160.0,
            max_error_hint: 80.0,
        },
        RichBlockKind::Divider | RichBlockKind::Separator => BlockHeightRule::Fixed(32.0),
        RichBlockKind::FootnoteDefinition => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            20.0,
            8.0,
        )),
        RichBlockKind::Comment => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_BODY_LINE_HEIGHT_PX,
            9.0,
        )),
        RichBlockKind::RawMarkdown => BlockHeightRule::TextLike(text_metrics(
            text_block_chrome_metrics_for_kind(kind),
            NOTION_BODY_LINE_HEIGHT_PX,
            9.0,
        )),
        RichBlockKind::Database => BlockHeightRule::Database,
        RichBlockKind::Custom(_) => BlockHeightRule::StableBox {
            estimated_height: 96.0,
            max_error_hint: 48.0,
        },
    }
}

pub fn estimate_block_height(
    kind: &RichBlockKind,
    payload: &BlockPayload,
    width_px: f64,
) -> HeightEstimate {
    match height_rule_for_kind(kind) {
        BlockHeightRule::TextLike(metrics) => {
            let text = payload.plain_text();
            if matches!(kind, RichBlockKind::Code { .. }) {
                estimate_code_block_height_v1(&text, width_px, metrics)
            } else {
                estimate_text_like_height(&text, width_px, metrics)
            }
        }
        BlockHeightRule::Fixed(height) => HeightEstimate::new(height, HeightConfidence::Exact, 0.0),
        BlockHeightRule::StableBox {
            estimated_height,
            max_error_hint,
        } => HeightEstimate::new(
            estimated_height,
            HeightConfidence::Predictive,
            max_error_hint,
        ),
        BlockHeightRule::Table => estimate_table_height(payload),
        BlockHeightRule::Media => HeightEstimate::new(
            IMAGE_BLOCK_ESTIMATED_HEIGHT_PX,
            HeightConfidence::Predictive,
            96.0,
        ),
        BlockHeightRule::Database => {
            HeightEstimate::new(360.0, HeightConfidence::Predictive, 160.0)
        }
    }
}

pub fn estimate_kind_fallback_height(kind: &RichBlockKind) -> HeightEstimate {
    match height_rule_for_kind(kind) {
        BlockHeightRule::TextLike(metrics) => {
            HeightEstimate::new(metrics.min_height, metrics.confidence, metrics.line_height)
        }
        BlockHeightRule::Fixed(height) => HeightEstimate::new(height, HeightConfidence::Exact, 0.0),
        BlockHeightRule::StableBox {
            estimated_height,
            max_error_hint,
        } => HeightEstimate::new(
            estimated_height,
            HeightConfidence::Predictive,
            max_error_hint,
        ),
        BlockHeightRule::Table => HeightEstimate::new(
            3.0 * NOTION_TABLE_DEFAULT_ROW_HEIGHT_PX
                + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX
                + TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX,
            HeightConfidence::Predictive,
            72.0,
        ),
        BlockHeightRule::Media => HeightEstimate::new(
            IMAGE_BLOCK_ESTIMATED_HEIGHT_PX,
            HeightConfidence::Predictive,
            96.0,
        ),
        BlockHeightRule::Database => {
            HeightEstimate::new(360.0, HeightConfidence::Predictive, 160.0)
        }
    }
}

pub fn estimate_text_payload_height(
    kind: &RichBlockKind,
    text: &str,
    width_px: f64,
) -> HeightEstimate {
    match height_rule_for_kind(kind) {
        BlockHeightRule::TextLike(metrics) => {
            if matches!(kind, RichBlockKind::Code { .. }) {
                estimate_code_block_height_v1(text, width_px, metrics)
            } else {
                estimate_text_like_height(text, width_px, metrics)
            }
        }
        _ => estimate_kind_fallback_height(kind),
    }
}

pub fn estimate_rich_spans_height(
    kind: &RichBlockKind,
    spans: &[crate::rich_text::InlineSpan],
    width_px: f64,
) -> HeightEstimate {
    let text = plain_text_from_spans(spans);
    estimate_text_payload_height(kind, &text, width_px)
}

pub fn text_line_height_for_kind(kind: &RichBlockKind) -> f64 {
    match height_rule_for_kind(kind) {
        BlockHeightRule::TextLike(metrics) => metrics.line_height,
        _ => 24.0,
    }
}

pub fn normalize_text_inner_measured_height(
    kind: &RichBlockKind,
    text_inner_height: f64,
) -> HeightEstimate {
    match height_rule_for_kind(kind) {
        BlockHeightRule::TextLike(metrics) => {
            let mut height = if matches!(kind, RichBlockKind::Code { .. }) {
                (text_inner_height + code_block_v1_chrome_y()).max(code_block_v1_outer_min_height())
            } else {
                (text_inner_height + metrics.chrome_y).max(metrics.min_height)
            };
            if let Some(max_height) = metrics.max_height {
                height = height.min(max_height).max(metrics.min_height);
            }
            HeightEstimate::new(height, HeightConfidence::Exact, 0.0)
        }
        BlockHeightRule::Fixed(height) => HeightEstimate::new(height, HeightConfidence::Exact, 0.0),
        _ => estimate_kind_fallback_height(kind),
    }
}

pub fn estimate_code_block_height_v1(
    text: &str,
    width_px: f64,
    metrics: TextLikeMetrics,
) -> HeightEstimate {
    let line_count =
        estimate_wrapped_line_count(text, width_px, metrics.avg_char_width_px).max(1) as f64;
    let inner_height = (V1_CODE_BASE_HEIGHT_PX + line_count * V1_CODE_TEXT_LINE_HEIGHT_PX)
        .max(V1_CODE_INNER_MIN_HEIGHT_PX);
    let mut height = BLOCK_SHELL_PADDING_Y_PX * 2.0 + inner_height;
    if let Some(max_height) = metrics.max_height {
        height = height.min(max_height).max(code_block_v1_outer_min_height());
    }
    HeightEstimate::new(height, metrics.confidence, V1_CODE_TEXT_LINE_HEIGHT_PX)
}

const fn code_block_v1_outer_min_height() -> f64 {
    BLOCK_SHELL_PADDING_Y_PX * 2.0 + V1_CODE_INNER_MIN_HEIGHT_PX
}

const fn code_block_v1_chrome_y() -> f64 {
    BLOCK_SHELL_PADDING_Y_PX * 2.0 + V1_CODE_BASE_HEIGHT_PX
}

pub fn estimate_text_like_height(
    text: &str,
    width_px: f64,
    metrics: TextLikeMetrics,
) -> HeightEstimate {
    let line_count =
        estimate_wrapped_line_count(text, width_px, metrics.avg_char_width_px).max(1) as f64;
    let mut height = (line_count * metrics.line_height + metrics.chrome_y).max(metrics.min_height);
    if let Some(max_height) = metrics.max_height {
        height = height.min(max_height).max(metrics.min_height);
    }
    HeightEstimate::new(height, metrics.confidence, metrics.line_height)
}

pub fn estimate_wrapped_line_count(text: &str, width_px: f64, avg_char_width_px: f64) -> usize {
    let chars_per_line = (width_px / avg_char_width_px.max(1.0)).floor().max(1.0) as usize;
    text.split('\n')
        .map(|line| {
            let weighted_chars = estimate_text_width_units(line).ceil().max(1.0) as usize;
            weighted_chars.div_ceil(chars_per_line).max(1)
        })
        .sum::<usize>()
        .max(1)
}

fn estimate_text_width_units(text: &str) -> f64 {
    text.chars()
        .map(|ch| {
            if is_cjk(ch) {
                1.0
            } else if ch.is_ascii() {
                0.56
            } else {
                0.8
            }
        })
        .sum()
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF | 0xAC00..=0xD7AF
    )
}

fn estimate_table_height(payload: &BlockPayload) -> HeightEstimate {
    let rows = match payload {
        BlockPayload::Table(table) => table.rows.len().max(1),
        _ => 3,
    };
    let height = rows as f64 * NOTION_TABLE_DEFAULT_ROW_HEIGHT_PX
        + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX
        + TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX;
    HeightEstimate::new(height, HeightConfidence::Predictive, 72.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_text::{BlockPayload, InlineSpan};

    #[test]
    fn text_block_chrome_metrics_match_notion_shell_contract() {
        let paragraph = text_block_chrome_metrics_for_kind(&RichBlockKind::Paragraph);
        assert_eq!(paragraph.content_min_height, 24.0);
        assert_eq!(paragraph.content_padding_y, 0.0);
        assert_eq!(paragraph.outer_min_height(), 32.0);
        assert_eq!(paragraph.outer_chrome_y(), 8.0);

        let callout = text_block_chrome_metrics_for_kind(&RichBlockKind::Callout {
            variant: crate::rich_text::CalloutVariant::Note,
        });
        assert_eq!(callout.content_min_height, 48.0);
        assert_eq!(callout.content_padding_y, 12.0);
        assert_eq!(callout.outer_min_height(), 56.0);
        assert_eq!(callout.outer_chrome_y(), 32.0);

        let code = text_block_chrome_metrics_for_kind(&RichBlockKind::Code { language: None });
        assert_eq!(code.content_min_height, 92.0);
        assert_eq!(code.content_padding_y, 0.0);
        assert_eq!(code.extra_inner_chrome_y, 0.0);
        assert_eq!(code.outer_min_height(), 100.0);
        assert_eq!(code.outer_chrome_y(), 8.0);
        assert_eq!(code_block_v1_outer_min_height(), 100.0);
        assert_eq!(code_block_v1_chrome_y(), 56.0);
    }

    #[test]
    fn list_todo_quote_initial_heights_use_same_chrome_contract() {
        for kind in [
            RichBlockKind::BulletedList,
            RichBlockKind::NumberedList,
            RichBlockKind::Todo { checked: false },
            RichBlockKind::Quote,
        ] {
            let estimate = estimate_text_payload_height(&kind, "item", DEFAULT_LAYOUT_WIDTH_PX);
            assert_eq!(estimate.height, 32.0);
        }
    }

    #[test]
    fn code_height_includes_chrome_and_lines() {
        let payload = BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "fn main() {\n    let value = 1;\n    value + 1\n}".to_owned(),
        };
        let estimate = estimate_block_height(
            &RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            &payload,
            DEFAULT_LAYOUT_WIDTH_PX,
        );

        assert_eq!(estimate.height, 152.0);
    }

    #[test]
    fn multiline_list_todo_quote_callout_and_code_have_non_overlapping_outer_heights() {
        let three_lines = "a\nb\nc";
        let cases = [
            (RichBlockKind::BulletedList, 80.0),
            (RichBlockKind::Todo { checked: false }, 80.0),
            (RichBlockKind::Quote, 80.0),
            (
                RichBlockKind::Callout {
                    variant: crate::rich_text::CalloutVariant::Warning,
                },
                104.0,
            ),
            (RichBlockKind::Code { language: None }, 128.0),
        ];

        for (kind, minimum_height) in cases {
            let estimate =
                estimate_text_payload_height(&kind, three_lines, DEFAULT_LAYOUT_WIDTH_PX);
            assert_eq!(estimate.height, minimum_height, "{kind:?}");
        }
    }

    #[test]
    fn text_like_height_grows_with_explicit_lines() {
        let estimate = estimate_block_height(
            &RichBlockKind::Quote,
            &BlockPayload::RichText {
                spans: vec![InlineSpan::plain("a\nb\nc")],
            },
            DEFAULT_LAYOUT_WIDTH_PX,
        );

        assert!(estimate.height > 32.0);
    }

    #[test]
    fn fixed_and_stable_kinds_have_non_zero_heights() {
        assert_eq!(
            estimate_kind_fallback_height(&RichBlockKind::Divider).height,
            32.0
        );
        assert!(estimate_kind_fallback_height(&RichBlockKind::Whiteboard).height >= 240.0);
        assert!(estimate_kind_fallback_height(&RichBlockKind::Database).height >= 160.0);
    }

    #[test]
    fn mermaid_fallback_matches_the_loading_preview_box() {
        let estimate = estimate_kind_fallback_height(&RichBlockKind::Mermaid);
        assert_eq!(estimate.height, 232.0);
        assert_eq!(estimate.confidence, HeightConfidence::Predictive);
    }

    #[test]
    fn table_height_includes_rendered_shell_and_scrollbar_chrome() {
        let payload = BlockPayload::Table(crate::rich_text::TablePayload {
            rows: vec![crate::rich_text::TableRowPayload {
                cells: vec![crate::rich_text::TableCellPayload::plain("cell")],
                height: Default::default(),
            }],
            ..Default::default()
        });
        let estimate =
            estimate_block_height(&RichBlockKind::Table, &payload, DEFAULT_LAYOUT_WIDTH_PX);

        assert_eq!(
            estimate.height,
            NOTION_TABLE_DEFAULT_ROW_HEIGHT_PX
                + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX
                + TABLE_HORIZONTAL_SCROLLBAR_CHROME_HEIGHT_PX
        );
    }

    #[test]
    fn measured_text_inner_height_uses_same_kind_chrome_and_minimums() {
        assert_eq!(text_line_height_for_kind(&RichBlockKind::Paragraph), 24.0);
        assert_eq!(
            normalize_text_inner_measured_height(&RichBlockKind::Paragraph, 24.0).height,
            32.0
        );
        assert_eq!(
            text_line_height_for_kind(&RichBlockKind::Code { language: None }),
            24.0
        );
        assert_eq!(
            normalize_text_inner_measured_height(&RichBlockKind::Code { language: None }, 88.0,)
                .height,
            144.0
        );
        assert_eq!(
            normalize_text_inner_measured_height(&RichBlockKind::Divider, 200.0).height,
            32.0
        );
    }

    #[test]
    fn notion_heading_line_heights_are_shared_with_layout() {
        assert_eq!(
            text_line_height_for_kind(&RichBlockKind::Heading { level: 1 }),
            39.0
        );
        assert_eq!(
            text_line_height_for_kind(&RichBlockKind::Heading { level: 2 }),
            32.0
        );
        assert_eq!(
            text_line_height_for_kind(&RichBlockKind::Heading { level: 3 }),
            26.0
        );
    }
}
