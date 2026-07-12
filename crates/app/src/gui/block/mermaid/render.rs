use cditor_core::ids::BlockId;
use gpui::{
    AnyElement, Entity, ImageSource, InteractiveElement, IntoElement, ParentElement, Styled, div,
    img, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::image_preview::open_image_preview;

use super::{MermaidRenderCache, MermaidRenderStatus};

const MERMAID_PREVIEW_HEIGHT_PX: f32 = 188.0;

pub(crate) fn render_mermaid_block(
    block_id: BlockId,
    source_content: AnyElement,
    show_source: bool,
    cache: &MermaidRenderCache,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    let toggle_view = view.clone();
    let body = if show_source {
        source_content
    } else {
        render_preview(block_id, source_content, cache, theme)
    };

    div()
        .id(("mermaid-block", block_id))
        .relative()
        .w_full()
        .h_full()
        .rounded(px(8.0))
        .bg(rgb(theme.code_background))
        .overflow_hidden()
        .child(
            div()
                .h(px(28.0))
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .px(px(8.0))
                .text_size(px(11.0))
                .text_color(rgb(theme.muted))
                .child("Mermaid")
                .child(
                    div()
                        .id(("mermaid-source-toggle", block_id))
                        .cursor_pointer()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .hover(|style| style.bg(rgb(theme.hover_surface)))
                        .child(if show_source { "预览" } else { "源码" })
                        .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, cx| {
                            let _ = toggle_view.update(cx, |view, cx| {
                                view.toggle_mermaid_source_from_gui(block_id, cx);
                            });
                            cx.stop_propagation();
                        }),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(MERMAID_PREVIEW_HEIGHT_PX))
                .p(px(8.0))
                .overflow_hidden()
                .child(body),
        )
        .into_any_element()
}

fn render_preview(
    block_id: BlockId,
    source_content: AnyElement,
    cache: &MermaidRenderCache,
    theme: GuiTheme,
) -> AnyElement {
    match cache.status(block_id) {
        Some(MermaidRenderStatus::Ready(image)) => clickable_preview(image, 1.0),
        Some(MermaidRenderStatus::Rendering {
            fallback: Some(image),
        }) => clickable_preview(image, 0.65),
        Some(MermaidRenderStatus::Failed { message }) => div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme.danger))
                    .child(format!("渲染失败：{}", concise_error(&message))),
            )
            .child(source_content)
            .into_any_element(),
        Some(MermaidRenderStatus::Rendering { fallback: None }) | None => div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme.muted))
                    .child("正在渲染 Mermaid…"),
            )
            .child(source_content)
            .into_any_element(),
    }
}

fn clickable_preview(image: std::sync::Arc<gpui::RenderImage>, opacity: f32) -> AnyElement {
    let preview_image = image.clone();
    let mut preview = div()
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, cx| {
            open_image_preview(preview_image.clone(), true, true, cx);
            cx.stop_propagation();
        });
    if opacity < 1.0 {
        preview = preview.opacity(opacity);
    }
    preview
        .child(
            img(ImageSource::Render(image.clone()))
                .max_w_full()
                .max_h(px(MERMAID_PREVIEW_HEIGHT_PX - 16.0)),
        )
        .into_any_element()
}

fn concise_error(message: &str) -> &str {
    message.lines().next().unwrap_or("未知错误")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_summary_uses_only_the_first_line() {
        assert_eq!(concise_error("parse failed\nstack detail"), "parse failed");
        assert_eq!(concise_error(""), "未知错误");
    }
}
