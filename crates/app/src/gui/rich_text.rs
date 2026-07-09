use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, IntoElement, ParentElement, SharedString, Styled, div, px, rgb,
};

use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan};

use super::GuiTheme;

pub fn render_payload_text(payload: &BlockPayloadRecord, theme: GuiTheme) -> AnyElement {
    match &payload.payload {
        BlockPayload::RichText { spans } => render_inline_spans(spans, theme),
        BlockPayload::Code { text, language } => {
            render_code_payload(text, language.as_deref(), theme)
        }
        BlockPayload::Table(table) => div()
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
            .rounded(px(8.0))
            .bg(rgb(0xf6f8fa))
            .child(format!("Image: {}", image.source))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.muted))
                    .child(format!("{} {}", image.alt, image.caption)),
            )
            .into_any_element(),
        BlockPayload::File(file) => div()
            .text_color(rgb(theme.muted))
            .child(format!("📎 {}", file.name))
            .into_any_element(),
        BlockPayload::Whiteboard(_) => div()
            .text_color(rgb(theme.muted))
            .child("Whiteboard")
            .into_any_element(),
        BlockPayload::Embed(embed) => div()
            .flex()
            .flex_col()
            .child(embed.title.clone())
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.muted))
                    .child(embed.url.clone()),
            )
            .into_any_element(),
        BlockPayload::Html { html, sanitized } => div()
            .font_family("Menlo")
            .text_size(px(13.0))
            .text_color(rgb(theme.muted))
            .child(format!("HTML sanitized={sanitized}: {html}"))
            .into_any_element(),
        BlockPayload::Empty => div()
            .text_color(rgb(theme.muted))
            .child("—")
            .into_any_element(),
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

fn render_span(span: &InlineSpan, theme: GuiTheme) -> AnyElement {
    let mut text_color = theme.text;
    let mut background_color = None;
    let mut link_href = None;
    let mut is_bold = false;
    let mut is_italic = false;
    let mut is_code = false;
    let mut is_strike = false;
    let mut is_underline = false;

    for mark in &span.marks {
        match mark {
            InlineMark::Bold => is_bold = true,
            InlineMark::Italic => is_italic = true,
            InlineMark::Underline => is_underline = true,
            InlineMark::Strike => is_strike = true,
            InlineMark::Code => is_code = true,
            InlineMark::Link { href } => {
                link_href = Some(href.as_str());
                text_color = theme.focused;
                is_underline = true;
            }
            InlineMark::Color(color) => text_color = parse_hex_color(color).unwrap_or(text_color),
            InlineMark::Background(color) => background_color = parse_hex_color(color),
        }
    }

    let label = if link_href.is_some() {
        span.text.replace('\n', "\n")
    } else {
        span.text.clone()
    };

    div()
        .when(is_code, |this| {
            this.px_1()
                .rounded(px(4.0))
                .bg(rgb(theme.code_background))
                .font_family("Menlo")
                .text_size(px(13.0))
        })
        .when_some(background_color, |this, color| this.bg(rgb(color)))
        .when(is_bold, |this| this.font_weight(FontWeight::BOLD))
        .when(is_italic, |this| this.italic())
        .when(is_underline, |this| this.text_decoration_1())
        .when(is_strike, |this| this.line_through())
        .text_color(rgb(text_color))
        .child(SharedString::from(label))
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
                .font_family("Menlo")
                .text_size(px(13.0))
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
