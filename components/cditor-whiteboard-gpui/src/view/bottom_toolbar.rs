use gpui::{
    AnyElement, AppContext, Context, InteractiveElement, IntoElement, ParentElement, PathBuilder,
    StatefulInteractiveElement, Styled, canvas, div, point, px, rgb,
};
use kurbo::{Point as KurboPoint, Size as KurboSize};

use super::{
    DrafftBoardView,
    components::{button::icon_button, icon::svg_icon, tooltip::ToolTip},
};
use crate::paint::GridStyle;

const ICON_COLOR: u32 = 0x334155;
const ZOOM_FACTOR: f64 = 1.2;

#[derive(Clone, Copy)]
enum BottomAction {
    Undo,
    Redo,
    ToggleGrid,
    ZoomOut,
    ZoomReset,
    ZoomIn,
    Center,
    Fit,
    ToggleGridSnap,
    ToggleSmartSnap,
    ToggleAngleSnap,
}

impl DrafftBoardView {
    pub(super) fn render_bottom_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .left(px(12.0))
            .bottom(px(12.0))
            .h(px(42.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(0xd7dce2))
            .bg(rgb(0xfafafc))
            .shadow_sm()
            .occlude()
            .child(self.bottom_svg_button(
                "bottom-undo",
                "drafft-icon-undo",
                include_bytes!("../../assets/undo.svg"),
                "Undo (Cmd+Z)",
                self.board.can_undo(),
                BottomAction::Undo,
                cx,
            ))
            .child(self.bottom_svg_button(
                "bottom-redo",
                "drafft-icon-redo",
                include_bytes!("../../assets/redo.svg"),
                "Redo (Cmd+Shift+Z)",
                self.board.can_redo(),
                BottomAction::Redo,
                cx,
            ))
            .child(separator())
            .child(self.grid_button(cx))
            .child(separator())
            .child(self.text_button(
                "bottom-zoom-out",
                "−",
                "Zoom out",
                BottomAction::ZoomOut,
                cx,
            ))
            .child(self.zoom_button(cx))
            .child(self.text_button("bottom-zoom-in", "+", "Zoom in", BottomAction::ZoomIn, cx))
            .child(self.bottom_svg_button(
                "bottom-center",
                "drafft-icon-center",
                include_bytes!("../../assets/center.svg"),
                "Center canvas at origin",
                true,
                BottomAction::Center,
                cx,
            ))
            .child(self.bottom_svg_button(
                "bottom-fit",
                "drafft-icon-fit",
                include_bytes!("../../assets/zoom-fit.svg"),
                if self.board.selected().is_empty() {
                    "Fit all elements"
                } else {
                    "Fit selection"
                },
                true,
                BottomAction::Fit,
                cx,
            ))
            .child(separator())
            .child(self.bottom_toggle_button(
                "bottom-grid-snap",
                "drafft-icon-grid-snap",
                include_bytes!("../../assets/snap-grid.svg"),
                if self.board.grid_snap_enabled() {
                    "Grid Snap: On"
                } else {
                    "Grid Snap: Off"
                },
                self.board.grid_snap_enabled(),
                BottomAction::ToggleGridSnap,
                cx,
            ))
            .child(self.bottom_toggle_button(
                "bottom-smart-snap",
                "drafft-icon-smart-snap",
                include_bytes!(
                    "../../assets/snap-shapes.svg"
                ),
                if self.board.smart_snap_enabled() {
                    "Smart Guides: On"
                } else {
                    "Smart Guides: Off"
                },
                self.board.smart_snap_enabled(),
                BottomAction::ToggleSmartSnap,
                cx,
            ))
            .child(self.bottom_toggle_button(
                "bottom-angle-snap",
                "drafft-icon-angle-snap",
                include_bytes!("../../assets/angle.svg"),
                if self.board.angle_snap_enabled() {
                    "Angle Snap: On (15 degrees)"
                } else {
                    "Angle Snap: Off"
                },
                self.board.angle_snap_enabled(),
                BottomAction::ToggleAngleSnap,
                cx,
            ))
    }

    fn bottom_svg_button(
        &self,
        id: &'static str,
        icon_key: &'static str,
        icon_bytes: &'static [u8],
        tooltip: &'static str,
        enabled: bool,
        action: BottomAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let button = icon_button(id, false, !enabled)
            .child(svg_icon(icon_key, icon_bytes, rgb(ICON_COLOR).into(), 16.0))
            .tooltip(move |_window, cx| cx.new(|_| ToolTip::new(tooltip)).into());
        if enabled {
            button
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.apply_bottom_action(action);
                    cx.notify();
                }))
                .into_any_element()
        } else {
            button.into_any_element()
        }
    }

    fn bottom_toggle_button(
        &self,
        id: &'static str,
        icon_key: &'static str,
        icon_bytes: &'static [u8],
        tooltip: &'static str,
        selected: bool,
        action: BottomAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        icon_button(id, selected, false)
            .child(svg_icon(icon_key, icon_bytes, rgb(ICON_COLOR).into(), 16.0))
            .tooltip(move |_window, cx| cx.new(|_| ToolTip::new(tooltip)).into())
            .on_click(cx.listener(move |view, _, _, cx| {
                view.apply_bottom_action(action);
                cx.notify();
            }))
            .into_any_element()
    }

    fn text_button(
        &self,
        id: &'static str,
        text: &'static str,
        tooltip: &'static str,
        action: BottomAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        icon_button(id, false, false)
            .text_size(px(16.0))
            .text_color(rgb(ICON_COLOR))
            .child(text)
            .tooltip(move |_window, cx| cx.new(|_| ToolTip::new(tooltip)).into())
            .on_click(cx.listener(move |view, _, _, cx| {
                view.apply_bottom_action(action);
                cx.notify();
            }))
            .into_any_element()
    }

    fn zoom_button(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("bottom-zoom-reset")
            .w(px(48.0))
            .h(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.0))
            .text_size(px(12.0))
            .text_color(rgb(ICON_COLOR))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0xf1f5f9)))
            .child(format!("{}%", self.board.zoom_percent()))
            .tooltip(|_window, cx| cx.new(|_| ToolTip::new("Reset to 100%")).into())
            .on_click(cx.listener(|view, _, _, cx| {
                view.apply_bottom_action(BottomAction::ZoomReset);
                cx.notify();
            }))
            .into_any_element()
    }

    fn grid_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let tooltip = grid_tooltip(self.grid_style);
        icon_button("bottom-grid", self.grid_style != GridStyle::None, false)
            .child(grid_icon(self.grid_style))
            .tooltip(move |_window, cx| cx.new(|_| ToolTip::new(tooltip)).into())
            .on_click(cx.listener(|view, _, _, cx| {
                view.apply_bottom_action(BottomAction::ToggleGrid);
                cx.notify();
            }))
            .into_any_element()
    }

    fn apply_bottom_action(&mut self, action: BottomAction) {
        let bounds = self.bounds.get();
        let center = KurboPoint::new(
            f32::from(bounds.size.width) as f64 / 2.0,
            f32::from(bounds.size.height) as f64 / 2.0,
        );
        match action {
            BottomAction::Undo => {
                self.board.undo();
            }
            BottomAction::Redo => {
                self.board.redo();
            }
            BottomAction::ToggleGrid => self.grid_style = self.grid_style.next(),
            BottomAction::ZoomOut => self.board.zoom_at(center, 1.0 / ZOOM_FACTOR),
            BottomAction::ZoomReset => self.board.zoom_reset_at(center),
            BottomAction::ZoomIn => self.board.zoom_at(center, ZOOM_FACTOR),
            BottomAction::Center => self.board.center_canvas(),
            BottomAction::Fit => self.board.zoom_to_fit(KurboSize::new(
                f32::from(bounds.size.width) as f64,
                f32::from(bounds.size.height) as f64,
            )),
            BottomAction::ToggleGridSnap => self.board.toggle_grid_snap(),
            BottomAction::ToggleSmartSnap => self.board.toggle_smart_snap(),
            BottomAction::ToggleAngleSnap => self.board.toggle_angle_snap(),
        }
    }
}

fn separator() -> impl IntoElement {
    div().mx(px(5.0)).w(px(1.0)).h(px(22.0)).bg(rgb(0xe2e8f0))
}

fn grid_tooltip(style: GridStyle) -> &'static str {
    match style {
        GridStyle::None => "Grid off; switch to square grid",
        GridStyle::Lines => "Square grid; switch to horizontal lines",
        GridStyle::HorizontalLines => "Horizontal lines; switch to crosses",
        GridStyle::CrossPlus => "Cross grid; switch to dots",
        GridStyle::Dots => "Dot grid; hide grid",
    }
}

fn grid_icon(style: GridStyle) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let left = bounds.origin.x + px(2.0);
            let top = bounds.origin.y + px(2.0);
            let right = bounds.origin.x + bounds.size.width - px(2.0);
            let bottom = bounds.origin.y + bounds.size.height - px(2.0);
            let mut path = if style == GridStyle::Dots {
                PathBuilder::fill()
            } else {
                PathBuilder::stroke(px(1.1))
            };
            match style {
                GridStyle::None => {
                    path.move_to(point(left, top));
                    path.line_to(point(right, top));
                    path.line_to(point(right, bottom));
                    path.line_to(point(left, bottom));
                    path.close();
                    path.move_to(point(left, bottom));
                    path.line_to(point(right, top));
                }
                GridStyle::Lines | GridStyle::HorizontalLines => {
                    for index in 0..3 {
                        let ratio = index as f32 / 2.0;
                        let y = top + (bottom - top) * ratio;
                        path.move_to(point(left, y));
                        path.line_to(point(right, y));
                        if style == GridStyle::Lines {
                            let x = left + (right - left) * ratio;
                            path.move_to(point(x, top));
                            path.line_to(point(x, bottom));
                        }
                    }
                }
                GridStyle::CrossPlus => {
                    for x_index in 0..3 {
                        for y_index in 0..3 {
                            let x = left + (right - left) * (x_index as f32 / 2.0);
                            let y = top + (bottom - top) * (y_index as f32 / 2.0);
                            path.move_to(point(x - px(1.5), y));
                            path.line_to(point(x + px(1.5), y));
                            path.move_to(point(x, y - px(1.5)));
                            path.line_to(point(x, y + px(1.5)));
                        }
                    }
                }
                GridStyle::Dots => {
                    for x_index in 0..3 {
                        for y_index in 0..3 {
                            let x = left + (right - left) * (x_index as f32 / 2.0);
                            let y = top + (bottom - top) * (y_index as f32 / 2.0);
                            path.add_polygon(
                                &[
                                    point(x - px(0.7), y - px(0.7)),
                                    point(x + px(0.7), y - px(0.7)),
                                    point(x + px(0.7), y + px(0.7)),
                                    point(x - px(0.7), y + px(0.7)),
                                ],
                                true,
                            );
                        }
                    }
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(ICON_COLOR));
            }
        },
    )
    .size(px(16.0))
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_tooltips_describe_every_transition() {
        let mut style = GridStyle::None;
        for _ in 0..5 {
            assert!(!grid_tooltip(style).is_empty());
            style = style.next();
        }
        assert_eq!(style, GridStyle::None);
    }
}
