use drafftink_core::shapes::{SerializableColor, ShapeStyle};
use gpui::{
    AnyElement, AppContext, Context, Hsla, InteractiveElement, IntoElement, ParentElement, Rgba,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px, rgb,
};

use super::DrafftBoardView;

const STROKE_WIDTHS: [(f64, &str); 4] = [
    (1.0, "Thin"),
    (2.0, "Normal"),
    (4.0, "Bold"),
    (8.0, "Extra Bold"),
];

// Exact Tailwind 500/50 values selected by upstream QUICK_COLORS.
const STROKE_COLORS: [(u32, &str); 6] = [
    (0x3b82f6, "Blue"),
    (0xef4444, "Red"),
    (0x10b981, "Emerald"),
    (0xf59e0b, "Amber"),
    (0xa855f7, "Purple"),
    (0x64748b, "Slate"),
];
const FILL_COLORS: [(u32, &str); 6] = [
    (0xeff6ff, "Blue"),
    (0xfef2f2, "Red"),
    (0xecfdf5, "Emerald"),
    (0xfffbeb, "Amber"),
    (0xfaf5ff, "Purple"),
    (0xf8fafc, "Slate"),
];

impl DrafftBoardView {
    pub(super) fn render_properties(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let style = self.visible_style();
        let stroke_color = style.stroke_color;
        let fill_color = style.fill_color;
        let stroke_width = style.stroke_width;

        div()
            .absolute()
            .top(px(12.0))
            .left_1_2()
            .ml(px(-263.0))
            .w(px(526.0))
            .h(px(52.0))
            .px(px(8.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(0xdcdcdc))
            .bg(rgb(0xfafafc))
            .shadow_sm()
            .occlude()
            .child(self.stroke_section(stroke_color, cx))
            .child(panel_separator())
            .child(self.fill_section(fill_color, cx))
            .child(panel_separator())
            .child(self.stroke_width_section(stroke_width, cx))
    }

    fn stroke_section(&self, current: SerializableColor, cx: &mut Context<Self>) -> AnyElement {
        let swatches = STROKE_COLORS
            .into_iter()
            .enumerate()
            .map(|(index, (hex, name))| {
                self.stroke_swatch(index, hex, name, current == serializable_rgb(hex), cx)
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(section_label("Stroke"))
            .child(
                div()
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .children(swatches)
                    .child(swatch_separator())
                    .child(self.stroke_picker.clone()),
            )
            .into_any_element()
    }

    fn fill_section(
        &self,
        current: Option<SerializableColor>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let swatches = FILL_COLORS
            .into_iter()
            .enumerate()
            .map(|(index, (hex, name))| {
                self.fill_swatch(index, hex, name, current == Some(serializable_rgb(hex)), cx)
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(section_label("Fill"))
            .child(
                div()
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(self.no_fill_swatch(current.is_none(), cx))
                    .children(swatches)
                    .child(swatch_separator())
                    .child(self.fill_picker.clone()),
            )
            .into_any_element()
    }

    fn stroke_width_section(&self, current: f64, cx: &mut Context<Self>) -> AnyElement {
        let buttons = STROKE_WIDTHS
            .into_iter()
            .enumerate()
            .map(|(index, (width, name))| {
                let selected = (current - width).abs() < 0.1;
                div()
                    .id(("stroke-width", index))
                    .w(px(28.0))
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(if selected {
                        rgb(0x3b82f6)
                    } else {
                        rgb(0xd2d2d2)
                    })
                    .bg(if selected {
                        rgb(0x3b82f6)
                    } else {
                        rgb(0xfafafa)
                    })
                    .tooltip(move |_window, cx| {
                        cx.new(|_| super::components::tooltip::ToolTip::new(name))
                            .into()
                    })
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.board.set_stroke_width(width);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(16.0))
                            .h(px(width as f32))
                            .rounded(px((width / 2.0) as f32))
                            .bg(if selected {
                                rgb(0xffffff)
                            } else {
                                rgb(0x505050)
                            }),
                    )
            });
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(section_label("Stroke width"))
            .child(div().h(px(20.0)).flex().gap(px(2.0)).children(buttons))
            .into_any_element()
    }

    fn stroke_swatch(
        &self,
        index: usize,
        hex: u32,
        name: &'static str,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        color_swatch(("stroke-swatch", index), hex, name, selected)
            .on_click(cx.listener(move |view, _, _, cx| {
                view.board.set_stroke_color(serializable_rgb(hex));
                view.sync_style_controls(cx);
                cx.notify();
            }))
            .into_any_element()
    }

    fn fill_swatch(
        &self,
        index: usize,
        hex: u32,
        name: &'static str,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        color_swatch(("fill-swatch", index), hex, name, selected)
            .on_click(cx.listener(move |view, _, _, cx| {
                view.board.set_fill_color(Some(serializable_rgb(hex)));
                view.sync_style_controls(cx);
                cx.notify();
            }))
            .into_any_element()
    }

    fn no_fill_swatch(&self, selected: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("fill-none")
            .relative()
            .size(px(20.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgb(0xd2d2d2))
            .bg(rgb(0xffffff))
            .cursor_pointer()
            .when(selected, |swatch| {
                swatch.child(selection_ring(rgb(0x1e1e1e).into()))
            })
            .child(
                div()
                    .absolute()
                    .left(px(2.0))
                    .top(px(9.0))
                    .w(px(16.0))
                    .h(px(1.0))
                    .bg(rgb(0xef4444)),
            )
            .on_click(cx.listener(|view, _, _, cx| {
                view.board.set_fill_color(None);
                view.sync_style_controls(cx);
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn sync_style_controls(&mut self, cx: &mut Context<Self>) {
        let style = self.visible_style().clone();
        self.stroke_picker.update(cx, |picker, cx| {
            picker.set_value(Some(serializable_to_hsla(style.stroke_color)), cx);
        });
        self.fill_picker.update(cx, |picker, cx| {
            picker.set_value(style.fill_color.map(serializable_to_hsla), cx);
        });
        self.opacity_slider.update(cx, |slider, cx| {
            slider.set_value(style.opacity as f32, cx);
        });
    }

    pub(super) fn visible_style(&self) -> &ShapeStyle {
        self.board
            .selected()
            .first()
            .and_then(|id| self.board.canvas.document.get_shape(*id))
            .map(|shape| shape.style())
            .unwrap_or(&self.board.canvas.tool_manager.current_style)
    }
}

fn color_swatch(
    id: impl Into<gpui::ElementId>,
    hex: u32,
    name: &'static str,
    selected: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .relative()
        .size(px(20.0))
        .rounded(px(10.0))
        .bg(rgb(hex))
        .cursor_pointer()
        .tooltip(move |_window, cx| {
            cx.new(|_| super::components::tooltip::ToolTip::new(name))
                .into()
        })
        .when(selected, |swatch| {
            swatch.child(selection_ring(rgb(0x1e1e1e).into()))
        })
}

fn selection_ring(color: Hsla) -> impl IntoElement {
    div()
        .absolute()
        .left(px(3.0))
        .top(px(3.0))
        .size(px(14.0))
        .rounded(px(7.0))
        .border_2()
        .border_color(color)
}

fn section_label(text: &'static str) -> impl IntoElement {
    div()
        .h(px(12.0))
        .text_size(px(10.0))
        .text_color(rgb(0x787878))
        .child(text)
}

fn panel_separator() -> impl IntoElement {
    div().w(px(1.0)).h(px(32.0)).bg(rgb(0xdcdcdc))
}

fn swatch_separator() -> impl IntoElement {
    div().mx(px(4.0)).w(px(1.0)).h(px(14.0)).bg(rgb(0xd2d2d2))
}

fn serializable_rgb(hex: u32) -> SerializableColor {
    SerializableColor::new(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
        255,
    )
}

pub(super) fn serializable_to_hsla(color: SerializableColor) -> Hsla {
    Hsla::from(Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: color.a as f32 / 255.0,
    })
}

pub(super) fn hsla_to_serializable(color: Hsla) -> SerializableColor {
    let color = Rgba::from(color);
    SerializableColor::new(
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
        (color.a * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_colors_match_upstream_tailwind_indices() {
        assert_eq!(STROKE_COLORS[0].0, 0x3b82f6);
        assert_eq!(FILL_COLORS[2].0, 0xecfdf5);
    }

    #[test]
    fn color_conversion_preserves_rgba8_values() {
        let color = SerializableColor::new(18, 127, 231, 96);
        assert_eq!(hsla_to_serializable(serializable_to_hsla(color)), color);
    }
}
