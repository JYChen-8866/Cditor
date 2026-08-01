use crate::tools::ToolKind;
use gpui::{
    AnyElement, AppContext, Context, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};

use crate::theme::chrome;

use super::{
    DrafftBoardView,
    components::{button::tool_button, icon::svg_icon, tooltip::ToolTip},
};

const TOOLS: [ToolKind; 12] = [
    ToolKind::Select,
    ToolKind::Pan,
    ToolKind::Rectangle,
    ToolKind::Ellipse,
    ToolKind::Arrow,
    ToolKind::Line,
    ToolKind::Freehand,
    ToolKind::Highlighter,
    ToolKind::Eraser,
    ToolKind::Text,
    ToolKind::Math,
    ToolKind::LaserPointer,
];

impl DrafftBoardView {
    pub(super) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = chrome(cx);
        let active = self.board.tool();
        let buttons = TOOLS.into_iter().map(|tool| {
            let selected = tool == active;
            tool_button(("drafft-tool", tool_index(tool)), selected)
                .child(tool_icon(
                    tool,
                    if selected {
                        rgb(c.on_accent).into()
                    } else {
                        rgb(c.text).into()
                    },
                ))
                .tooltip(move |_window, cx| cx.new(|_| ToolTip::new(tool_label(tool))).into())
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !view.request_focus(window, cx) {
                        return;
                    }
                    view.finish_text_edit(cx);
                    view.board.set_tool(tool);
                    cx.notify();
                }))
                .into_any_element()
        });

        div()
            .absolute()
            .left(px(12.0))
            .top_1_2()
            .mt(px(-211.0))
            .w(px(50.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(c.border))
            .bg(rgb(c.bg))
            .shadow_sm()
            .occlude()
            .children(buttons)
    }
}

fn tool_icon(tool: ToolKind, color: Hsla) -> AnyElement {
    let (key, bytes) = match tool {
        ToolKind::Pan => (
            "drafft-icon-pan",
            include_bytes!("../../assets/pan.svg").as_slice(),
        ),
        ToolKind::Select => (
            "drafft-icon-select",
            include_bytes!("../../assets/select.svg").as_slice(),
        ),
        ToolKind::Freehand => (
            "drafft-icon-freehand",
            include_bytes!("../../assets/freehand.svg").as_slice(),
        ),
        ToolKind::Rectangle => (
            "drafft-icon-rectangle",
            include_bytes!("../../assets/rectangle.svg").as_slice(),
        ),
        ToolKind::Ellipse => (
            "drafft-icon-ellipse",
            include_bytes!("../../assets/ellipse.svg").as_slice(),
        ),
        ToolKind::Line => (
            "drafft-icon-line",
            include_bytes!("../../assets/line.svg").as_slice(),
        ),
        ToolKind::Arrow => (
            "drafft-icon-arrow",
            include_bytes!("../../assets/arrow.svg").as_slice(),
        ),
        ToolKind::Highlighter => (
            "drafft-icon-highlighter",
            include_bytes!("../../assets/highlighter.svg").as_slice(),
        ),
        ToolKind::Eraser => (
            "drafft-icon-eraser",
            include_bytes!("../../assets/eraser.svg").as_slice(),
        ),
        ToolKind::Text => (
            "drafft-icon-text",
            include_bytes!("../../assets/text.svg").as_slice(),
        ),
        ToolKind::Math => (
            "drafft-icon-math",
            include_bytes!("../../assets/math.svg").as_slice(),
        ),
        ToolKind::LaserPointer => (
            "drafft-icon-laser",
            include_bytes!("../../assets/laser.svg").as_slice(),
        ),
    };
    svg_icon(key, bytes, color, 17.0)
}

fn tool_label(tool: ToolKind) -> &'static str {
    match tool {
        ToolKind::Pan => "Pan",
        ToolKind::Select => "Select",
        ToolKind::Freehand => "Freehand",
        ToolKind::Rectangle => "Rectangle",
        ToolKind::Ellipse => "Ellipse",
        ToolKind::Line => "Line",
        ToolKind::Arrow => "Arrow",
        ToolKind::Highlighter => "Highlighter",
        ToolKind::Eraser => "Eraser",
        ToolKind::Text => "Text",
        ToolKind::Math => "Math",
        ToolKind::LaserPointer => "Laser",
    }
}

fn tool_index(tool: ToolKind) -> usize {
    TOOLS
        .iter()
        .position(|candidate| *candidate == tool)
        .unwrap()
}
