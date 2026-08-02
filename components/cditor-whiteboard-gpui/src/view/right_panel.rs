use crate::{
    shapes::{FillPattern, PathStyle, Shape, Sloppiness, StrokeStyle},
    tools::ToolKind,
};
use gpui::{
    AnyElement, AppContext, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};

use crate::theme::chrome;

use super::{
    DrafftBoardView,
    components::{
        button::icon_button, icon::svg_icon, segmented::segment, slider::Slider, tooltip::ToolTip,
    },
};

const SLOPPINESS: [(Sloppiness, &str); 4] = [
    (Sloppiness::Architect, "Architect"),
    (Sloppiness::Artist, "Artist"),
    (Sloppiness::Cartoonist, "Cartoonist"),
    (Sloppiness::Drunk, "Drunk"),
];
const FILL_PATTERNS: [(FillPattern, &str); 4] = [
    (FillPattern::Solid, "Solid"),
    (FillPattern::Hachure, "Hatch"),
    (FillPattern::CrossHatch, "Cross"),
    (FillPattern::Dots, "Dots"),
];
const PATH_STYLES: [(PathStyle, &str); 3] = [
    (PathStyle::Direct, "Direct"),
    (PathStyle::Flowing, "Flowing"),
    (PathStyle::Angular, "Angular"),
];
const STROKE_STYLES: [(StrokeStyle, &str); 3] = [
    (StrokeStyle::Solid, "Solid"),
    (StrokeStyle::Dashed, "Dashed"),
    (StrokeStyle::Dotted, "Dotted"),
];
const FONT_SIZES: [(f64, &str); 4] = [(16.0, "S"), (20.0, "M"), (28.0, "L"), (36.0, "XL")];

#[derive(Clone, Copy)]
enum PanelAction {
    SendToBack,
    SendBackward,
    BringForward,
    BringToFront,
    FlipHorizontal,
    FlipVertical,
    AlignLeft,
    AlignCenterVertical,
    AlignRight,
    AlignTop,
    AlignCenterHorizontal,
    AlignBottom,
}

impl DrafftBoardView {
    pub(super) fn render_right_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        if self.board.selected().is_empty() && !is_property_drawing_tool(self.board.tool()) {
            return div().into_any_element();
        }

        let style = self.visible_style();
        let show_corner = self.supports_corner_radius();
        let text_properties = self.selected_text_properties();
        let math_size = self.selected_math_size();
        let show_freehand = self.supports_freehand();
        let show_linear = self.supports_linear();
        let show_sloppiness = !self.supports_text() && !show_freehand;
        let show_fill = style.fill_color.is_some() && !show_linear && !show_freehand;
        let show_selection =
            !self.board.selected().is_empty() && !is_property_drawing_tool(self.board.tool());
        let show_alignment = show_selection && self.board.selected().len() >= 2;
        let section_count = usize::from(show_corner)
            + usize::from(text_properties.is_some())
            + usize::from(math_size.is_some())
            + usize::from(show_sloppiness)
            + usize::from(show_fill)
            + usize::from(show_linear) * 2
            + usize::from(show_freehand) * 2
            + usize::from(show_selection) * 3
            + usize::from(show_alignment);
        let panel_height = 42.0 + section_count as f32 * 50.0;

        let mut panel = div()
            .absolute()
            .right(px(12.0))
            .top_1_2()
            .mt(px(-panel_height / 2.0))
            .w(px(200.0))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(c.border))
            .bg(rgb(c.bg))
            .shadow_sm()
            .occlude()
            .child(
                div()
                    .h(px(18.0))
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(c.text))
                    .child("Properties"),
            );

        if let Some(size) = text_properties {
            panel = panel.child(self.font_size_section(size, false, cx));
        }
        if let Some(size) = math_size {
            panel = panel.child(self.font_size_section(size, true, cx));
        }
        if show_corner {
            panel = panel.child(self.corner_section(self.visible_corner_radius(), cx));
        }
        if show_sloppiness {
            panel = panel.child(self.sloppiness_section(style.sloppiness, cx));
        }
        if show_fill {
            panel = panel.child(self.fill_pattern_section(style.fill_pattern, cx));
        }
        if show_linear {
            panel = panel
                .child(self.path_section(cx))
                .child(self.stroke_style_section(cx));
        }
        if show_freehand {
            panel = panel
                .child(self.freehand_style_section(cx))
                .child(self.pressure_section(cx));
        }
        if show_selection {
            panel = panel
                .child(self.layer_section(cx))
                .child(self.transform_section(cx))
                .child(self.opacity_section(style.opacity, cx));
        }
        if show_alignment {
            panel = panel.child(self.alignment_section(cx));
        }
        panel.into_any_element()
    }

    fn font_size_section(&self, current: f64, math: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let element_key = if math {
            "math-font-size"
        } else {
            "text-font-size"
        };
        section(
            "Font Size",
            c.text_muted,
            FONT_SIZES
                .into_iter()
                .enumerate()
                .map(|(index, (size, label))| {
                    segment((element_key, index), label, (current - size).abs() < 1.0)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if math {
                                view.board.set_math_font_size(size);
                            } else {
                                view.board.set_text_font_size(size);
                            }
                            cx.notify();
                        }))
                        .into_any_element()
                }),
        )
    }

    fn corner_section(&self, current: f64, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let off = current < 1.0;
        section(
            "Rounded Corners",
            c.text_muted,
            [
                segment("corner-off", "Off", off)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.board.set_corner_radius(0.0);
                        cx.notify();
                    }))
                    .into_any_element(),
                segment("corner-on", "On", !off)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.board.set_corner_radius(32.0);
                        cx.notify();
                    }))
                    .into_any_element(),
            ],
        )
    }

    fn sloppiness_section(&self, current: Sloppiness, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        section(
            "Sloppiness",
            c.text_muted,
            SLOPPINESS
                .into_iter()
                .enumerate()
                .map(|(index, (value, label))| {
                    segment(("sloppiness", index), label, current == value)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.board.set_sloppiness(value);
                            cx.notify();
                        }))
                        .into_any_element()
                }),
        )
    }

    fn fill_pattern_section(&self, current: FillPattern, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        section(
            "Fill Pattern",
            c.text_muted,
            FILL_PATTERNS
                .into_iter()
                .enumerate()
                .map(|(index, (value, label))| {
                    segment(("fill-pattern", index), label, current == value)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.board.set_fill_pattern(value);
                            cx.notify();
                        }))
                        .into_any_element()
                }),
        )
    }

    fn path_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let current = self.board.path_style();
        section(
            "Path",
            c.text_muted,
            PATH_STYLES
                .into_iter()
                .enumerate()
                .map(|(index, (value, label))| {
                    segment(("path-style", index), label, current == value)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.board.set_path_style(value);
                            cx.notify();
                        }))
                        .into_any_element()
                }),
        )
    }

    fn stroke_style_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let current = self.board.stroke_style();
        section(
            "Stroke",
            c.text_muted,
            STROKE_STYLES
                .into_iter()
                .enumerate()
                .map(|(index, (value, label))| {
                    segment(("stroke-style", index), label, current == value)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.board.set_stroke_style(value);
                            cx.notify();
                        }))
                        .into_any_element()
                }),
        )
    }

    fn freehand_style_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let calligraphy = self.board.calligraphy_mode();
        section(
            "Style",
            c.text_muted,
            [
                segment("freehand-normal", "Normal", !calligraphy)
                    .on_click(cx.listener(|view, _, _, cx| {
                        if view.board.calligraphy_mode() {
                            view.board.toggle_calligraphy();
                            cx.notify();
                        }
                    }))
                    .into_any_element(),
                segment("freehand-calligraphy", "Calligraphy", calligraphy)
                    .on_click(cx.listener(|view, _, _, cx| {
                        if !view.board.calligraphy_mode() {
                            view.board.toggle_calligraphy();
                            cx.notify();
                        }
                    }))
                    .into_any_element(),
            ],
        )
    }

    fn pressure_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let pressure = self.board.pressure_simulation();
        section(
            "Pressure",
            c.text_muted,
            [
                segment("pressure-uniform", "Uniform", !pressure)
                    .on_click(cx.listener(|view, _, _, cx| {
                        if view.board.pressure_simulation() {
                            view.board.toggle_pressure_simulation();
                            cx.notify();
                        }
                    }))
                    .into_any_element(),
                segment("pressure-variable", "Pressure", pressure)
                    .on_click(cx.listener(|view, _, _, cx| {
                        if !view.board.pressure_simulation() {
                            view.board.toggle_pressure_simulation();
                            cx.notify();
                        }
                    }))
                    .into_any_element(),
            ],
        )
    }

    fn layer_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        icon_section(
            "Layer",
            c.text_muted,
            [
                self.panel_icon_button(
                    "layer-back",
                    "layer-back",
                    include_bytes!("../../assets/layer-back.svg"),
                    "Send to Back",
                    PanelAction::SendToBack,
                    cx,
                ),
                self.panel_icon_button(
                    "layer-backward",
                    "layer-backward",
                    include_bytes!("../../assets/layer-backward.svg"),
                    "Send Backward",
                    PanelAction::SendBackward,
                    cx,
                ),
                self.panel_icon_button(
                    "layer-forward",
                    "layer-forward",
                    include_bytes!("../../assets/layer-forward.svg"),
                    "Bring Forward",
                    PanelAction::BringForward,
                    cx,
                ),
                self.panel_icon_button(
                    "layer-front",
                    "layer-front",
                    include_bytes!("../../assets/layer-front.svg"),
                    "Bring to Front",
                    PanelAction::BringToFront,
                    cx,
                ),
            ],
        )
    }

    fn transform_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        icon_section(
            "Transform",
            c.text_muted,
            [
                self.panel_icon_button(
                    "flip-h",
                    "flip-h",
                    include_bytes!("../../assets/flip-h.svg"),
                    "Flip Horizontal",
                    PanelAction::FlipHorizontal,
                    cx,
                ),
                self.panel_icon_button(
                    "flip-v",
                    "flip-v",
                    include_bytes!("../../assets/flip-v.svg"),
                    "Flip Vertical",
                    PanelAction::FlipVertical,
                    cx,
                ),
            ],
        )
    }

    fn opacity_section(&self, opacity: f64, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        let control = Slider::new(&self.opacity_slider);

        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(section_label("Opacity", c.text_muted))
            .child(
                div()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(control)
                    .child(
                        div()
                            .w(px(34.0))
                            .text_size(px(11.0))
                            .text_color(rgb(c.text_muted))
                            .child(format!("{}%", (opacity * 100.0).round() as i32)),
                    ),
            )
            .into_any_element()
    }

    fn alignment_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = chrome(cx);
        icon_section(
            "Align",
            c.text_muted,
            [
                self.panel_icon_button(
                    "align-left",
                    "align-left",
                    include_bytes!("../../assets/align-left.svg"),
                    "Align Left",
                    PanelAction::AlignLeft,
                    cx,
                ),
                self.panel_icon_button(
                    "align-center-v",
                    "align-center-v",
                    include_bytes!("../../assets/align-center-v.svg"),
                    "Align Center (Vertical)",
                    PanelAction::AlignCenterVertical,
                    cx,
                ),
                self.panel_icon_button(
                    "align-right",
                    "align-right",
                    include_bytes!("../../assets/align-right.svg"),
                    "Align Right",
                    PanelAction::AlignRight,
                    cx,
                ),
                self.panel_icon_button(
                    "align-top",
                    "align-top",
                    include_bytes!("../../assets/align-top.svg"),
                    "Align Top",
                    PanelAction::AlignTop,
                    cx,
                ),
                self.panel_icon_button(
                    "align-center-h",
                    "align-center-h",
                    include_bytes!("../../assets/align-center-h.svg"),
                    "Align Center (Horizontal)",
                    PanelAction::AlignCenterHorizontal,
                    cx,
                ),
                self.panel_icon_button(
                    "align-bottom",
                    "align-bottom",
                    include_bytes!("../../assets/align-bottom.svg"),
                    "Align Bottom",
                    PanelAction::AlignBottom,
                    cx,
                ),
            ],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn panel_icon_button(
        &self,
        id: &'static str,
        icon_key: &'static str,
        icon_bytes: &'static [u8],
        tooltip: &'static str,
        action: PanelAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = chrome(cx);
        icon_button(id, false, false, c)
            .child(svg_icon(icon_key, icon_bytes, rgb(c.text).into(), 16.0))
            .tooltip(move |_window, cx| cx.new(|_| ToolTip::new(tooltip)).into())
            .on_click(cx.listener(move |view, _, _, cx| {
                view.apply_panel_action(action);
                cx.notify();
            }))
            .into_any_element()
    }

    fn apply_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::SendToBack => self.board.send_to_back(),
            PanelAction::SendBackward => self.board.send_backward(),
            PanelAction::BringForward => self.board.bring_forward(),
            PanelAction::BringToFront => self.board.bring_to_front(),
            PanelAction::FlipHorizontal => self.board.flip_horizontal(),
            PanelAction::FlipVertical => self.board.flip_vertical(),
            PanelAction::AlignLeft => self.board.align_left(),
            PanelAction::AlignCenterVertical => self.board.align_center_vertical(),
            PanelAction::AlignRight => self.board.align_right(),
            PanelAction::AlignTop => self.board.align_top(),
            PanelAction::AlignCenterHorizontal => self.board.align_center_horizontal(),
            PanelAction::AlignBottom => self.board.align_bottom(),
        }
    }

    fn supports_corner_radius(&self) -> bool {
        self.board.tool() == ToolKind::Rectangle
            || self.board.selected().iter().any(|id| {
                matches!(
                    self.board.canvas.document.get_shape(*id),
                    Some(Shape::Rectangle(_))
                )
            })
    }

    fn visible_corner_radius(&self) -> f64 {
        self.board
            .selected()
            .iter()
            .find_map(|id| match self.board.canvas.document.get_shape(*id) {
                Some(Shape::Rectangle(rectangle)) => Some(rectangle.corner_radius),
                _ => None,
            })
            .unwrap_or(self.board.canvas.tool_manager.corner_radius)
    }

    fn supports_linear(&self) -> bool {
        matches!(self.board.tool(), ToolKind::Line | ToolKind::Arrow)
            || self.board.selected().iter().any(|id| {
                matches!(
                    self.board.canvas.document.get_shape(*id),
                    Some(Shape::Line(_) | Shape::Arrow(_))
                )
            })
    }

    fn supports_freehand(&self) -> bool {
        matches!(
            self.board.tool(),
            ToolKind::Freehand | ToolKind::Highlighter
        ) || self.board.selected().iter().any(|id| {
            matches!(
                self.board.canvas.document.get_shape(*id),
                Some(Shape::Freehand(_))
            )
        })
    }

    fn supports_text(&self) -> bool {
        self.board.selected().iter().any(|id| {
            matches!(
                self.board.canvas.document.get_shape(*id),
                Some(Shape::Text(_))
            )
        })
    }

    fn selected_text_properties(&self) -> Option<f64> {
        self.board.selected().iter().find_map(|id| {
            let Some(Shape::Text(text)) = self.board.canvas.document.get_shape(*id) else {
                return None;
            };
            Some(text.font_size)
        })
    }

    fn selected_math_size(&self) -> Option<f64> {
        self.board.selected().iter().find_map(|id| {
            let Some(Shape::Math(math)) = self.board.canvas.document.get_shape(*id) else {
                return None;
            };
            Some(math.font_size)
        })
    }
}

fn section(
    title: &'static str,
    color: u32,
    buttons: impl IntoIterator<Item = AnyElement>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(section_label(title, color))
        .child(div().h(px(24.0)).flex().gap(px(4.0)).children(buttons))
        .into_any_element()
}

fn icon_section(
    title: &'static str,
    color: u32,
    buttons: impl IntoIterator<Item = AnyElement>,
) -> AnyElement {
    section(title, color, buttons)
}

fn section_label(title: &'static str, color: u32) -> AnyElement {
    div()
        .h(px(14.0))
        .text_size(px(11.0))
        .text_color(rgb(color))
        .child(title)
        .into_any_element()
}

fn is_property_drawing_tool(tool: ToolKind) -> bool {
    matches!(
        tool,
        ToolKind::Rectangle
            | ToolKind::Ellipse
            | ToolKind::Line
            | ToolKind::Arrow
            | ToolKind::Freehand
            | ToolKind::Highlighter
    )
}

#[cfg(test)]
#[path = "right_panel_tests.rs"]
mod tests;
