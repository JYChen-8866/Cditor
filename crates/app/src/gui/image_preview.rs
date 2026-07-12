use std::sync::Arc;
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnimationExt, AnyElement, App, BoxShadow, Global, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, Pixels, RenderImage, Size, Styled, Window, div, px, rgb, size,
};

use crate::gui::image_loader::RasterImageElement;

const PREVIEW_CLOSE_DURATION: Duration = Duration::from_millis(150);
const PREVIEW_MAX_VIEWPORT_RATIO: f32 = 0.88;
const PREVIEW_OVERLAY_OPACITY: f32 = 0.72;
const PREVIEW_FRAME_RADIUS_PX: f32 = 4.0;

pub struct ActiveImagePreview {
    image: Option<Arc<RenderImage>>,
    closing: bool,
    close_on_click_outside: bool,
    close_on_escape: bool,
}

impl Global for ActiveImagePreview {}

pub fn open_image_preview(
    image: Arc<RenderImage>,
    close_on_click_outside: bool,
    close_on_escape: bool,
    cx: &mut App,
) {
    if !cx.has_global::<ActiveImagePreview>() {
        cx.set_global(ActiveImagePreview {
            image: None,
            closing: false,
            close_on_click_outside,
            close_on_escape,
        });
    }
    let preview = cx.global_mut::<ActiveImagePreview>();
    preview.image = Some(image);
    preview.closing = false;
    preview.close_on_click_outside = close_on_click_outside;
    preview.close_on_escape = close_on_escape;
    cx.refresh_windows();
}

pub fn close_active_preview_if_escape_enabled(cx: &mut App) -> bool {
    let close_on_escape = cx
        .try_global::<ActiveImagePreview>()
        .is_some_and(|preview| preview.image.is_some() && preview.close_on_escape);
    if close_on_escape {
        close_active_preview(cx);
    }
    close_on_escape
}

pub fn close_active_preview(cx: &mut App) {
    if !cx.has_global::<ActiveImagePreview>() {
        return;
    }
    let preview = cx.global_mut::<ActiveImagePreview>();
    if preview.image.is_none() || preview.closing {
        return;
    }
    preview.closing = true;
    cx.refresh_windows();

    let async_cx = cx.to_async();
    let executor = cx.background_executor().clone();
    cx.foreground_executor()
        .spawn(async move {
            executor.timer(preview_close_duration()).await;
            let _ = async_cx.update(|cx| {
                if cx.has_global::<ActiveImagePreview>() {
                    let preview = cx.global_mut::<ActiveImagePreview>();
                    if preview.closing {
                        preview.image = None;
                        preview.closing = false;
                        cx.refresh_windows();
                    }
                }
            });
        })
        .detach();
}

pub fn render_image_preview_overlay(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let (image, closing, close_on_click_outside, close_on_escape) =
        cx.try_global::<ActiveImagePreview>().and_then(|preview| {
            preview.image.clone().map(|image| {
                (
                    image,
                    preview.closing,
                    preview.close_on_click_outside,
                    preview.close_on_escape,
                )
            })
        })?;

    let viewport = window.viewport_size();
    let preview_size = preview_image_box_size(
        &image,
        viewport.width * PREVIEW_MAX_VIEWPORT_RATIO,
        viewport.height * PREVIEW_MAX_VIEWPORT_RATIO,
    );
    let overlay_id = if closing {
        "liora-preview-overlay-exit"
    } else {
        "liora-preview-overlay-enter"
    };
    let frame_id = if closing {
        "liora-preview-frame-exit"
    } else {
        "liora-preview-frame-enter"
    };
    let overlay_direction = if closing {
        FadeDirection::Out
    } else {
        FadeDirection::In
    };

    Some(
        fade(
            overlay_id,
            overlay_direction,
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::black().opacity(PREVIEW_OVERLAY_OPACITY))
                .when(close_on_click_outside, |s| {
                    s.on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        close_active_preview(cx);
                        cx.stop_propagation();
                    })
                })
                .when(close_on_escape, |s| s)
                .child(pop_in(
                    frame_id,
                    div()
                        .w(preview_size.width)
                        .h(preview_size.height)
                        .rounded(px(PREVIEW_FRAME_RADIUS_PX))
                        .overflow_hidden()
                        .shadow(preview_image_frame_shadow())
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .child(RasterImageElement::new(
                            image,
                            ObjectFit::Contain,
                            px(PREVIEW_FRAME_RADIUS_PX),
                        )),
                ))
                .child(
                    div()
                        .absolute()
                        .top(px(16.0))
                        .right(px(16.0))
                        .size(px(32.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0x252525))
                        .text_color(rgb(0xffffff))
                        .text_size(px(14.0))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x454545)))
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            close_active_preview(cx);
                            cx.stop_propagation();
                        })
                        .child("X"),
                ),
        )
        .into_any_element(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FadeDirection {
    In,
    Out,
}

fn fade<E>(id: &'static str, direction: FadeDirection, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(
        id,
        gpui::Animation::new(preview_close_duration()),
        move |element, delta| {
            let opacity = match direction {
                FadeDirection::In => delta,
                FadeDirection::Out => 1.0 - delta,
            };
            element.opacity(opacity)
        },
    )
}

fn pop_in<E>(id: &'static str, element: E) -> impl IntoElement
where
    E: Styled + IntoElement + 'static,
{
    element.with_animation(
        id,
        gpui::Animation::new(Duration::from_millis(250)),
        |element, delta| element.opacity(0.86 + delta * 0.14),
    )
}

fn preview_close_duration() -> Duration {
    PREVIEW_CLOSE_DURATION
}

fn preview_image_box_size(
    image: &RenderImage,
    max_width: Pixels,
    max_height: Pixels,
) -> Size<Pixels> {
    let image_size = image.size(0);
    let image_width = i32::from(image_size.width).max(1) as f32;
    let image_height = i32::from(image_size.height).max(1) as f32;
    let scale = (f32::from(max_width) / image_width)
        .min(f32::from(max_height) / image_height)
        .min(1.0);

    size(px(image_width * scale), px(image_height * scale))
}

fn preview_image_frame_shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: gpui::black().opacity(0.32),
            offset: gpui::point(px(0.0), px(16.0)),
            blur_radius: px(48.0),
            spread_radius: px(0.0),
            inset: false,
        },
        BoxShadow {
            color: gpui::black().opacity(0.18),
            offset: gpui::point(px(0.0), px(2.0)),
            blur_radius: px(8.0),
            spread_radius: px(0.0),
            inset: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_render_image(width: u32, height: u32) -> RenderImage {
        RenderImage::new([::image::Frame::new(::image::RgbaImage::new(width, height))])
    }

    #[test]
    fn preview_image_box_size_matches_contained_image_bounds() {
        let wide = test_render_image(400, 200);
        let wide_size = preview_image_box_size(&wide, px(300.0), px(300.0));
        assert_eq!(wide_size.width, px(300.0));
        assert_eq!(wide_size.height, px(150.0));

        let tall = test_render_image(200, 400);
        let tall_size = preview_image_box_size(&tall, px(300.0), px(300.0));
        assert_eq!(tall_size.width, px(150.0));
        assert_eq!(tall_size.height, px(300.0));
    }

    #[test]
    fn preview_frame_shadow_uses_quiet_notion_elevation() {
        let shadow = preview_image_frame_shadow();

        assert_eq!(shadow.len(), 2);
        assert_eq!(shadow[0].offset.y, px(16.0));
        assert_eq!(shadow[0].blur_radius, px(48.0));
        assert_eq!(shadow[1].offset.y, px(2.0));
    }

    #[test]
    fn preview_does_not_upscale_small_images() {
        let small = test_render_image(120, 80);
        let preview = preview_image_box_size(&small, px(600.0), px(600.0));

        assert_eq!(preview, size(px(120.0), px(80.0)));
    }
}
