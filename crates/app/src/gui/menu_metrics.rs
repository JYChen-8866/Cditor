use gpui::{Bounds, Pixels, Size, point, px};

/// Shared width for secondary menus opened from a primary toolbar or menu.
///
/// Keeping this compact reduces edge collisions while preserving enough room
/// for an icon or color swatch, a Chinese label, and an active check mark.
pub(crate) const SECONDARY_MENU_WIDTH_PX: f32 = 160.0;

/// The Cditor root viewport expressed in host-window coordinates.
///
/// Menu anchors produced by GPUI layout are window-relative. Rendering happens
/// inside the Cditor root, so every floating menu must convert through this
/// value before applying editor-local viewport constraints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EditorViewport {
    pub(crate) window_left: f32,
    pub(crate) window_top: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl EditorViewport {
    pub(crate) fn from_measurement(measured: Bounds<Pixels>, fallback_size: Size<Pixels>) -> Self {
        if measured.size.width > px(0.5) && measured.size.height > px(0.5) {
            Self {
                window_left: f32::from(measured.left()),
                window_top: f32::from(measured.top()),
                width: f32::from(measured.size.width),
                height: f32::from(measured.size.height),
            }
        } else {
            Self::from_size(fallback_size)
        }
    }

    pub(crate) fn from_size(viewport: Size<Pixels>) -> Self {
        Self {
            window_left: 0.0,
            window_top: 0.0,
            width: f32::from(viewport.width),
            height: f32::from(viewport.height),
        }
    }

    pub(crate) fn window_point_to_local(self, x: f32, y: f32) -> (f32, f32) {
        (x - self.window_left, y - self.window_top)
    }

    pub(crate) fn window_bounds_to_local(self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        Bounds::new(
            point(
                bounds.origin.x - px(self.window_left),
                bounds.origin.y - px(self.window_top),
            ),
            bounds.size,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MenuViewportBounds {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) bottom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecondaryMenuPlacement {
    Right,
    Left,
    Below,
    Above,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SecondaryMenuGeometry {
    pub(crate) placement: SecondaryMenuPlacement,
    pub(crate) left: f32,
    pub(crate) top: f32,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn secondary_menu_geometry(
    primary_left: f32,
    primary_top: f32,
    primary_width: f32,
    primary_height: f32,
    secondary_width: f32,
    secondary_height: f32,
    viewport: MenuViewportBounds,
    gap: f32,
    margin: f32,
) -> SecondaryMenuGeometry {
    let right = primary_left + primary_width + gap;
    let left = primary_left - gap - secondary_width;
    let min_left = viewport.left + margin;
    let min_top = viewport.top + margin;
    let max_left = (viewport.right - margin - secondary_width).max(min_left);
    let max_top = (viewport.bottom - margin - secondary_height).max(min_top);
    let side_top = primary_top.clamp(min_top, max_top);

    if right + secondary_width <= viewport.right - margin {
        return SecondaryMenuGeometry {
            placement: SecondaryMenuPlacement::Right,
            left: right,
            top: side_top,
        };
    }
    if left >= min_left {
        return SecondaryMenuGeometry {
            placement: SecondaryMenuPlacement::Left,
            left,
            top: side_top,
        };
    }

    let below = primary_top + primary_height + gap;
    let above = primary_top - gap - secondary_height;
    let space_below = (viewport.bottom - margin - below).max(0.0);
    let space_above = (primary_top - gap - min_top).max(0.0);
    let (placement, top) = if space_below >= secondary_height || space_below >= space_above {
        (SecondaryMenuPlacement::Below, below.clamp(min_top, max_top))
    } else {
        (SecondaryMenuPlacement::Above, above.clamp(min_top, max_top))
    };
    SecondaryMenuGeometry {
        placement,
        left: primary_left.clamp(min_left, max_left),
        top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    #[test]
    fn editor_viewport_converts_host_window_coordinates_to_editor_local() {
        let viewport = EditorViewport::from_measurement(
            Bounds::new(point(px(240.0), px(80.0)), size(px(900.0), px(600.0))),
            size(px(1_400.0), px(900.0)),
        );

        assert_eq!(viewport.window_point_to_local(600.0, 300.0), (360.0, 220.0));
        assert_eq!(viewport.width, 900.0);
        assert_eq!(viewport.height, 600.0);
    }

    #[test]
    fn secondary_menu_prefers_right_then_really_moves_left() {
        let right = secondary_menu_geometry(
            100.0,
            100.0,
            264.0,
            70.0,
            160.0,
            302.0,
            MenuViewportBounds {
                left: 0.0,
                top: 0.0,
                right: 900.0,
                bottom: 700.0,
            },
            6.0,
            8.0,
        );
        assert_eq!(right.placement, SecondaryMenuPlacement::Right);
        assert_eq!(right.left, 370.0);

        let left = secondary_menu_geometry(
            500.0,
            100.0,
            264.0,
            70.0,
            160.0,
            302.0,
            MenuViewportBounds {
                left: 0.0,
                top: 0.0,
                right: 800.0,
                bottom: 700.0,
            },
            6.0,
            8.0,
        );
        assert_eq!(left.placement, SecondaryMenuPlacement::Left);
        assert_eq!(left.left + 160.0 + 6.0, 500.0);
    }

    #[test]
    fn secondary_menu_uses_below_or_above_when_neither_side_fits() {
        let below = secondary_menu_geometry(
            70.0,
            40.0,
            264.0,
            70.0,
            160.0,
            302.0,
            MenuViewportBounds {
                left: 0.0,
                top: 0.0,
                right: 400.0,
                bottom: 700.0,
            },
            6.0,
            8.0,
        );
        assert_eq!(below.placement, SecondaryMenuPlacement::Below);
        assert_eq!(below.top, 116.0);

        let above = secondary_menu_geometry(
            70.0,
            500.0,
            264.0,
            70.0,
            160.0,
            302.0,
            MenuViewportBounds {
                left: 0.0,
                top: 0.0,
                right: 400.0,
                bottom: 700.0,
            },
            6.0,
            8.0,
        );
        assert_eq!(above.placement, SecondaryMenuPlacement::Above);
        assert_eq!(above.top + 302.0 + 6.0, 500.0);
    }
}
