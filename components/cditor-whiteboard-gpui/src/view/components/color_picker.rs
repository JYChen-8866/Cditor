use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, Bounds, Context, EventEmitter, Hsla, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement, PathBuilder, Pixels, Render, Rgba,
    StatefulInteractiveElement, Styled, div, hsla, prelude::FluentBuilder, px, rgb,
};

use crate::theme::chrome;

const TRACK_WIDTH: f32 = 180.0;
const TRACK_THUMB_SIZE: f32 = 10.0;

#[derive(Clone, Copy, Debug)]
pub(in crate::view) enum ColorPickerEvent {
    Change(Option<Hsla>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerTab {
    Palette,
    Hsla,
}

#[derive(Clone, Copy, Debug)]
enum Channel {
    Hue,
    Saturation,
    Lightness,
    Alpha,
}

pub(in crate::view) struct ColorPicker {
    value: Option<Hsla>,
    open: bool,
    allow_none: bool,
    active_tab: PickerTab,
    channel_bounds: [Rc<Cell<Bounds<Pixels>>>; 4],
}

impl ColorPicker {
    pub(in crate::view) fn new(value: Option<Hsla>, allow_none: bool) -> Self {
        Self {
            value,
            open: false,
            allow_none,
            active_tab: PickerTab::Palette,
            channel_bounds: std::array::from_fn(|_| Rc::new(Cell::new(Bounds::default()))),
        }
    }

    pub(in crate::view) fn set_value(&mut self, value: Option<Hsla>, cx: &mut Context<Self>) {
        self.value = value;
        cx.notify();
    }

    fn select(&mut self, value: Option<Hsla>, close: bool, cx: &mut Context<Self>) {
        if value.is_none() && !self.allow_none {
            return;
        }
        self.value = value;
        self.open = !close;
        cx.emit(ColorPickerEvent::Change(value));
        cx.notify();
    }

    fn current_color(&self) -> Hsla {
        self.value.unwrap_or_else(|| hsla(0.0, 0.0, 0.0, 1.0))
    }

    fn set_channel_from_position(
        &mut self,
        channel: Channel,
        position_x: Pixels,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.channel_bounds[channel_index(channel)].get();
        let width = f32::from(bounds.size.width).max(1.0);
        let value = (f32::from(position_x - bounds.origin.x) / width).clamp(0.0, 1.0);
        let mut color = self.current_color();
        match channel {
            Channel::Hue => color.h = value,
            Channel::Saturation => color.s = value,
            Channel::Lightness => color.l = value,
            Channel::Alpha => color.a = value,
        }
        self.select(Some(color), false, cx);
    }

    fn on_track_down(&mut self, channel: Channel, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.set_channel_from_position(channel, event.position.x, cx);
    }

    fn on_track_move(&mut self, channel: Channel, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button == Some(MouseButton::Left) {
            self.set_channel_from_position(channel, event.position.x, cx);
        }
    }

    fn render_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.current_color();
        let c = chrome(cx);
        div()
            .id("color-picker-trigger")
            .relative()
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(10.0))
            .overflow_hidden()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.open = !this.open;
                cx.notify();
            }))
            .child(hue_wheel())
            .child(
                div()
                    .absolute()
                    .left(px(5.0))
                    .top(px(5.0))
                    .size(px(10.0))
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(rgb(c.bg))
                    .bg(value),
            )
    }

    fn render_palette(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = palette_colors();
        let mut grid = div().grid().grid_cols(10).gap(px(4.0));
        if self.allow_none {
            grid = grid.child(self.palette_none(cx));
        }
        let swatches = colors
            .into_iter()
            .map(|color| self.palette_swatch(color, cx))
            .collect::<Vec<_>>();
        grid.children(swatches)
    }

    fn palette_none(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.value.is_none();
        let c = chrome(cx);
        div()
            .id("picker-none")
            .relative()
            .size(px(20.0))
            .rounded(px(3.0))
            .border(if selected { px(2.0) } else { px(1.0) })
            .border_color(if selected {
                rgb(c.accent)
            } else {
                rgb(c.border)
            })
            .child(
                div()
                    .absolute()
                    .left(px(2.0))
                    .top(px(8.0))
                    .w(px(14.0))
                    .h(px(2.0))
                    .bg(rgb(c.danger)),
            )
            .on_click(cx.listener(|this, _, _, cx| this.select(None, true, cx)))
            .into_any_element()
    }

    fn palette_swatch(&self, color: Hsla, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let selected = self
            .value
            .is_some_and(|value| color_distance(value, color) < 0.01);
        div()
            .id(("picker-color", color_id(color)))
            .size(px(20.0))
            .rounded(px(3.0))
            .border(if selected { px(2.0) } else { px(1.0) })
            .border_color(if selected {
                rgb(c.accent)
            } else {
                rgb(c.border)
            })
            .bg(color)
            .hover(|style| style.shadow_sm())
            .on_click(cx.listener(move |this, _, _, cx| this.select(Some(color), true, cx)))
            .into_any_element()
    }

    fn render_hsla(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let color = self.current_color();
        let c = chrome(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.render_track("Hue", Channel::Hue, color.h, color, cx))
            .child(self.render_track("Saturation", Channel::Saturation, color.s, color, cx))
            .child(self.render_track("Lightness", Channel::Lightness, color.l, color, cx))
            .child(self.render_track("Alpha", Channel::Alpha, color.a, color, cx))
            .child(
                div()
                    .pt(px(2.0))
                    .text_size(px(11.0))
                    .text_color(rgb(c.text_muted))
                    .child(format!("{}  A {:.0}%", color_hex(color), color.a * 100.0)),
            )
    }

    fn render_track(
        &self,
        label: &'static str,
        channel: Channel,
        value: f32,
        color: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bounds = self.channel_bounds[channel_index(channel)].clone();
        let colors = track_colors(channel, color);
        let c = chrome(cx);
        let track = div()
            .id(("color-track", channel_index(channel)))
            .relative()
            .w(px(TRACK_WIDTH))
            .h(px(18.0))
            .flex()
            .items_center()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .h(px(8.0))
                    .rounded(px(4.0))
                    .overflow_hidden()
                    .flex()
                    .children(
                        colors
                            .into_iter()
                            .map(|color| div().flex_1().h_full().bg(color)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(value * (TRACK_WIDTH - TRACK_THUMB_SIZE)))
                    .size(px(TRACK_THUMB_SIZE))
                    .rounded(px(TRACK_THUMB_SIZE / 2.0))
                    .border_2()
                    .border_color(rgb(c.bg))
                    .shadow_sm()
                    .bg(match channel {
                        Channel::Alpha => Hsla { a: value, ..color },
                        _ => color,
                    }),
            )
            .child(
                gpui::canvas(
                    move |new_bounds, _, _| bounds.set(new_bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, _, cx| this.on_track_down(channel, event, cx)),
            )
            .on_mouse_move(
                cx.listener(move |this, event, _, cx| this.on_track_move(channel, event, cx)),
            );

        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .w(px(70.0))
                    .text_size(px(10.0))
                    .text_color(rgb(c.text_muted))
                    .child(label),
            )
            .child(track)
    }

    fn render_popover(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette_active = self.active_tab == PickerTab::Palette;
        let c = chrome(cx);
        div()
            .absolute()
            .top(px(26.0))
            .left_0()
            .w(px(286.0))
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(7.0))
            .border_1()
            .border_color(rgb(c.border))
            .bg(rgb(c.bg))
            .shadow_lg()
            .occlude()
            .child(
                div()
                    .h(px(28.0))
                    .p(px(2.0))
                    .flex()
                    .rounded(px(5.0))
                    .bg(rgb(c.bg_strong))
                    .child(self.tab_button("Palette", PickerTab::Palette, palette_active, cx))
                    .child(self.tab_button("HSLA", PickerTab::Hsla, !palette_active, cx)),
            )
            .child(if palette_active {
                self.render_palette(cx).into_any_element()
            } else {
                self.render_hsla(cx).into_any_element()
            })
    }

    fn tab_button(
        &self,
        label: &'static str,
        tab: PickerTab,
        active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = chrome(cx);
        div()
            .id(("picker-tab", tab as usize))
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .text_size(px(11.0))
            .text_color(rgb(c.text))
            .when(active, |style| style.bg(rgb(c.bg)).shadow_sm())
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active_tab = tab;
                cx.notify();
            }))
    }
}

impl EventEmitter<ColorPickerEvent> for ColorPicker {}

impl Render for ColorPicker {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().relative().child(self.render_trigger(cx));
        if self.open {
            root = root.child(self.render_popover(cx));
        }
        root
    }
}

fn hue_wheel() -> AnyElement {
    gpui::canvas(
        |_, _, _| {},
        |bounds, _, window, _| {
            let center = gpui::point(
                bounds.origin.x + bounds.size.width / 2.0,
                bounds.origin.y + bounds.size.height / 2.0,
            );
            let radius = bounds.size.width.min(bounds.size.height) / 2.0;
            for index in 0..32 {
                let angle_a = index as f32 / 32.0 * std::f32::consts::TAU;
                let angle_b = (index + 1) as f32 / 32.0 * std::f32::consts::TAU;
                let mut path = PathBuilder::fill();
                path.move_to(center);
                path.line_to(gpui::point(
                    center.x + radius * angle_a.cos(),
                    center.y + radius * angle_a.sin(),
                ));
                path.line_to(gpui::point(
                    center.x + radius * angle_b.cos(),
                    center.y + radius * angle_b.sin(),
                ));
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, hsla(index as f32 / 32.0, 1.0, 0.5, 1.0));
                }
            }
        },
    )
    .size_full()
    .into_any_element()
}

fn channel_index(channel: Channel) -> usize {
    match channel {
        Channel::Hue => 0,
        Channel::Saturation => 1,
        Channel::Lightness => 2,
        Channel::Alpha => 3,
    }
}

fn color_distance(left: Hsla, right: Hsla) -> f32 {
    (left.h - right.h).abs()
        + (left.s - right.s).abs()
        + (left.l - right.l).abs()
        + (left.a - right.a).abs()
}

fn color_id(color: Hsla) -> u64 {
    u32::from(Rgba::from(color)) as u64
}

fn color_hex(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    format!(
        "#{:02X}{:02X}{:02X}",
        (rgba.r * 255.0).round() as u8,
        (rgba.g * 255.0).round() as u8,
        (rgba.b * 255.0).round() as u8,
    )
}

fn palette_colors() -> Vec<Hsla> {
    let hues = [0.0, 0.04, 0.10, 0.16, 0.33, 0.48, 0.58, 0.67, 0.76, 0.90];
    let mut colors = Vec::with_capacity(50);
    for lightness in [0.18, 0.34, 0.50, 0.68, 0.86] {
        for hue in hues {
            colors.push(hsla(
                hue,
                if hue == 0.0 { 0.0 } else { 0.72 },
                lightness,
                1.0,
            ));
        }
    }
    colors
}

fn track_colors(channel: Channel, color: Hsla) -> Vec<Hsla> {
    (0..24)
        .map(|index| {
            let value = index as f32 / 23.0;
            match channel {
                Channel::Hue => hsla(value, 1.0, 0.5, 1.0),
                Channel::Saturation => hsla(color.h, value, color.l, 1.0),
                Channel::Lightness => hsla(color.h, color.s, value, 1.0),
                Channel::Alpha => Hsla { a: value, ..color },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_full_neutral_and_hue_matrix() {
        assert_eq!(palette_colors().len(), 50);
    }

    #[test]
    fn each_slider_track_has_stable_resolution() {
        let color = hsla(0.3, 0.5, 0.5, 1.0);
        for channel in [
            Channel::Hue,
            Channel::Saturation,
            Channel::Lightness,
            Channel::Alpha,
        ] {
            assert_eq!(track_colors(channel, color).len(), 24);
        }
    }
}
