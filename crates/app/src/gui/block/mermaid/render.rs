use cditor_core::ids::BlockId;
use cditor_core::layout::COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX;
use gpui::{
    AnyElement, App, Entity, ImageSource, InteractiveElement, IntoElement, ParentElement,
    RenderImage, Styled, div, img, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::chrome::{
    BLOCK_GUTTER_WIDTH_PX, BLOCK_PREFIX_WIDTH_PX, BLOCK_ROW_GAP_PX, BLOCK_SHELL_OUTER_PADDING_X_PX,
};
use crate::gui::block::media::schedule_rendered_media_height_report;
use crate::gui::document::DEFAULT_DOCUMENT_CONTENT_WIDTH_PX;
use crate::gui::image_preview::open_image_preview;

use super::{MermaidRenderCache, MermaidRenderStatus};

const MERMAID_TOOLBAR_HEIGHT_PX: f32 = 28.0;
const MERMAID_BODY_PADDING_PX: f32 = 8.0;
const MERMAID_LOADING_BODY_HEIGHT_PX: f32 = 188.0;
const MERMAID_MAX_IMAGE_HEIGHT_PX: f32 = 1200.0;
const MERMAID_MAX_IMAGE_WIDTH_PX: f32 = DEFAULT_DOCUMENT_CONTENT_WIDTH_PX
    - BLOCK_SHELL_OUTER_PADDING_X_PX * 2.0
    - BLOCK_GUTTER_WIDTH_PX
    - BLOCK_ROW_GAP_PX
    - BLOCK_PREFIX_WIDTH_PX
    - MERMAID_BODY_PADDING_PX * 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct MermaidPreviewGeometry {
    image_width_px: f32,
    image_height_px: f32,
    body_height_px: f32,
    block_height_px: f64,
}

pub(crate) fn render_mermaid_block(
    block_id: BlockId,
    content_version: u64,
    source_content: AnyElement,
    show_source: bool,
    cache: &MermaidRenderCache,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    cx: &mut App,
) -> AnyElement {
    let toggle_view = view.clone();
    let status = cache.status(block_id);
    let geometry = (!show_source)
        .then(|| status.as_ref().and_then(preview_geometry_for_status))
        .flatten();
    schedule_rendered_media_height_report(
        view.clone(),
        block_id,
        content_version,
        geometry
            .map(|geometry| geometry.block_height_px)
            .unwrap_or_else(default_mermaid_block_height_px),
        cx,
    );
    let (body, body_height) = if show_source {
        (source_content, MERMAID_LOADING_BODY_HEIGHT_PX)
    } else {
        (
            render_preview(status, source_content, theme, geometry),
            geometry
                .map(|geometry| geometry.body_height_px)
                .unwrap_or(MERMAID_LOADING_BODY_HEIGHT_PX),
        )
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
                .h(px(MERMAID_TOOLBAR_HEIGHT_PX))
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
                .h(px(body_height))
                .p(px(MERMAID_BODY_PADDING_PX))
                .overflow_hidden()
                .child(body),
        )
        .into_any_element()
}

fn render_preview(
    status: Option<MermaidRenderStatus>,
    source_content: AnyElement,
    theme: GuiTheme,
    geometry: Option<MermaidPreviewGeometry>,
) -> AnyElement {
    match status {
        Some(MermaidRenderStatus::Ready(image)) => {
            clickable_preview(image, geometry.expect("ready image has geometry"), 1.0)
        }
        Some(MermaidRenderStatus::Rendering {
            fallback: Some(image),
        }) => clickable_preview(image, geometry.expect("fallback image has geometry"), 0.65),
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

fn clickable_preview(
    image: std::sync::Arc<RenderImage>,
    geometry: MermaidPreviewGeometry,
    opacity: f32,
) -> AnyElement {
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
                .w(px(geometry.image_width_px))
                .h(px(geometry.image_height_px)),
        )
        .into_any_element()
}

fn preview_geometry_for_status(status: &MermaidRenderStatus) -> Option<MermaidPreviewGeometry> {
    match status {
        MermaidRenderStatus::Ready(image)
        | MermaidRenderStatus::Rendering {
            fallback: Some(image),
        } => Some(mermaid_preview_geometry(image)),
        MermaidRenderStatus::Rendering { fallback: None } | MermaidRenderStatus::Failed { .. } => {
            None
        }
    }
}

fn mermaid_preview_geometry(image: &RenderImage) -> MermaidPreviewGeometry {
    let size = image.size(0);
    let natural_width = i32::from(size.width).max(1) as f32;
    let natural_height = i32::from(size.height).max(1) as f32;
    let scale = (MERMAID_MAX_IMAGE_WIDTH_PX / natural_width)
        .min(MERMAID_MAX_IMAGE_HEIGHT_PX / natural_height)
        .min(1.0);
    let image_width_px = natural_width * scale;
    let image_height_px = natural_height * scale;
    let body_height_px = image_height_px + MERMAID_BODY_PADDING_PX * 2.0;
    let block_height_px = f64::from(
        MERMAID_TOOLBAR_HEIGHT_PX + body_height_px + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX as f32,
    );
    MermaidPreviewGeometry {
        image_width_px,
        image_height_px,
        body_height_px,
        block_height_px,
    }
}

fn default_mermaid_block_height_px() -> f64 {
    f64::from(
        MERMAID_TOOLBAR_HEIGHT_PX
            + MERMAID_LOADING_BODY_HEIGHT_PX
            + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX as f32,
    )
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

    fn test_render_image(width: u32, height: u32) -> RenderImage {
        RenderImage::new([::image::Frame::new(::image::RgbaImage::new(width, height))])
    }

    #[test]
    fn preview_geometry_tracks_intrinsic_aspect_ratio_and_full_block_height() {
        let image = test_render_image(1496, 600);
        let geometry = mermaid_preview_geometry(&image);

        assert_eq!(MERMAID_MAX_IMAGE_WIDTH_PX, 748.0);
        assert_eq!(geometry.image_width_px, 748.0);
        assert_eq!(geometry.image_height_px, 300.0);
        assert_eq!(geometry.body_height_px, 316.0);
        assert_eq!(geometry.block_height_px, 360.0);
    }

    #[test]
    fn extremely_tall_preview_is_bounded_without_distortion() {
        let image = test_render_image(400, 2400);
        let geometry = mermaid_preview_geometry(&image);

        assert_eq!(geometry.image_width_px, 200.0);
        assert_eq!(geometry.image_height_px, MERMAID_MAX_IMAGE_HEIGHT_PX);
        assert_eq!(geometry.image_height_px / geometry.image_width_px, 6.0);
        assert_eq!(geometry.block_height_px, 1260.0);
    }

    #[test]
    fn loading_preview_height_matches_the_rendered_fixed_box() {
        assert_eq!(default_mermaid_block_height_px(), 232.0);
    }
}
