use cditor_core::ids::BlockId;
use cditor_core::layout::{
    BODY_BLOCK_CONTENT_WIDTH_PX, COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX,
    MERMAID_LOADING_PREVIEW_BODY_HEIGHT_PX, MERMAID_SOURCE_CHROME_HEIGHT_PX,
    MERMAID_SOURCE_PADDING_Y_PX, MERMAID_TOOLBAR_HEIGHT_PX as MERMAID_TOOLBAR_HEIGHT_PX_F64,
    V1_CODE_TEXT_LINE_HEIGHT_PX,
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Entity, ImageSource, InteractiveElement, IntoElement, ParentElement,
    RenderImage, Styled, div, img, px, rgb, rgba,
};

use crate::block::chrome::BLOCK_CONTENT_BORDER_WIDTH_PX;
use crate::editor_view::CditorV2View;
use crate::features::media::schedule_rendered_media_height_report;
use crate::image_preview::open_image_preview;
use crate::theme::GuiTheme;

use super::cache::MermaidRenderDimensions;
use super::{MermaidRenderCache, MermaidRenderStatus};

/// 工具栏与源码内边距由 core 定义：布局按同样的 chrome 预留高度。
const MERMAID_TOOLBAR_HEIGHT_PX: f32 = MERMAID_TOOLBAR_HEIGHT_PX_F64 as f32;
const MERMAID_SOURCE_PADDING_PX: f32 = MERMAID_SOURCE_PADDING_Y_PX as f32;
const MERMAID_PREVIEW_PADDING_X_PX: f32 = 22.0;
const MERMAID_PREVIEW_PADDING_Y_PX: f32 = 32.0;
const MERMAID_FRAME_RADIUS_PX: f32 = 10.0;
const MERMAID_FRAME_BORDER_WIDTH_PX: f32 = 1.0;
const MERMAID_LOADING_BODY_HEIGHT_PX: f32 = MERMAID_LOADING_PREVIEW_BODY_HEIGHT_PX as f32;
/// 源码区至少一行，之后跟着内容长高——和代码块一样，不设默认高度。
const MERMAID_SOURCE_MIN_BODY_HEIGHT_PX: f32 = V1_CODE_TEXT_LINE_HEIGHT_PX as f32;
const MERMAID_MAX_IMAGE_HEIGHT_PX: f32 = 1200.0;
const MERMAID_MAX_IMAGE_WIDTH_PX: f32 = BODY_BLOCK_CONTENT_WIDTH_PX as f32
    - BLOCK_CONTENT_BORDER_WIDTH_PX * 2.0
    - MERMAID_PREVIEW_PADDING_X_PX * 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct MermaidPreviewGeometry {
    image_width_px: f32,
    image_height_px: f32,
    body_height_px: f32,
    block_height_px: f64,
}

#[expect(clippy::too_many_arguments, reason = "P4-002 render context 聚合")]
pub(crate) fn render_mermaid_block(
    block_id: BlockId,
    content_version: u64,
    layout_height_px: f64,
    source_block_height_px: f64,
    source_content: AnyElement,
    show_source: bool,
    cache: &MermaidRenderCache,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    // When Some, a source<->preview switch animation is in flight.
    // Value is the *total block* height we should drive right now.
    // Content subtree for the active side (source or preview) stays mounted
    // and is clipped; only on stable (no tween) do we use pure min_h / geometry h.
    animated_block_height: Option<f64>,
    cx: &mut App,
) -> AnyElement {
    let toggle_view = view.clone();
    let status = cache.status(block_id);
    let geometry = (!show_source)
        .then(|| {
            cache
                .preview_dimensions(block_id, content_version, theme)
                .map(mermaid_preview_geometry_for_dimensions)
        })
        .flatten();

    let has_animated = animated_block_height.map_or(false, |h| h > 0.5);
    // During animation we feed the tween height from the driver (advance_*).
    // Do not emit a stable report that would fight the tween.
    //
    // For preview: the rendered image geometry owns the block height.
    // For source editing ("编辑模式"): the live text surface layout measurement
    // owns the height (see accept_text_layout). We must NOT push the estimate
    // here on every render, otherwise typing will cause repeated estimate-vs-measured
    // corrections that make the document below jump and flicker.
    if !has_animated && !show_source {
        if let Some(measured_height) =
            mermaid_height_report(show_source, geometry, source_block_height_px)
        {
            schedule_rendered_media_height_report(
                view,
                block_id,
                content_version,
                measured_height,
                cx,
            );
        }
    }

    // Animated body (area under toolbar). Subtract toolbar+shell chrome.
    let animated_body_h = animated_block_height.map(|total| {
        (total - f64::from(MERMAID_TOOLBAR_HEIGHT_PX) - COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX)
            .max(0.0) as f32
    });

    let (body, body_height) = if show_source {
        (
            source_content,
            animated_body_h.unwrap_or_else(|| mermaid_source_body_height(source_block_height_px)),
        )
    } else {
        (
            render_preview(status, source_content, theme, geometry),
            animated_body_h.unwrap_or_else(|| {
                geometry
                    .map(|geometry| geometry.body_height_px)
                    .unwrap_or_else(|| mermaid_body_height_for_layout(layout_height_px))
            }),
        )
    };

    let frame_background = (theme.text << 8) | 0x08;
    let frame = div()
        .id(("mermaid-block", block_id))
        .relative()
        .w_full()
        // Stable preview uses h_full reservation; during animation or source we do not.
        .when(!show_source && !has_animated, |frame| frame.h_full())
        .rounded(px(MERMAID_FRAME_RADIUS_PX))
        .border(px(MERMAID_FRAME_BORDER_WIDTH_PX))
        .border_color(rgb(theme.border))
        .bg(rgba(frame_background))
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
                            toggle_view.update(cx, |view, cx| {
                                super::actions::toggle_source_from_gui(view, block_id, cx);
                            });
                            cx.stop_propagation();
                        }),
                ),
        );
    frame
        .child(
            div()
                .w_full()
                // Animation: explicit h + clip, subtree stays mounted.
                // Stable source: min_h (grows with lines).
                // Stable preview: fixed h from geometry (or layout fallback).
                .when(has_animated, |b| {
                    b.h(px(body_height.max(0.0)))
                        .overflow_hidden()
                        .p(px(MERMAID_SOURCE_PADDING_PX))
                })
                .when(!has_animated && show_source, |source| {
                    // In source editing mode ("编辑模式"), do not pin the body to a
                    // pre-computed estimate. Let the inner text surface determine its
                    // natural size (like a code block). The block's effective height is
                    // kept in sync via the text layout measurement path in accept_text_layout.
                    // Only reserve a minimal floor so an empty block doesn't collapse.
                    source
                        .min_h(px(MERMAID_SOURCE_MIN_BODY_HEIGHT_PX))
                        .p(px(MERMAID_SOURCE_PADDING_PX))
                })
                .when(!has_animated && !show_source, |preview| {
                    preview
                        .h(px(body_height))
                        .px(px(MERMAID_PREVIEW_PADDING_X_PX))
                        .py(px(MERMAID_PREVIEW_PADDING_Y_PX))
                        .overflow_hidden()
                })
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
            open_image_preview(preview_image.clone(), cx);
            cx.stop_propagation();
        });
    if opacity < 1.0 {
        preview = preview.opacity(opacity);
    }
    preview
        .child(
            img(ImageSource::Render(image))
                .w(px(geometry.image_width_px))
                .h(px(geometry.image_height_px)),
        )
        .into_any_element()
}

#[cfg(test)]
fn mermaid_preview_geometry(image: &RenderImage) -> MermaidPreviewGeometry {
    let size = image.size(0);
    mermaid_preview_geometry_for_dimensions(MermaidRenderDimensions {
        width: i32::from(size.width).max(1) as u32,
        height: i32::from(size.height).max(1) as u32,
    })
}

fn mermaid_preview_geometry_for_dimensions(
    dimensions: MermaidRenderDimensions,
) -> MermaidPreviewGeometry {
    let natural_width = dimensions.width.max(1) as f32;
    let natural_height = dimensions.height.max(1) as f32;
    let scale = (MERMAID_MAX_IMAGE_WIDTH_PX / natural_width)
        .min(MERMAID_MAX_IMAGE_HEIGHT_PX / natural_height)
        .min(1.0);
    let image_width_px = natural_width * scale;
    let image_height_px = natural_height * scale;
    let body_height_px = image_height_px + MERMAID_PREVIEW_PADDING_Y_PX * 2.0;
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

/// 预览加载盒的块高度，仅用于和 core 的 Mermaid 估算对齐（见测试）。
#[cfg(test)]
fn default_mermaid_block_height_px() -> f64 {
    f64::from(
        MERMAID_TOOLBAR_HEIGHT_PX
            + MERMAID_LOADING_BODY_HEIGHT_PX
            + COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX as f32,
    )
}

fn mermaid_body_height_for_layout(layout_height_px: f64) -> f32 {
    (layout_height_px - f64::from(MERMAID_TOOLBAR_HEIGHT_PX) - COMPLEX_BLOCK_SHELL_CHROME_HEIGHT_PX)
        .max(1.0) as f32
}

fn mermaid_height_report(
    show_source: bool,
    geometry: Option<MermaidPreviewGeometry>,
    source_block_height_px: f64,
) -> Option<f64> {
    if show_source {
        Some(source_block_height_px)
    } else {
        geometry.map(|geometry| geometry.block_height_px)
    }
}

/// 源码区自身的高度 = 块高度减去 frame chrome。
fn mermaid_source_body_height(source_block_height_px: f64) -> f32 {
    (source_block_height_px - MERMAID_SOURCE_CHROME_HEIGHT_PX).max(0.0) as f32
}

fn concise_error(message: &str) -> &str {
    message.lines().next().unwrap_or("未知错误")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mermaid_frame_uses_the_prototype_radius() {
        assert_eq!(MERMAID_FRAME_RADIUS_PX, 10.0);
        assert_eq!(MERMAID_FRAME_BORDER_WIDTH_PX, 1.0);
    }

    #[test]
    fn mermaid_toolbar_height_is_part_of_source_and_preview_geometry() {
        assert_eq!(MERMAID_TOOLBAR_HEIGHT_PX, 28.0);
        // 预览还在渲染时用稳定的加载盒；这个高度与 core 的 Mermaid 估算一致。
        assert_eq!(
            default_mermaid_block_height_px(),
            cditor_core::layout::estimate_kind_fallback_height(
                &cditor_core::rich_text::RichBlockKind::Mermaid
            )
            .height
        );
    }

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
        let image = test_render_image(1404, 600);
        let geometry = mermaid_preview_geometry(&image);

        assert_eq!(MERMAID_MAX_IMAGE_WIDTH_PX, 754.0);
        assert!((geometry.image_width_px - 754.0).abs() < 0.001);
        assert!((geometry.image_height_px - 322.222_23).abs() < 0.001);
        assert!((geometry.body_height_px - 386.222_23).abs() < 0.001);
        assert!((geometry.block_height_px - 430.222_23).abs() < 0.001);
    }

    #[test]
    fn small_preview_keeps_its_intrinsic_size_instead_of_stretching() {
        let image = test_render_image(140, 159);
        let geometry = mermaid_preview_geometry(&image);

        assert_eq!(geometry.image_width_px, 140.0);
        assert_eq!(geometry.image_height_px, 159.0);
        assert_eq!(geometry.body_height_px, 223.0);
        assert_eq!(geometry.block_height_px, 267.0);
    }

    #[test]
    fn extremely_tall_preview_is_bounded_without_distortion() {
        let image = test_render_image(400, 2400);
        let geometry = mermaid_preview_geometry(&image);

        assert_eq!(geometry.image_width_px, 200.0);
        assert_eq!(geometry.image_height_px, MERMAID_MAX_IMAGE_HEIGHT_PX);
        assert_eq!(geometry.image_height_px / geometry.image_width_px, 6.0);
        assert_eq!(geometry.block_height_px, 1308.0);
    }

    #[test]
    fn source_mode_body_height_follows_the_source_instead_of_a_fixed_box() {
        let one_line = cditor_core::layout::estimate_mermaid_source_block_height_px(
            "flowchart LR",
            BODY_BLOCK_CONTENT_WIDTH_PX,
        );
        let three_lines = cditor_core::layout::estimate_mermaid_source_block_height_px(
            "flowchart LR\n  A --> B\n  B --> C",
            BODY_BLOCK_CONTENT_WIDTH_PX,
        );

        // 源码区高度 = 块高度 - frame chrome，并且随行数增长。
        assert_eq!(
            mermaid_source_body_height(one_line),
            V1_CODE_TEXT_LINE_HEIGHT_PX as f32
        );
        assert_eq!(
            mermaid_source_body_height(three_lines) - mermaid_source_body_height(one_line),
            (V1_CODE_TEXT_LINE_HEIGHT_PX * 2.0) as f32
        );
        assert_ne!(
            mermaid_source_body_height(one_line),
            MERMAID_LOADING_BODY_HEIGHT_PX
        );
    }

    #[test]
    fn loading_preview_preserves_the_existing_layout_without_reporting_an_estimate() {
        assert_eq!(mermaid_height_report(false, None, 96.0), None);
        assert_eq!(mermaid_body_height_for_layout(430.0), 386.0);
    }

    #[test]
    fn only_source_mode_or_completed_preview_reports_a_height() {
        let geometry = MermaidPreviewGeometry {
            image_width_px: 320.0,
            image_height_px: 180.0,
            body_height_px: 244.0,
            block_height_px: 288.0,
        };

        // 源码模式报告的是内容算出来的高度，预览模式报告图片几何。
        assert_eq!(mermaid_height_report(true, None, 96.0), Some(96.0));
        assert_eq!(
            mermaid_height_report(true, Some(geometry), 96.0),
            Some(96.0)
        );
        assert_eq!(
            mermaid_height_report(false, Some(geometry), 96.0),
            Some(288.0)
        );
    }
}

#[cfg(test)]
mod source_geometry_tests {
    use super::*;
    use crate::document::block_tracks::{
        MERMAID_SOURCE_TEXT_OFFSET_TOP_PX, MERMAID_SOURCE_TEXT_OFFSET_X_PX,
    };

    #[test]
    fn projected_source_text_offsets_match_the_frame_chrome() {
        // The projected text geometry (IME candidate placement, hit testing,
        // shaping width) must model exactly the chrome this renderer draws
        // around the source text.
        assert_eq!(
            MERMAID_SOURCE_TEXT_OFFSET_X_PX,
            MERMAID_FRAME_BORDER_WIDTH_PX + MERMAID_SOURCE_PADDING_PX
        );
        assert_eq!(
            MERMAID_SOURCE_TEXT_OFFSET_TOP_PX,
            MERMAID_FRAME_BORDER_WIDTH_PX + MERMAID_TOOLBAR_HEIGHT_PX + MERMAID_SOURCE_PADDING_PX
        );
    }
}
