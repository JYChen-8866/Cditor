use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use gpui::{
    AnyElement, App, Entity, InteractiveElement, IntoElement, MouseButton, ObjectFit,
    ParentElement, RenderImage, Styled, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::image_loader::{RasterImageElement, load_render_image};
use crate::gui::image_preview::open_image_preview;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::ImagePayload;

const NOTE_IMAGE_MAX_WIDTH_PX: f32 = 704.0;
const DEFAULT_IMAGE_MAX_WIDTH_PX: f32 = 560.0;
const MIN_IMAGE_WIDTH_RATIO_MILLI: u16 = 200;
const MAX_IMAGE_WIDTH_RATIO_MILLI: u16 = 1000;
const V1_IMAGE_RADIUS_PX: f32 = 8.0;
const IMAGE_BLOCK_OUTER_CHROME_HEIGHT_PX: f32 = 16.0;
const IMAGE_RESIZE_HANDLE_SIZE_PX: f32 = 24.0;
const IMAGE_RESIZE_HANDLE_EDGE_GAP_PX: f32 = 4.0;
const IMAGE_RESIZE_HANDLE_DOT_ROWS: [usize; 3] = [1, 2, 3];
const IMAGE_RESIZE_HANDLE_ROW_HEIGHT_PX: f32 = 6.0;

pub fn render_image_block(
    block_id: BlockId,
    content_version: u64,
    image: &ImagePayload,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
    image_resize_preview_width_px: Option<f32>,
    cx: &mut App,
) -> AnyElement {
    let loaded = load_render_image(&image.source, cx);
    let display_size = loaded.as_deref().map(|render_image| {
        display_image_size_px(render_image, image, image_resize_preview_width_px)
    });
    if let Some((_, height)) = display_size {
        let measured_height = image_block_measured_height(height);
        schedule_rendered_media_height_report(
            view.clone(),
            block_id,
            content_version,
            measured_height,
            cx,
        );
    }

    let mut card = div()
        .rounded(px(V1_IMAGE_RADIUS_PX))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        .overflow_hidden();
    if let Some((width, _)) = display_size {
        card = card.w(px(width));
    } else {
        card = card.w_full();
    }

    let card = card.child(if let Some(render_image) = loaded {
        let preview_image = render_image.clone();
        let resize_view = view.clone();
        let (width, height) =
            display_image_size_px(&render_image, image, image_resize_preview_width_px);
        let max_width = max_resizable_image_width_px(&render_image);
        div()
            .relative()
            .w(px(width))
            .h(px(height))
            .group("image-resize")
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(theme.focused)).shadow_lg())
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                open_image_preview(preview_image.clone(), true, true, cx);
                cx.stop_propagation();
            })
            .child(RasterImageElement::new(
                render_image,
                ObjectFit::Contain,
                px(0.0),
            ))
            .child(render_image_resize_handle(
                block_id,
                width,
                max_width,
                theme,
                resize_view,
            ))
            .into_any_element()
    } else {
        render_image_placeholder(image, theme)
    });

    div()
        .w_full()
        .flex()
        .justify_center()
        .child(card)
        .into_any_element()
}

fn media_height_report_cache() -> &'static Mutex<HashMap<BlockId, (u64, u64)>> {
    static CACHE: OnceLock<Mutex<HashMap<BlockId, (u64, u64)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn schedule_rendered_media_height_report(
    view: Entity<CditorV2View>,
    block_id: BlockId,
    content_version: u64,
    measured_height: f64,
    cx: &mut App,
) {
    let height_key = (measured_height * 2.0).round().to_bits();
    let already_scheduled = media_height_report_cache()
        .lock()
        .ok()
        .and_then(|mut cache| {
            let next = (content_version, height_key);
            if cache.get(&block_id).copied() == Some(next) {
                Some(true)
            } else {
                cache.insert(block_id, next);
                Some(false)
            }
        })
        .unwrap_or(false);
    if already_scheduled {
        return;
    }

    let async_cx = cx.to_async();
    cx.foreground_executor()
        .spawn(async move {
            let _ = async_cx.update(|cx| {
                let _ = view.update(cx, |view, cx| {
                    if view.queue_rendered_media_height(
                        block_id,
                        content_version,
                        measured_height,
                        cx,
                    ) {
                        view.mark_dirty(cx);
                        cx.notify();
                    }
                });
            });
        })
        .detach();
}

fn natural_image_size_px(image: &RenderImage) -> (f32, f32) {
    let image_size = image.size(0);
    (
        i32::from(image_size.width).max(1) as f32,
        i32::from(image_size.height).max(1) as f32,
    )
}

fn max_resizable_image_width_px(image: &RenderImage) -> f32 {
    let (natural_width, _) = natural_image_size_px(image);
    natural_width.min(NOTE_IMAGE_MAX_WIDTH_PX).max(1.0)
}

fn default_display_image_width_px(image: &RenderImage) -> f32 {
    let (natural_width, _) = natural_image_size_px(image);
    natural_width.min(DEFAULT_IMAGE_MAX_WIDTH_PX).max(1.0)
}

fn image_width_from_ratio_px(image: &RenderImage, ratio_milli: u16) -> f32 {
    let ratio =
        f32::from(ratio_milli.clamp(MIN_IMAGE_WIDTH_RATIO_MILLI, MAX_IMAGE_WIDTH_RATIO_MILLI))
            / 1000.0;
    max_resizable_image_width_px(image) * ratio
}

fn display_image_size_px(
    image: &RenderImage,
    payload: &ImagePayload,
    preview_width_px: Option<f32>,
) -> (f32, f32) {
    let (natural_width, natural_height) = natural_image_size_px(image);
    let max_width = max_resizable_image_width_px(image);
    let display_width = preview_width_px
        .or_else(|| {
            payload
                .display_width_ratio_milli
                .map(|ratio| image_width_from_ratio_px(image, ratio))
        })
        .unwrap_or_else(|| default_display_image_width_px(image))
        .clamp(
            max_width * f32::from(MIN_IMAGE_WIDTH_RATIO_MILLI) / 1000.0,
            max_width,
        );
    let scale = display_width / natural_width.max(1.0);
    (display_width, natural_height * scale)
}

pub(crate) fn image_width_ratio_milli_for_width(width_px: f32, max_width_px: f32) -> u16 {
    if max_width_px <= 0.0 {
        return MAX_IMAGE_WIDTH_RATIO_MILLI;
    }
    ((width_px / max_width_px).clamp(
        f32::from(MIN_IMAGE_WIDTH_RATIO_MILLI) / 1000.0,
        f32::from(MAX_IMAGE_WIDTH_RATIO_MILLI) / 1000.0,
    ) * 1000.0)
        .round() as u16
}

fn render_image_resize_handle(
    block_id: BlockId,
    current_width_px: f32,
    max_width_px: f32,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    div()
        .absolute()
        .right(px(IMAGE_RESIZE_HANDLE_EDGE_GAP_PX))
        .bottom(px(IMAGE_RESIZE_HANDLE_EDGE_GAP_PX))
        .w(px(IMAGE_RESIZE_HANDLE_SIZE_PX))
        .h(px(IMAGE_RESIZE_HANDLE_SIZE_PX))
        .flex()
        .items_end()
        .justify_end()
        .opacity(0.0)
        .group_hover("image-resize", |s| s.opacity(0.9))
        .hover(|s| s.opacity(1.0))
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.start_image_resize_from_gui(
                    block_id,
                    current_width_px,
                    max_width_px,
                    event.position,
                    window,
                    cx,
                );
            });
            cx.stop_propagation();
        })
        .child(render_image_resize_triangle_grip(theme))
        .into_any_element()
}

fn render_image_resize_triangle_grip(theme: GuiTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_end()
        .justify_end()
        .children(
            IMAGE_RESIZE_HANDLE_DOT_ROWS
                .into_iter()
                .map(move |dots| render_image_resize_triangle_row(dots, theme)),
        )
        .into_any_element()
}

fn render_image_resize_triangle_row(dots: usize, theme: GuiTheme) -> AnyElement {
    let width = IMAGE_RESIZE_HANDLE_ROW_HEIGHT_PX * dots as f32;
    div()
        .w(px(width))
        .h(px(IMAGE_RESIZE_HANDLE_ROW_HEIGHT_PX))
        .flex()
        .items_center()
        .justify_end()
        .gap(px(2.0))
        .pr(px(2.0))
        .bg(rgb(theme.gutter_background))
        .group_hover("image-resize", move |s| {
            s.bg(rgb(theme.action_hover_background))
        })
        .children((0..dots).map(move |_| render_image_resize_grip_dot(theme)))
        .into_any_element()
}

fn render_image_resize_grip_dot(theme: GuiTheme) -> AnyElement {
    div()
        .w(px(2.5))
        .h(px(2.5))
        .rounded(px(2.0))
        .bg(rgb(theme.gutter_foreground))
        .group_hover("image-resize", move |s| s.bg(rgb(theme.action_accent)))
        .into_any_element()
}

fn image_block_measured_height(image_height_px: f32) -> f64 {
    f64::from(image_height_px + IMAGE_BLOCK_OUTER_CHROME_HEIGHT_PX)
}

fn render_image_placeholder(image: &ImagePayload, theme: GuiTheme) -> AnyElement {
    div()
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.0))
        .text_color(rgb(theme.muted))
        .child(if image.source.is_empty() {
            "Image".to_owned()
        } else {
            format!("Image: {}", image.source)
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_block_measured_height_uses_displayed_image_height_without_footer() {
        assert_eq!(
            image_block_measured_height(512.0),
            f64::from(512.0 + IMAGE_BLOCK_OUTER_CHROME_HEIGHT_PX)
        );
    }

    #[test]
    fn image_resize_handle_uses_triangular_dot_rows() {
        assert_eq!(IMAGE_RESIZE_HANDLE_DOT_ROWS, [1, 2, 3]);
        assert_eq!(IMAGE_RESIZE_HANDLE_DOT_ROWS.iter().sum::<usize>(), 6);
        assert_eq!(IMAGE_RESIZE_HANDLE_EDGE_GAP_PX, 4.0);
        assert_eq!(
            IMAGE_RESIZE_HANDLE_ROW_HEIGHT_PX * IMAGE_RESIZE_HANDLE_DOT_ROWS[2] as f32,
            18.0
        );
    }

    fn test_render_image(width: u32, height: u32) -> RenderImage {
        RenderImage::new([::image::Frame::new(::image::RgbaImage::new(width, height))])
    }

    #[test]
    fn display_image_size_scales_down_large_images_without_distortion_or_upscaling() {
        let default_payload = ImagePayload::default();
        let large = test_render_image(1408, 1000);
        let large_size = display_image_size_px(&large, &default_payload, None);
        assert_eq!(large_size.0, DEFAULT_IMAGE_MAX_WIDTH_PX);
        assert_eq!(large_size.1, DEFAULT_IMAGE_MAX_WIDTH_PX / 1408.0 * 1000.0);

        let resized_payload = ImagePayload {
            display_width_ratio_milli: Some(1000),
            ..ImagePayload::default()
        };
        let resized_size = display_image_size_px(&large, &resized_payload, None);
        assert_eq!(resized_size.0, NOTE_IMAGE_MAX_WIDTH_PX);
        assert_eq!(resized_size.1, 500.0);

        let small = test_render_image(320, 240);
        let small_size = display_image_size_px(&small, &default_payload, None);
        assert_eq!(small_size, (320.0, 240.0));
    }
}
