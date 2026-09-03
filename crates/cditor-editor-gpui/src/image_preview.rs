mod camera;

use std::sync::Arc;

use cditor_component::SvgIcon;
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, CursorStyle, Div, Global, InteractiveElement,
    IntoElement, MouseButton, ObjectFit, ParentElement, PinchEvent, Pixels, Render, RenderImage,
    ScrollDelta, ScrollWheelEvent, Size, StatefulInteractiveElement, Styled, Task, Window, div,
    point, px, rgb, size,
};

use crate::app::worker_admission::EditorWorkerAdmission;
use crate::image_loader::{PreviewImageLoad, RasterImageElement};
use crate::theme::active_theme;
use camera::{PREVIEW_SCROLL_LINE_PX, PREVIEW_ZOOM_STEP, PreviewCamera, wheel_zoom_factor};
use cditor_runtime::WorkerTaskKind;

const PREVIEW_CONTROL_SIZE_PX: f32 = 32.0;
const PREVIEW_CONTROL_ICON_SIZE_PX: f32 = 16.0;
const PREVIEW_CONTROLS_MARGIN_PX: f32 = 16.0;

const ICON_PREVIEW_FIT: &[u8] = include_bytes!("../../../assets/icons/preview-fit.svg");
const ICON_PREVIEW_RESET: &[u8] = include_bytes!("../../../assets/icons/preview-reset.svg");
const ICON_PREVIEW_PLUS: &[u8] = include_bytes!("../../../assets/icons/preview-plus.svg");
const ICON_PREVIEW_MINUS: &[u8] = include_bytes!("../../../assets/icons/preview-minus.svg");

pub struct ActiveImagePreview {
    content: PreviewContent,
    camera: PreviewCamera,
    camera_initialized: bool,
    load_task: Option<Task<()>>,
}

impl Global for ActiveImagePreview {}

pub fn open_image_preview(image: Arc<RenderImage>, cx: &mut App) {
    ensure_active_preview(cx);
    let (old_task, retired) = {
        let preview = cx.global_mut::<ActiveImagePreview>();
        let old_task = preview.load_task.take();
        let (_, retired) = preview.content.open(image, false);
        preview.camera = PreviewCamera::default();
        preview.camera_initialized = false;
        (old_task, retired)
    };
    drop(old_task);
    retire_preview_images(retired, cx);
    cx.refresh_windows();
}

pub(crate) fn open_source_image_preview(
    source: &str,
    display_fallback: Arc<RenderImage>,
    workers: EditorWorkerAdmission,
    asset_provider: Option<Arc<dyn cditor_sdk::providers::AssetProvider>>,
    cx: &mut App,
) {
    let Some(load) = PreviewImageLoad::capture(source, asset_provider, cx) else {
        open_image_preview(display_fallback, cx);
        return;
    };
    let Some(permit) = workers.try_acquire(WorkerTaskKind::ImageDecode) else {
        open_image_preview(display_fallback, cx);
        return;
    };

    ensure_active_preview(cx);
    let (old_task, generation, retired) = {
        let preview = cx.global_mut::<ActiveImagePreview>();
        let old_task = preview.load_task.take();
        let (generation, retired) = preview.content.open(display_fallback, false);
        preview.camera = PreviewCamera::default();
        preview.camera_initialized = false;
        (old_task, generation, retired)
    };
    drop(old_task);
    retire_preview_images(retired, cx);

    let async_cx = cx.to_async();
    let executor = cx.background_executor().clone();
    let task = cx.foreground_executor().spawn(async move {
        let image = executor
            .spawn(async move {
                let _permit = permit;
                load.decode().await
            })
            .await;
        if let Some(image) = image {
            async_cx.update(|cx| apply_loaded_preview_image(generation, image, cx));
        }
    });
    cx.global_mut::<ActiveImagePreview>().load_task = Some(task);
    cx.refresh_windows();
}

fn ensure_active_preview(cx: &mut App) {
    if !cx.has_global::<ActiveImagePreview>() {
        cx.set_global(ActiveImagePreview {
            content: PreviewContent::default(),
            camera: PreviewCamera::default(),
            camera_initialized: false,
            load_task: None,
        });
    }
}

pub fn close_active_preview_if_open(cx: &mut App) -> bool {
    let has_preview = cx
        .try_global::<ActiveImagePreview>()
        .is_some_and(|preview| preview.content.image.is_some());
    if has_preview {
        close_active_preview(cx);
    }
    has_preview
}

pub fn close_active_preview(cx: &mut App) {
    if !cx.has_global::<ActiveImagePreview>() {
        return;
    }
    let (old_task, retired, was_open) = {
        let preview = cx.global_mut::<ActiveImagePreview>();
        let was_open = preview.content.image.is_some();
        let old_task = preview.load_task.take();
        let retired = preview.content.close();
        preview.camera = PreviewCamera::default();
        preview.camera_initialized = false;
        (old_task, retired, was_open)
    };
    drop(old_task);
    retire_preview_images(retired, cx);
    if !was_open {
        return;
    }
    cx.refresh_windows();
}

pub fn render_image_preview_canvas(
    window: &mut Window,
    editor_viewport_bounds: Option<Bounds<Pixels>>,
    cx: &mut App,
) -> Option<AnyElement> {
    let theme = active_theme(cx);
    let image = cx
        .try_global::<ActiveImagePreview>()
        .and_then(|preview| preview.content.image.clone())?;

    let viewport = preview_viewport_bounds(editor_viewport_bounds, window.viewport_size());
    let natural_image_size = natural_image_size(&image);
    let camera = {
        let preview = cx.global_mut::<ActiveImagePreview>();
        if !preview.camera_initialized {
            preview
                .camera
                .fit_to_view(viewport.size, natural_image_size);
            preview.camera_initialized = true;
        }
        preview.camera
    };
    Some(
        div()
            .id("cditor-image-preview-canvas")
            .size_full()
            .bg(rgb(theme.surface))
            .child(preview_frame(
                &image,
                natural_image_size,
                viewport,
                camera,
                theme,
            ))
            .into_any_element(),
    )
}

fn preview_frame(
    image: &Arc<RenderImage>,
    natural_image_size: Size<f32>,
    viewport: Bounds<Pixels>,
    camera: PreviewCamera,
    theme: crate::theme::GuiTheme,
) -> Div {
    let image_frame = camera.image_frame(viewport.size, natural_image_size);
    div()
        .w(viewport.size.width)
        .h(viewport.size.height)
        .relative()
        .overflow_hidden()
        .cursor(if camera.is_dragging() {
            CursorStyle::ClosedHand
        } else {
            CursorStyle::Arrow
        })
        .on_scroll_wheel(move |event, _, cx| {
            handle_preview_scroll(event, viewport, cx);
            cx.stop_propagation();
        })
        .on_pinch(move |event: &PinchEvent, _, cx| {
            let factor = (1.0 + event.delta).max(0.01);
            update_active_camera(cx, |camera| {
                camera.zoom_at(event.position, viewport, factor)
            });
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Left, |event, _, cx| {
            begin_preview_drag(event.position, cx);
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_move(|event, _, cx| {
            if event.dragging() {
                update_active_camera(cx, |camera| camera.drag_to(event.position));
            } else {
                end_preview_drag(cx);
            }
            cx.stop_propagation();
        })
        .on_mouse_up(MouseButton::Left, |_, _, cx| {
            end_preview_drag(cx);
            cx.stop_propagation();
        })
        .on_mouse_up_out(MouseButton::Left, |_, _, cx| {
            end_preview_drag(cx);
            cx.stop_propagation();
        })
        .child(
            div()
                .absolute()
                .left(image_frame.origin.x)
                .top(image_frame.origin.y)
                .w(image_frame.size.width)
                .h(image_frame.size.height)
                .child(RasterImageElement::new(
                    image.clone(),
                    ObjectFit::Contain,
                    px(0.0),
                )),
        )
        .child(preview_controls(natural_image_size, viewport, theme))
}

fn handle_preview_scroll(event: &ScrollWheelEvent, viewport: Bounds<Pixels>, cx: &mut App) {
    if event.modifiers.control || event.modifiers.platform {
        let delta_y = match event.delta {
            ScrollDelta::Pixels(delta) => f32::from(delta.y),
            ScrollDelta::Lines(delta) => delta.y * PREVIEW_SCROLL_LINE_PX,
        };
        let factor = wheel_zoom_factor(delta_y);
        update_active_camera(cx, |camera| {
            camera.zoom_at(event.position, viewport, factor)
        });
    } else {
        let delta = event.delta.pixel_delta(px(PREVIEW_SCROLL_LINE_PX));
        update_active_camera(cx, |camera| camera.pan_by(delta));
    }
}

fn begin_preview_drag(position: gpui::Point<Pixels>, cx: &mut App) {
    let changed = if cx.has_global::<ActiveImagePreview>() {
        let preview = cx.global_mut::<ActiveImagePreview>();
        if preview.content.image.is_some() {
            preview.camera.begin_drag(position);
            true
        } else {
            false
        }
    } else {
        false
    };
    if changed {
        cx.refresh_windows();
    }
}

fn end_preview_drag(cx: &mut App) {
    update_active_camera(cx, PreviewCamera::end_drag);
}

fn update_active_camera(cx: &mut App, update: impl FnOnce(&mut PreviewCamera) -> bool) {
    let changed = if cx.has_global::<ActiveImagePreview>() {
        let preview = cx.global_mut::<ActiveImagePreview>();
        preview.content.image.is_some() && update(&mut preview.camera)
    } else {
        false
    };
    if changed {
        cx.refresh_windows();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewControl {
    Fit,
    ActualSize,
    ZoomIn,
    ZoomOut,
}

impl PreviewControl {
    fn tooltip(self) -> &'static str {
        match self {
            Self::Fit => "Fit to view",
            Self::ActualSize => "Actual size",
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
        }
    }
}

struct PreviewControlTooltip {
    label: &'static str,
    theme: crate::theme::GuiTheme,
}

impl Render for PreviewControlTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(4.0))
            .border(px(1.0))
            .border_color(rgb(self.theme.border))
            .bg(rgb(self.theme.panel))
            .text_color(rgb(self.theme.text))
            .text_size(px(12.0))
            .child(self.label)
    }
}

fn preview_controls(
    natural_image_size: Size<f32>,
    viewport: Bounds<Pixels>,
    theme: crate::theme::GuiTheme,
) -> Div {
    div()
        .absolute()
        .top(px(PREVIEW_CONTROLS_MARGIN_PX))
        .right(px(PREVIEW_CONTROLS_MARGIN_PX))
        .flex()
        .gap(px(4.0))
        .p(px(4.0))
        .rounded(px(6.0))
        .border(px(1.0))
        .border_color(rgb(theme.border))
        .bg(rgb(theme.panel))
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .children([
            preview_control_button(PreviewControl::Fit, natural_image_size, viewport, theme),
            preview_control_button(
                PreviewControl::ActualSize,
                natural_image_size,
                viewport,
                theme,
            ),
            preview_control_button(PreviewControl::ZoomIn, natural_image_size, viewport, theme),
            preview_control_button(PreviewControl::ZoomOut, natural_image_size, viewport, theme),
        ])
}

fn preview_control_button(
    control: PreviewControl,
    natural_image_size: Size<f32>,
    viewport: Bounds<Pixels>,
    theme: crate::theme::GuiTheme,
) -> AnyElement {
    let (id, icon_key, icon) = match control {
        PreviewControl::Fit => ("preview-fit", "preview-fit-icon", ICON_PREVIEW_FIT),
        PreviewControl::ActualSize => (
            "preview-actual-size",
            "preview-actual-size-icon",
            ICON_PREVIEW_RESET,
        ),
        PreviewControl::ZoomIn => ("preview-zoom-in", "preview-zoom-in-icon", ICON_PREVIEW_PLUS),
        PreviewControl::ZoomOut => (
            "preview-zoom-out",
            "preview-zoom-out-icon",
            ICON_PREVIEW_MINUS,
        ),
    };
    let tooltip = control.tooltip();
    div()
        .id(id)
        .size(px(PREVIEW_CONTROL_SIZE_PX))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(theme.hover_surface)))
        .tooltip(move |_, cx| {
            cx.new(|_| PreviewControlTooltip {
                label: tooltip,
                theme,
            })
            .into()
        })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            update_active_camera(cx, |camera| {
                match control {
                    PreviewControl::Fit => {
                        camera.fit_to_view(viewport.size, natural_image_size);
                    }
                    PreviewControl::ActualSize => {
                        camera.show_actual_size();
                    }
                    PreviewControl::ZoomIn => {
                        return camera.zoom_at(viewport.center(), viewport, PREVIEW_ZOOM_STEP);
                    }
                    PreviewControl::ZoomOut => {
                        return camera.zoom_at(
                            viewport.center(),
                            viewport,
                            1.0 / PREVIEW_ZOOM_STEP,
                        );
                    }
                }
                true
            });
            cx.stop_propagation();
        })
        .child(
            SvgIcon::new(icon_key, icon)
                .color(rgb(theme.text))
                .size(px(PREVIEW_CONTROL_ICON_SIZE_PX)),
        )
        .into_any_element()
}

fn natural_image_size(image: &RenderImage) -> Size<f32> {
    let natural = image.size(0);
    size(
        i32::from(natural.width).max(1) as f32,
        i32::from(natural.height).max(1) as f32,
    )
}

#[derive(Default)]
struct PreviewContent {
    image: Option<Arc<RenderImage>>,
    owns_image: bool,
    generation: u64,
}

impl PreviewContent {
    fn open(&mut self, image: Arc<RenderImage>, owns_image: bool) -> (u64, Vec<Arc<RenderImage>>) {
        self.generation = self.generation.wrapping_add(1);
        let retired = self.replace(image, owns_image);
        (self.generation, retired)
    }

    fn complete_load(
        &mut self,
        generation: u64,
        image: Option<Arc<RenderImage>>,
    ) -> (bool, Vec<Arc<RenderImage>>) {
        let Some(image) = image else {
            return (false, Vec::new());
        };
        if generation != self.generation || self.image.is_none() {
            return (false, vec![image]);
        }
        (true, self.replace(image, true))
    }

    fn close(&mut self) -> Vec<Arc<RenderImage>> {
        self.generation = self.generation.wrapping_add(1);
        let image = self.image.take();
        let owns_image = std::mem::take(&mut self.owns_image);
        image.filter(|_| owns_image).into_iter().collect()
    }

    fn replace(&mut self, image: Arc<RenderImage>, owns_image: bool) -> Vec<Arc<RenderImage>> {
        let previous = self.image.replace(image.clone());
        let previous_owned = std::mem::replace(&mut self.owns_image, owns_image);
        let Some(previous) = previous else {
            return Vec::new();
        };
        if Arc::ptr_eq(&previous, &image) {
            self.owns_image |= previous_owned;
            Vec::new()
        } else if previous_owned {
            vec![previous]
        } else {
            Vec::new()
        }
    }
}

fn apply_loaded_preview_image(generation: u64, image: Arc<RenderImage>, cx: &mut App) {
    let (accepted, retired) = if cx.has_global::<ActiveImagePreview>() {
        cx.global_mut::<ActiveImagePreview>()
            .content
            .complete_load(generation, Some(image))
    } else {
        (false, vec![image])
    };
    retire_preview_images(retired, cx);
    if accepted {
        cx.refresh_windows();
    }
}

fn retire_preview_images(images: Vec<Arc<RenderImage>>, cx: &mut App) {
    if images.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for image in images {
            cx.drop_image(image, None);
        }
    });
}

fn preview_viewport_bounds(
    editor_viewport_bounds: Option<Bounds<Pixels>>,
    window_viewport_size: Size<Pixels>,
) -> Bounds<Pixels> {
    editor_viewport_bounds.unwrap_or(Bounds {
        origin: point(px(0.0), px(0.0)),
        size: window_viewport_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(width: u32, height: u32) -> Arc<RenderImage> {
        Arc::new(RenderImage::new([image::Frame::new(
            image::RgbaImage::new(width, height),
        )]))
    }

    #[test]
    fn preview_uses_the_editor_parent_before_window_size() {
        let editor = gpui::Bounds {
            origin: gpui::point(gpui::px(200.0), gpui::px(80.0)),
            size: gpui::size(gpui::px(720.0), gpui::px(640.0)),
        };
        let window = gpui::size(gpui::px(1440.0), gpui::px(900.0));

        assert_eq!(preview_viewport_bounds(Some(editor), window), editor);
        assert_eq!(
            preview_viewport_bounds(None, window),
            gpui::Bounds {
                origin: gpui::point(gpui::px(0.0), gpui::px(0.0)),
                size: window,
            }
        );
    }

    #[test]
    fn preview_content_retires_only_owned_images_and_rejects_stale_results() {
        let fallback = test_image(4, 4);
        let original = test_image(16, 16);
        let stale = test_image(32, 32);
        let mut content = PreviewContent::default();

        let (generation, retired) = content.open(fallback, false);
        assert!(retired.is_empty());
        let (accepted, retired) = content.complete_load(generation, Some(original.clone()));
        assert!(accepted);
        assert!(retired.is_empty());

        let (next_generation, retired) = content.open(test_image(8, 8), true);
        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &original));

        let (accepted, retired) = content.complete_load(next_generation - 1, Some(stale.clone()));
        assert!(!accepted);
        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &stale));
    }

    #[test]
    fn preview_close_returns_owned_image_for_gpu_retirement() {
        let mut content = PreviewContent::default();
        let image = test_image(8, 8);
        content.open(image.clone(), true);
        let retired = content.close();
        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &image));
        assert!(content.image.is_none());
    }
}
