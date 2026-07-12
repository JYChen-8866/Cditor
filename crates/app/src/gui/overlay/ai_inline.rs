use std::collections::HashMap;

use cditor_core::ids::BlockId;
use cditor_runtime::{AiApplyMode, AiPreviewKind, AiPreviewSnapshot, AiPreviewStatus};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Bounds, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, ScrollHandle, StatefulInteractiveElement, Styled, deferred, div,
    px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::{
    heading::render_heading,
    list::{render_bulleted, render_numbered, render_todo},
};
use crate::gui::input::{
    AiPromptState, SINGLE_LINE_INPUT_FONT_SIZE_PX, SingleLineTextInputElement,
};
use crate::gui::rich_text::render_wrapped_payload_text;
use crate::gui::text::{RichTextPlatformLayout, platform_range_bounds};
use cditor_core::rich_text::{
    BlockPayloadRecord, MarkdownImportOptions, RichBlockKind, parse_markdown_document,
};

const AI_PROMPT_WIDTH_PX: f32 = 420.0;
const AI_PREVIEW_WIDTH_PX: f32 = 520.0;
const AI_ASSISTANT_PANEL_WIDTH_PX: f32 = 640.0;
const AI_ASSISTANT_PANEL_HEIGHT_PX: f32 = 320.0;
const AI_SELECTION_PANEL_HEIGHT_PX: f32 = 240.0;
const AI_VIEWPORT_MARGIN_PX: f32 = 12.0;
const AI_PANEL_GAP_PX: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct AiPreviewGeometry {
    x: f32,
    y: f32,
    width: f32,
    max_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AiScrollbarMetrics {
    thumb_top: f32,
    thumb_height: f32,
}

pub fn render_ai_prompt(
    prompt: &AiPromptState,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    focus: FocusHandle,
) -> AnyElement {
    let quick_actions = [
        "Improve writing",
        "Fix spelling and grammar",
        "Make shorter",
        "Make longer",
        "Translate",
    ];
    let panel = div()
        .absolute()
        .left(prompt.x)
        .top(prompt.y + px(8.0))
        .w(px(AI_PROMPT_WIDTH_PX))
        .p(px(8.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(theme.strong_border))
        .bg(rgb(theme.panel))
        .shadow_lg()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down_out({
            let view = view.clone();
            move |_event, _window, cx| {
                let _ = view.update(cx, |view, cx| view.cancel_ai_prompt(cx));
            }
        })
        .child(
            div()
                .h(px(36.0))
                .w_full()
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(theme.border))
                .track_focus(&focus)
                .child(SingleLineTextInputElement {
                    handler: view.clone(),
                    focus,
                    value: prompt.draft.clone(),
                    placeholder: Some("Ask AI to write or edit...".to_owned()),
                    caret_offset: Some(prompt.caret_offset),
                    marked_range: prompt.marked_range.clone(),
                    text_color: theme.text,
                    placeholder_color: theme.muted,
                    caret_color: theme.focused,
                    font_size: px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
                }),
        )
        .child(
            div().flex().flex_wrap().gap(px(4.0)).children(
                quick_actions
                    .into_iter()
                    .map(|label| ai_command_button(label, theme, view.clone())),
            ),
        );
    deferred(panel).with_priority(150).into_any_element()
}

pub(crate) fn render_ai_preview_overlay(
    preview: Option<&AiPreviewSnapshot>,
    layouts: &HashMap<BlockId, RichTextPlatformLayout>,
    block_anchor: Option<Bounds<gpui::Pixels>>,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    scroll_handle: &ScrollHandle,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<AnyElement> {
    let preview = preview?;
    let layout = layouts.get(&preview.block_id);
    let text_anchor = preview
        .replacement_range
        .clone()
        .and_then(|range| layout.and_then(|layout| platform_range_bounds(layout, range)))
        .or_else(|| {
            layout.and_then(|layout| {
                platform_range_bounds(layout, preview.anchor_offset..preview.anchor_offset)
            })
        });
    let anchor = preferred_ai_preview_anchor(preview.kind, text_anchor, block_anchor)?;
    let geometry = ai_preview_geometry(anchor, preview.kind, viewport_width, viewport_height);

    let content = match &preview.status {
        AiPreviewStatus::Failed(message) => message.clone(),
        AiPreviewStatus::Streaming if preview.text.is_empty() => "Thinking...".to_owned(),
        AiPreviewStatus::Streaming | AiPreviewStatus::Ready => preview.text.clone(),
    };
    let is_inline = preview.kind == AiPreviewKind::InlineCompletion;
    let is_selection = preview.kind == AiPreviewKind::SelectionRewrite;
    let failed = matches!(preview.status, AiPreviewStatus::Failed(_));
    let ready = matches!(preview.status, AiPreviewStatus::Ready);
    let show_actions = ready || failed;
    let panel_height = ai_panel_height(preview.kind, geometry.max_height);
    // Reserve panel padding, the action row, and the flex gap so the content
    // receives a real viewport. `max_h` alone lets the child keep its intrinsic
    // height on the first layout pass, which makes GPUI report no scroll range.
    let content_max_height =
        (panel_height - 20.0 - if show_actions { 36.0 } else { 0.0 } - 8.0).max(48.0);
    let estimated_content_height =
        estimate_ai_content_height(&content, (geometry.width - 28.0).max(80.0));
    let scroll_view = view.clone();
    let scrollable_content = div()
        .id(("ai-preview-content", preview.request_id))
        .w_full()
        .min_w(px(0.0))
        .h(px(content_max_height))
        .whitespace_normal()
        .overflow_y_scroll()
        .track_scroll(scroll_handle)
        .on_scroll_wheel(move |_event, _window, cx| {
            let _ = scroll_view.update(cx, |_view, cx| cx.notify());
        })
        .child(render_ai_preview_content(&content, theme));
    let actions = div()
        .flex()
        .gap(px(6.0))
        .when(ready, |actions| {
            actions
                .when(is_selection, |actions| {
                    actions.child(ai_preview_apply_button(
                        "替换",
                        AiApplyMode::Replace,
                        true,
                        theme,
                        view.clone(),
                    ))
                })
                .child(ai_preview_apply_button(
                    "插入",
                    AiApplyMode::InsertAfter,
                    is_inline,
                    theme,
                    view.clone(),
                ))
        })
        .child(ai_preview_discard_button(theme, view.clone()));
    let panel = div()
        .id(("ai-preview-panel", preview.request_id))
        .absolute()
        .left(px(geometry.x))
        .top(px(geometry.y))
        .max_w(px(geometry.width))
        .when(!is_inline, |panel| {
            panel
                .w(px(geometry.width))
                // Keep the outer result card stable. Only the inner content
                // viewport is allowed to scroll as the model streams more text.
                .h(px(panel_height))
                .max_h(px(geometry.max_height))
                .overflow_hidden()
                .p(px(10.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(if failed { theme.danger } else { theme.border }))
                .bg(rgb(theme.panel))
                .shadow_lg()
                .child(scrollable_content)
                .when(show_actions, |panel| panel.child(actions))
                .child(render_ai_panel_scrollbar(
                    scroll_handle,
                    content_max_height,
                    estimated_content_height,
                    theme,
                ))
        })
        .when(is_inline, |panel| {
            panel.px(px(2.0)).rounded(px(3.0)).bg(rgb(theme.page))
        })
        .flex()
        .flex_col()
        .gap(px(8.0))
        .text_size(px(14.0))
        .text_color(rgb(if failed {
            theme.danger
        } else if is_inline {
            theme.muted
        } else {
            theme.text
        }))
        .occlude()
        .when(is_inline, |panel| panel.child(content));
    Some(deferred(panel).with_priority(145).into_any_element())
}

fn preferred_ai_preview_anchor(
    kind: AiPreviewKind,
    text_anchor: Option<Bounds<gpui::Pixels>>,
    block_anchor: Option<Bounds<gpui::Pixels>>,
) -> Option<Bounds<gpui::Pixels>> {
    if kind == AiPreviewKind::InlineCompletion {
        text_anchor.or(block_anchor)
    } else {
        block_anchor.or(text_anchor)
    }
}

fn render_ai_panel_scrollbar(
    scroll_handle: &ScrollHandle,
    track_height: f32,
    estimated_content_height: f32,
    theme: GuiTheme,
) -> AnyElement {
    let max_offset = f32::from(scroll_handle.max_offset().y)
        .max((estimated_content_height - track_height).max(0.0));
    let Some(metrics) = ai_scrollbar_metrics(
        f32::from(scroll_handle.offset().y),
        max_offset,
        track_height,
    ) else {
        return div().into_any_element();
    };
    div()
        .absolute()
        .top(px(10.0))
        .right(px(4.0))
        .w(px(6.0))
        .h(px(track_height))
        .child(
            div()
                .absolute()
                .top(px(metrics.thumb_top))
                .w_full()
                .h(px(metrics.thumb_height))
                .rounded(px(3.0))
                .bg(rgb(theme.scrollbar))
                .hover(|style| style.bg(rgb(theme.scrollbar_hover))),
        )
        .into_any_element()
}

fn ai_panel_height(kind: AiPreviewKind, available_height: f32) -> f32 {
    let desired = if kind == AiPreviewKind::AssistantPanel {
        AI_ASSISTANT_PANEL_HEIGHT_PX
    } else {
        AI_SELECTION_PANEL_HEIGHT_PX
    };
    desired.min(available_height).max(80.0)
}

fn render_ai_preview_content(markdown: &str, theme: GuiTheme) -> AnyElement {
    let parsed = parse_markdown_document(markdown, MarkdownImportOptions::default());
    if parsed.blocks.is_empty() {
        return div().child(markdown.to_owned()).into_any_element();
    }

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .whitespace_normal()
        .children(parsed.blocks.into_iter().enumerate().map(|(index, block)| {
            let payload = BlockPayloadRecord {
                block_id: block.id,
                content_version: block.content_version,
                kind: block.kind.clone(),
                payload: block.payload.clone(),
            };
            let text = render_wrapped_payload_text(&payload, theme);
            match block.kind {
                RichBlockKind::Heading { level } => render_heading(level, text),
                RichBlockKind::BulletedList => render_bulleted(text),
                RichBlockKind::NumberedList => render_numbered(index + 1, text),
                RichBlockKind::Todo { checked } => render_todo(checked, text),
                RichBlockKind::Quote => div()
                    .border_l_4()
                    .border_color(rgb(theme.quote_bar))
                    .pl(px(10.0))
                    .text_color(rgb(theme.quote_text))
                    .child(text)
                    .into_any_element(),
                RichBlockKind::Code { .. } => div()
                    .w_full()
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .bg(rgb(theme.code_background))
                    .font_weight(FontWeight::NORMAL)
                    .child(text)
                    .into_any_element(),
                RichBlockKind::Divider | RichBlockKind::Separator => div()
                    .w_full()
                    .h(px(1.0))
                    .my(px(6.0))
                    .bg(rgb(theme.border))
                    .into_any_element(),
                _ => div()
                    .w_full()
                    .min_w(px(0.0))
                    .whitespace_normal()
                    .child(text)
                    .into_any_element(),
            }
        }))
        .into_any_element()
}

fn estimate_ai_content_height(text: &str, width: f32) -> f32 {
    const APPROX_GLYPH_WIDTH_PX: f32 = 14.0;
    const LINE_HEIGHT_PX: f32 = 22.0;
    let chars_per_line = (width / APPROX_GLYPH_WIDTH_PX).floor().max(1.0) as usize;
    let lines = text
        .split('\n')
        .map(|line| line.chars().count().max(1).div_ceil(chars_per_line))
        .sum::<usize>()
        .max(1);
    lines as f32 * LINE_HEIGHT_PX
}

fn ai_scrollbar_metrics(
    offset_y: f32,
    max_offset: f32,
    track_height: f32,
) -> Option<AiScrollbarMetrics> {
    if max_offset <= 0.0 || track_height <= 0.0 {
        return None;
    }
    let track_height = track_height.max(48.0);
    let thumb_height =
        (track_height * track_height / (track_height + max_offset)).clamp(24.0, track_height);
    let progress = (-offset_y / max_offset).clamp(0.0, 1.0);
    Some(AiScrollbarMetrics {
        thumb_top: (track_height - thumb_height) * progress,
        thumb_height,
    })
}

fn ai_preview_geometry(
    anchor: Bounds<gpui::Pixels>,
    kind: AiPreviewKind,
    viewport_width: f32,
    viewport_height: f32,
) -> AiPreviewGeometry {
    let is_inline = kind == AiPreviewKind::InlineCompletion;
    let desired_width = match kind {
        AiPreviewKind::InlineCompletion => 460.0,
        AiPreviewKind::SelectionRewrite => AI_PREVIEW_WIDTH_PX,
        AiPreviewKind::AssistantPanel => AI_ASSISTANT_PANEL_WIDTH_PX,
    };
    let width = desired_width.min((viewport_width - AI_VIEWPORT_MARGIN_PX * 2.0).max(220.0));
    let desired_y = if is_inline {
        f32::from(anchor.top())
    } else {
        f32::from(anchor.bottom()) + AI_PANEL_GAP_PX
    };
    let estimated_height = if kind == AiPreviewKind::AssistantPanel {
        AI_ASSISTANT_PANEL_HEIGHT_PX
    } else {
        AI_SELECTION_PANEL_HEIGHT_PX
    };
    let above_y = f32::from(anchor.top()) - estimated_height - AI_PANEL_GAP_PX;
    let min_panel_height = if is_inline { 24.0 } else { 80.0 };
    let max_top =
        (viewport_height - AI_VIEWPORT_MARGIN_PX - min_panel_height).max(AI_VIEWPORT_MARGIN_PX);
    let y = if !is_inline
        && desired_y + min_panel_height > viewport_height - AI_VIEWPORT_MARGIN_PX
        && above_y >= AI_VIEWPORT_MARGIN_PX
    {
        above_y
    } else {
        desired_y.clamp(AI_VIEWPORT_MARGIN_PX, max_top)
    };
    let max_x = (viewport_width - AI_VIEWPORT_MARGIN_PX - width).max(AI_VIEWPORT_MARGIN_PX);
    AiPreviewGeometry {
        x: f32::from(anchor.left()).clamp(AI_VIEWPORT_MARGIN_PX, max_x),
        y,
        width,
        max_height: (viewport_height - y - AI_VIEWPORT_MARGIN_PX).max(min_panel_height),
    }
}

fn ai_command_button(
    label: &'static str,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    div()
        .h(px(28.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(rgb(theme.hover_surface))
        .text_size(px(12.0))
        .text_color(rgb(theme.text))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.action_background)))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.submit_ai_prompt_instruction_from_gui(label, cx)
            });
            cx.stop_propagation();
        })
        .child(label)
        .into_any_element()
}

fn ai_preview_apply_button(
    label: &'static str,
    mode: AiApplyMode,
    primary: bool,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    div()
        .h(px(28.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(rgb(if primary {
            theme.action_background
        } else {
            theme.hover_surface
        }))
        .text_size(px(12.0))
        .text_color(rgb(if primary {
            theme.action_accent
        } else {
            theme.text
        }))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.action_hover_background)))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = view.update(cx, |view, cx| view.apply_ai_preview_from_gui(mode, cx));
            cx.stop_propagation();
        })
        .child(label)
        .into_any_element()
}

fn ai_preview_discard_button(theme: GuiTheme, view: Entity<CditorV2View>) -> AnyElement {
    div()
        .h(px(28.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(rgb(theme.hover_surface))
        .text_size(px(12.0))
        .text_color(rgb(theme.text))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.action_hover_background)))
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = view.update(cx, |view, cx| view.reject_ai_preview_from_gui(cx));
            cx.stop_propagation();
        })
        .child("丢弃")
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, size};

    #[test]
    fn assistant_panel_flips_above_when_anchor_is_near_viewport_bottom() {
        let geometry = ai_preview_geometry(
            Bounds::new(point(px(200.0), px(700.0)), size(px(1.0), px(24.0))),
            AiPreviewKind::AssistantPanel,
            1200.0,
            800.0,
        );
        assert!(geometry.y < 700.0);
        assert!(geometry.y + geometry.max_height <= 800.0);
    }

    #[test]
    fn preview_geometry_clamps_horizontal_overflow_and_narrow_viewports() {
        let geometry = ai_preview_geometry(
            Bounds::new(point(px(780.0), px(200.0)), size(px(1.0), px(24.0))),
            AiPreviewKind::AssistantPanel,
            800.0,
            600.0,
        );
        assert!(geometry.x >= AI_VIEWPORT_MARGIN_PX);
        assert!(geometry.x + geometry.width <= 800.0 - AI_VIEWPORT_MARGIN_PX);
        assert!(geometry.width <= 800.0 - AI_VIEWPORT_MARGIN_PX * 2.0);
    }

    #[test]
    fn long_streaming_content_has_visible_scrollbar_metrics_on_first_layout() {
        let text = (0..30)
            .map(|index| format!("第 {index} 行内容"))
            .collect::<Vec<_>>()
            .join("\n");
        let estimated_height = estimate_ai_content_height(&text, 600.0);
        assert!(estimated_height > 240.0);

        let metrics = ai_scrollbar_metrics(0.0, estimated_height - 240.0, 240.0).unwrap();
        assert_eq!(metrics.thumb_top, 0.0);
        assert!(metrics.thumb_height >= 24.0);
        assert!(metrics.thumb_height < 240.0);
        assert!(ai_scrollbar_metrics(0.0, 0.0, 240.0).is_none());
    }

    #[test]
    fn panel_preview_prefers_block_anchor_over_stale_text_layout() {
        let text_anchor = Bounds::new(point(px(200.0), px(700.0)), size(px(1.0), px(24.0)));
        let block_anchor = Bounds::new(point(px(200.0), px(300.0)), size(px(720.0), px(36.0)));

        assert_eq!(
            preferred_ai_preview_anchor(
                AiPreviewKind::AssistantPanel,
                Some(text_anchor),
                Some(block_anchor),
            ),
            Some(block_anchor)
        );
        assert_eq!(
            preferred_ai_preview_anchor(
                AiPreviewKind::SelectionRewrite,
                Some(text_anchor),
                Some(block_anchor),
            ),
            Some(block_anchor)
        );
        assert_eq!(
            preferred_ai_preview_anchor(
                AiPreviewKind::InlineCompletion,
                Some(text_anchor),
                Some(block_anchor),
            ),
            Some(text_anchor)
        );
    }

    #[test]
    fn preview_panel_height_is_fixed_and_viewport_clamped() {
        assert_eq!(ai_panel_height(AiPreviewKind::AssistantPanel, 700.0), 320.0);
        assert_eq!(
            ai_panel_height(AiPreviewKind::SelectionRewrite, 700.0),
            240.0
        );
        assert_eq!(ai_panel_height(AiPreviewKind::AssistantPanel, 160.0), 160.0);
        assert_eq!(ai_panel_height(AiPreviewKind::SelectionRewrite, 60.0), 80.0);
    }
}
