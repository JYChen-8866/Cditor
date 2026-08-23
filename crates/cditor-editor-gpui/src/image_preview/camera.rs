use gpui::{Bounds, Pixels, Point, Size, point, px, size};

pub(crate) const PREVIEW_MIN_ZOOM: f32 = 0.1;
pub(crate) const PREVIEW_MAX_ZOOM: f32 = 20.0;
pub(crate) const PREVIEW_ZOOM_STEP: f32 = 1.2;
pub(crate) const PREVIEW_SCROLL_LINE_PX: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PreviewCamera {
    zoom: f32,
    min_zoom: f32,
    max_zoom: f32,
    pan_offset: Point<Pixels>,
    last_drag_position: Option<Point<Pixels>>,
}

impl Default for PreviewCamera {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            min_zoom: PREVIEW_MIN_ZOOM,
            max_zoom: PREVIEW_MAX_ZOOM,
            pan_offset: point(px(0.0), px(0.0)),
            last_drag_position: None,
        }
    }
}

impl PreviewCamera {
    #[cfg(test)]
    pub(crate) fn zoom(&self) -> f32 {
        self.zoom
    }

    pub(crate) fn is_dragging(&self) -> bool {
        self.last_drag_position.is_some()
    }

    pub(crate) fn fit_to_view(&mut self, viewport: Size<Pixels>, natural_image_size: Size<f32>) {
        let fit_zoom = fit_scale(viewport, natural_image_size);
        self.min_zoom = PREVIEW_MIN_ZOOM.min(fit_zoom * 0.1);
        self.max_zoom = PREVIEW_MAX_ZOOM.max(fit_zoom);
        self.zoom = fit_zoom;
        self.pan_offset = point(px(0.0), px(0.0));
        self.last_drag_position = None;
    }

    pub(crate) fn show_actual_size(&mut self) {
        self.zoom = 1.0_f32.clamp(self.min_zoom, self.max_zoom);
        self.pan_offset = point(px(0.0), px(0.0));
        self.last_drag_position = None;
    }

    pub(crate) fn zoom_at(
        &mut self,
        anchor: Point<Pixels>,
        viewport: Bounds<Pixels>,
        factor: f32,
    ) -> bool {
        if !factor.is_finite() || factor <= 0.0 {
            return false;
        }
        let old_zoom = self.zoom;
        let new_zoom = (old_zoom * factor).clamp(self.min_zoom, self.max_zoom);
        if (new_zoom - old_zoom).abs() < f32::EPSILON {
            return false;
        }

        let viewport_center = point(
            viewport.origin.x + viewport.size.width / 2.0,
            viewport.origin.y + viewport.size.height / 2.0,
        );
        let anchor_from_view_center = anchor - viewport_center;
        let anchor_from_image_center = anchor_from_view_center - self.pan_offset;
        let zoom_ratio = new_zoom / old_zoom;

        self.zoom = new_zoom;
        self.pan_offset += anchor_from_image_center * (1.0 - zoom_ratio);
        true
    }

    pub(crate) fn pan_by(&mut self, delta: Point<Pixels>) -> bool {
        if delta.x == px(0.0) && delta.y == px(0.0) {
            return false;
        }
        self.pan_offset += delta;
        true
    }

    pub(crate) fn begin_drag(&mut self, position: Point<Pixels>) {
        self.last_drag_position = Some(position);
    }

    pub(crate) fn drag_to(&mut self, position: Point<Pixels>) -> bool {
        let Some(previous) = self.last_drag_position.replace(position) else {
            return false;
        };
        self.pan_by(position - previous)
    }

    pub(crate) fn end_drag(&mut self) -> bool {
        self.last_drag_position.take().is_some()
    }

    pub(crate) fn image_frame(
        &self,
        viewport: Size<Pixels>,
        natural_image_size: Size<f32>,
    ) -> Bounds<Pixels> {
        let scaled_size = size(
            px(natural_image_size.width * self.zoom),
            px(natural_image_size.height * self.zoom),
        );
        Bounds {
            origin: point(
                (viewport.width - scaled_size.width) / 2.0 + self.pan_offset.x,
                (viewport.height - scaled_size.height) / 2.0 + self.pan_offset.y,
            ),
            size: scaled_size,
        }
    }
}

pub(crate) fn wheel_zoom_factor(delta_y: f32) -> f32 {
    if delta_y > 0.0 {
        1.0 + delta_y.abs() * 0.01
    } else if delta_y < 0.0 {
        1.0 / (1.0 + delta_y.abs() * 0.01)
    } else {
        1.0
    }
}

fn fit_scale(viewport: Size<Pixels>, natural_image_size: Size<f32>) -> f32 {
    let image_width = natural_image_size.width.max(1.0);
    let image_height = natural_image_size.height.max(1.0);
    let viewport_width = f32::from(viewport.width).max(1.0);
    let viewport_height = f32::from(viewport.height).max(1.0);
    (viewport_width / image_width)
        .min(viewport_height / image_height)
        .max(f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: Pixels, expected: Pixels) {
        assert!((f32::from(actual) - f32::from(expected)).abs() < 0.001);
    }

    #[test]
    fn zoom_keeps_the_pointer_anchor_stable() {
        let viewport = Bounds {
            origin: point(px(100.0), px(50.0)),
            size: size(px(800.0), px(600.0)),
        };
        let anchor = point(px(700.0), px(200.0));
        let mut camera = PreviewCamera::default();
        let center = viewport.center();
        let before = (anchor - center - camera.pan_offset) * (1.0 / camera.zoom);

        assert!(camera.zoom_at(anchor, viewport, 2.0));

        let after = (anchor - center - camera.pan_offset) * (1.0 / camera.zoom);
        assert_near(before.x, after.x);
        assert_near(before.y, after.y);
    }

    #[test]
    fn zoom_is_clamped_to_supported_range() {
        let viewport = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(800.0), px(600.0)),
        };
        let center = viewport.center();
        let mut camera = PreviewCamera::default();

        assert!(camera.zoom_at(center, viewport, 1_000.0));
        assert_eq!(camera.zoom(), PREVIEW_MAX_ZOOM);
        assert!(camera.zoom_at(center, viewport, 0.000_01));
        assert_eq!(camera.zoom(), PREVIEW_MIN_ZOOM);
    }

    #[test]
    fn dragging_accumulates_incremental_pan() {
        let mut camera = PreviewCamera::default();
        camera.begin_drag(point(px(10.0), px(20.0)));

        assert!(camera.drag_to(point(px(25.0), px(35.0))));
        assert!(camera.drag_to(point(px(30.0), px(50.0))));

        assert_eq!(camera.pan_offset, point(px(20.0), px(30.0)));
        assert!(camera.end_drag());
        assert!(!camera.is_dragging());
    }

    #[test]
    fn pressing_without_pointer_movement_does_not_pan_the_canvas() {
        let mut camera = PreviewCamera::default();
        let initial_frame = camera.image_frame(size(px(800.0), px(600.0)), size(400.0, 300.0));

        camera.begin_drag(point(px(100.0), px(120.0)));

        assert_eq!(
            camera.image_frame(size(px(800.0), px(600.0)), size(400.0, 300.0)),
            initial_frame
        );
        assert!(camera.end_drag());
        assert!(!camera.is_dragging());
    }

    #[test]
    fn actual_size_uses_one_display_pixel_per_image_pixel() {
        let mut camera = PreviewCamera::default();
        camera.show_actual_size();

        assert_eq!(camera.zoom(), 1.0);
        assert_eq!(
            camera
                .image_frame(size(px(800.0), px(600.0)), size(1_600.0, 900.0))
                .size,
            size(px(1_600.0), px(900.0))
        );
    }

    #[test]
    fn fit_to_view_centers_the_natural_image_without_distorting_it() {
        let viewport = size(px(800.0), px(600.0));
        let image = size(1_600.0, 900.0);
        let mut camera = PreviewCamera::default();

        camera.fit_to_view(viewport, image);
        let frame = camera.image_frame(viewport, image);

        assert_eq!(camera.zoom(), 0.5);
        assert_eq!(frame.size, size(px(800.0), px(450.0)));
        assert_eq!(frame.origin, point(px(0.0), px(75.0)));
    }

    #[test]
    fn fit_zoom_remains_available_for_extreme_image_sizes() {
        let viewport = size(px(1_000.0), px(1_000.0));
        let mut camera = PreviewCamera::default();

        camera.fit_to_view(viewport, size(100_000.0, 50_000.0));
        assert_eq!(camera.zoom(), 0.01);
        assert!(camera.zoom_at(
            point(px(500.0), px(500.0)),
            Bounds {
                origin: point(px(0.0), px(0.0)),
                size: viewport,
            },
            0.5,
        ));
        assert_eq!(camera.zoom(), 0.005);

        camera.fit_to_view(viewport, size(10.0, 10.0));
        assert_eq!(camera.zoom(), 100.0);
        camera.show_actual_size();
        assert_eq!(camera.zoom(), 1.0);
    }

    #[test]
    fn wheel_zoom_factor_tracks_scroll_direction() {
        assert!(wheel_zoom_factor(10.0) > 1.0);
        assert!(wheel_zoom_factor(-10.0) < 1.0);
        assert_eq!(wheel_zoom_factor(0.0), 1.0);
    }
}
