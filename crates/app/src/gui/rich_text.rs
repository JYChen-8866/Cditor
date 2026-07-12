use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, IntoElement, ParentElement, SharedString, Styled, div, px, rgb,
};

use cditor_core::layout::block_metrics::BLOCK_SHELL_PADDING_Y_PX;
use cditor_core::layout::estimate_kind_fallback_height;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan, RichBlockKind,
};

use super::GuiTheme;

pub(crate) const NOTION_INLINE_CODE_RADIUS_PX: f32 = 3.0;
pub(crate) const NOTION_INLINE_CODE_TEXT_SIZE_PX: f32 = 13.0;
pub(crate) const NOTION_MONO_FONT_FAMILY: &str = "Menlo";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InlineMarkVisualStyle {
    pub text_color: u32,
    pub background_color: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub underline: bool,
    pub link: bool,
}

pub(crate) fn inline_mark_visual_style(
    marks: &[InlineMark],
    theme: GuiTheme,
    base_text_color: u32,
) -> InlineMarkVisualStyle {
    let mut explicit_text_color = None;
    let mut explicit_background = None;
    let mut bold = false;
    let mut italic = false;
    let mut code = false;
    let mut strike = false;
    let mut underline = false;
    let mut link = false;

    for mark in marks {
        match mark {
            InlineMark::Bold => bold = true,
            InlineMark::Italic => italic = true,
            InlineMark::Underline => underline = true,
            InlineMark::Strike => strike = true,
            InlineMark::Code => code = true,
            InlineMark::Link { .. } => link = true,
            InlineMark::Color(color) => {
                if let Some(color) = parse_hex_color(color) {
                    explicit_text_color = Some(color);
                }
            }
            InlineMark::Background(color) => {
                if let Some(color) = parse_hex_color(color) {
                    explicit_background = Some(color);
                }
            }
        }
    }

    let text_color = explicit_text_color.unwrap_or_else(|| {
        if link {
            theme.focused
        } else if code {
            theme.inline_code_text
        } else {
            base_text_color
        }
    });
    InlineMarkVisualStyle {
        text_color,
        background_color: explicit_background.or(code.then_some(theme.inline_code_background)),
        bold,
        italic,
        code,
        strike,
        underline: underline || link,
        link,
    }
}

pub fn render_payload_text(payload: &BlockPayloadRecord, theme: GuiTheme) -> AnyElement {
    match &payload.payload {
        BlockPayload::RichText { spans } => render_inline_spans(spans, theme),
        BlockPayload::Code { text, language } => {
            render_code_payload(text, language.as_deref(), theme)
        }
        BlockPayload::Table(table) => div()
            .min_h(px(fallback_inner_height_px(&payload.kind)))
            .flex()
            .flex_col()
            .gap_1()
            .children(table.rows.iter().map(|row| {
                div().flex().children(row.cells.iter().map(|cell| {
                    div()
                        .min_w(px(96.0))
                        .px_2()
                        .py_1()
                        .border_1()
                        .border_color(rgb(theme.border))
                        .child(render_inline_spans(&cell.spans, theme))
                }))
            }))
            .into_any_element(),
        BlockPayload::Image(image) => div()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .border_1()
            .border_color(rgb(theme.border))
            .rounded(px(3.0))
            .bg(rgb(theme.hover_surface))
            .child(image.alt.clone())
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.muted))
                    .child(format!("{} {}", image.alt, image.caption)),
            )
            .into_any_element(),
        BlockPayload::File(file) => {
            render_file_card(file, fallback_inner_height_px(&payload.kind), theme)
        }
        BlockPayload::Whiteboard(_) => render_embedded_surface(
            embedded_surface_label(&payload.kind),
            fallback_inner_height_px(&payload.kind),
            theme,
        ),
        BlockPayload::Embed(embed) => div()
            .w_full()
            .min_h(px(fallback_inner_height_px(&payload.kind)))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(3.0))
            .border_1()
            .border_color(rgb(theme.border))
            .flex()
            .flex_col()
            .justify_center()
            .child(embed.title.clone())
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.muted))
                    .child(embed.url.clone()),
            )
            .into_any_element(),
        BlockPayload::Html { html, sanitized } => div()
            .w_full()
            .p_3()
            .rounded(px(3.0))
            .bg(rgb(theme.code_background))
            .font_family(NOTION_MONO_FONT_FAMILY)
            .text_size(px(NOTION_INLINE_CODE_TEXT_SIZE_PX))
            .text_color(rgb(theme.code_text))
            .child(if *sanitized {
                html.clone()
            } else {
                "Unsafe HTML was blocked".to_owned()
            })
            .into_any_element(),
        BlockPayload::Empty => render_empty_payload(&payload.kind, theme),
    }
}

pub fn render_wrapped_payload_text(payload: &BlockPayloadRecord, theme: GuiTheme) -> AnyElement {
    match &payload.payload {
        BlockPayload::RichText { spans } => render_wrapped_inline_spans(spans, theme),
        _ => render_payload_text(payload, theme),
    }
}

fn render_file_card(
    file: &cditor_core::rich_text::FilePayload,
    min_height_px: f32,
    theme: GuiTheme,
) -> AnyElement {
    div()
        .w_full()
        .min_h(px(min_height_px))
        .px(px(10.0))
        .rounded(px(3.0))
        .border_1()
        .border_color(rgb(theme.border))
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(
            div()
                .size(px(32.0))
                .rounded(px(3.0))
                .bg(rgb(theme.hover_surface))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(theme.muted))
                .child("FILE"),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(theme.text))
                        .child(file.name.clone()),
                )
                .when_some(file.size_bytes, |this, bytes| {
                    this.child(
                        div()
                            .mt(px(2.0))
                            .text_size(px(12.0))
                            .text_color(rgb(theme.muted))
                            .child(format_file_size(bytes)),
                    )
                }),
        )
        .into_any_element()
}

fn render_embedded_surface(label: &'static str, min_height_px: f32, theme: GuiTheme) -> AnyElement {
    div()
        .w_full()
        .min_h(px(min_height_px))
        .rounded(px(3.0))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.page))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.0))
        .text_color(rgb(theme.muted))
        .child(label)
        .into_any_element()
}

fn render_empty_payload(kind: &RichBlockKind, theme: GuiTheme) -> AnyElement {
    match kind {
        RichBlockKind::Image
        | RichBlockKind::Whiteboard
        | RichBlockKind::MindMap
        | RichBlockKind::Embed
        | RichBlockKind::Database => render_embedded_surface(
            embedded_surface_label(kind),
            fallback_inner_height_px(kind),
            theme,
        ),
        _ => div().into_any_element(),
    }
}

fn embedded_surface_label(kind: &RichBlockKind) -> &'static str {
    match kind {
        RichBlockKind::Image => "Image",
        RichBlockKind::MindMap => "Mind map",
        RichBlockKind::Embed => "Embed",
        RichBlockKind::Database => "Database",
        _ => "Whiteboard",
    }
}

fn fallback_inner_height_px(kind: &RichBlockKind) -> f32 {
    let shell_height = BLOCK_SHELL_PADDING_Y_PX * 2.0;
    (estimate_kind_fallback_height(kind).height - shell_height).max(0.0) as f32
}

fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{} B", bytes as u64)
    }
}

pub fn render_inline_spans(spans: &[InlineSpan], theme: GuiTheme) -> AnyElement {
    if spans.is_empty() || spans.iter().all(|span| span.text.is_empty()) {
        return div()
            .text_color(rgb(theme.muted))
            .child("请输入...")
            .into_any_element();
    }

    div()
        .flex()
        .flex_wrap()
        .items_baseline()
        .children(spans.iter().map(|span| render_span(span, theme)))
        .into_any_element()
}

fn render_wrapped_inline_spans(spans: &[InlineSpan], theme: GuiTheme) -> AnyElement {
    if spans.is_empty() || spans.iter().all(|span| span.text.is_empty()) {
        return div()
            .w_full()
            .min_w(px(0.0))
            .whitespace_normal()
            .text_color(rgb(theme.muted))
            .child("请输入...")
            .into_any_element();
    }

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_wrap()
        .items_baseline()
        .whitespace_normal()
        .children(spans.iter().map(|span| render_wrapped_span(span, theme)))
        .into_any_element()
}

fn render_span(span: &InlineSpan, theme: GuiTheme) -> AnyElement {
    render_span_with_wrapping(span, theme, false)
}

fn render_wrapped_span(span: &InlineSpan, theme: GuiTheme) -> AnyElement {
    render_span_with_wrapping(span, theme, true)
}

fn render_span_with_wrapping(span: &InlineSpan, theme: GuiTheme, wrapping: bool) -> AnyElement {
    let style = inline_mark_visual_style(&span.marks, theme, theme.text);

    div()
        .when(wrapping, |this| this.min_w(px(0.0)).whitespace_normal())
        .when(style.code, |this| {
            this.px_1()
                .rounded(px(NOTION_INLINE_CODE_RADIUS_PX))
                .font_family(NOTION_MONO_FONT_FAMILY)
                .text_size(px(NOTION_INLINE_CODE_TEXT_SIZE_PX))
        })
        .when_some(style.background_color, |this, color| this.bg(rgb(color)))
        .when(style.bold, |this| this.font_weight(FontWeight::BOLD))
        .when(style.italic, |this| this.italic())
        .when(style.underline, |this| this.text_decoration_1())
        .when(style.strike, |this| this.line_through())
        .text_color(rgb(style.text_color))
        .child(SharedString::from(span.text.clone()))
        .into_any_element()
}

fn render_code_payload(text: &str, language: Option<&str>, theme: GuiTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme.muted))
                .child(language.unwrap_or("plain text").to_owned()),
        )
        .child(
            div()
                .font_family(NOTION_MONO_FONT_FAMILY)
                .text_size(px(NOTION_INLINE_CODE_TEXT_SIZE_PX))
                .child(if text.is_empty() {
                    "请输入代码...".to_owned()
                } else {
                    text.to_owned()
                }),
        )
        .into_any_element()
}

fn parse_hex_color(color: &str) -> Option<u32> {
    let value = color.strip_prefix('#').unwrap_or(color);
    u32::from_str_radix(value, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_code_uses_notion_red_and_neutral_background() {
        let theme = GuiTheme::light();
        let style = inline_mark_visual_style(&[InlineMark::Code], theme, theme.text);

        assert_eq!(style.text_color, theme.inline_code_text);
        assert_eq!(style.background_color, Some(theme.inline_code_background));
        assert!(style.code);
    }

    #[test]
    fn explicit_inline_colors_override_code_defaults() {
        let theme = GuiTheme::light();
        let style = inline_mark_visual_style(
            &[
                InlineMark::Code,
                InlineMark::Color("#123456".to_owned()),
                InlineMark::Background("abcdef".to_owned()),
                InlineMark::Strike,
            ],
            theme,
            theme.text,
        );

        assert_eq!(style.text_color, 0x123456);
        assert_eq!(style.background_color, Some(0xabcdef));
        assert!(style.strike);
    }

    #[test]
    fn file_size_label_uses_compact_notion_units() {
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn fallback_card_heights_match_core_block_height_contract() {
        assert_eq!(fallback_inner_height_px(&RichBlockKind::File), 48.0);
        assert_eq!(fallback_inner_height_px(&RichBlockKind::Attachment), 56.0);
        assert_eq!(fallback_inner_height_px(&RichBlockKind::Embed), 152.0);
        assert_eq!(fallback_inner_height_px(&RichBlockKind::Whiteboard), 472.0);
        assert_eq!(fallback_inner_height_px(&RichBlockKind::MindMap), 352.0);
    }

    #[test]
    fn embedded_fallback_labels_distinguish_supported_surface_kinds() {
        assert_eq!(
            embedded_surface_label(&RichBlockKind::Whiteboard),
            "Whiteboard"
        );
        assert_eq!(embedded_surface_label(&RichBlockKind::MindMap), "Mind map");
        assert_eq!(embedded_surface_label(&RichBlockKind::Embed), "Embed");
        assert_eq!(embedded_surface_label(&RichBlockKind::Database), "Database");
    }
}
