use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, MouseDownEvent, ParentElement,
    Styled, Window, div, px, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use cditor_editor::scroll::{ScrollbarPolicy, ScrollbarVisualState};
use cditor_runtime::DocumentRuntime;

const GUI_SCROLLBAR_WIDTH_PX: f32 = 10.0;
const GUI_SCROLLBAR_RIGHT_PX: f32 = 8.0;
const GUI_SCROLLBAR_THUMB_INSET_PX: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::gui::app) struct GuiScrollbarDrag {
    pub(in crate::gui::app) pointer_y_offset_in_thumb: f64,
}

pub(in crate::gui::app) fn scrollbar_policy(runtime: &DocumentRuntime) -> ScrollbarPolicy {
    ScrollbarPolicy {
        track_height: runtime.scroll.viewport_height.max(1.0),
        min_thumb_height: 24.0,
        local_list_state_scrollbar_enabled: false,
    }
}

pub(in crate::gui::app) fn render_scrollbar(
    visual: ScrollbarVisualState,
    dragging: bool,
    theme: GuiTheme,
    on_mouse_down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    if !visual.enabled {
        return div().into_any_element();
    }

    let thumb_color = scrollbar_thumb_color(theme, dragging);
    div()
        .absolute()
        .top_0()
        .right(px(GUI_SCROLLBAR_RIGHT_PX))
        .w(px(GUI_SCROLLBAR_WIDTH_PX))
        .h(px(visual.track_height as f32))
        .on_mouse_down(gpui::MouseButton::Left, on_mouse_down)
        .child(
            div()
                .absolute()
                .top(px(visual.thumb_top as f32))
                .left(px(GUI_SCROLLBAR_THUMB_INSET_PX))
                .right(px(GUI_SCROLLBAR_THUMB_INSET_PX))
                .h(px(visual.thumb_height as f32))
                .rounded(px((GUI_SCROLLBAR_WIDTH_PX
                    - GUI_SCROLLBAR_THUMB_INSET_PX * 2.0)
                    / 2.0))
                .bg(rgb(thumb_color))
                .hover(move |style| style.bg(rgb(theme.scrollbar_hover))),
        )
        .into_any_element()
}

fn scrollbar_thumb_color(theme: GuiTheme, dragging: bool) -> u32 {
    if dragging {
        theme.scrollbar_hover
    } else {
        theme.scrollbar
    }
}

impl CditorV2View {
    pub(in crate::gui::app) fn on_scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return;
        };
        let policy = scrollbar_policy(runtime);
        let visual = runtime.begin_scrollbar_drag(policy);
        if !visual.enabled {
            return;
        }
        let pointer_y = f64::from(event.position.y);
        let inside_thumb =
            visual.thumb_top <= pointer_y && pointer_y <= visual.thumb_top + visual.thumb_height;
        let pointer_y_offset_in_thumb = if inside_thumb {
            (pointer_y - visual.thumb_top).clamp(0.0, visual.thumb_height)
        } else {
            visual.thumb_height / 2.0
        };
        self.scrollbar_drag = Some(GuiScrollbarDrag {
            pointer_y_offset_in_thumb,
        });
        let _ = runtime.drag_scrollbar_to_thumb_top(policy, pointer_y - pointer_y_offset_in_thumb);
        cx.stop_propagation();
        cx.notify();
    }

    pub(in crate::gui::app) fn finish_gui_scrollbar_drag(&mut self, cx: &mut Context<Self>) {
        if self.scrollbar_drag.take().is_none() {
            return;
        }
        if let CditorViewState::Ready(runtime) = &mut self.state {
            let _ = runtime.finish_scrollbar_drag();
        }
        cx.stop_propagation();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_thumb_uses_notion_theme_states() {
        let theme = GuiTheme::light();

        assert_eq!(scrollbar_thumb_color(theme, false), theme.scrollbar);
        assert_eq!(scrollbar_thumb_color(theme, true), theme.scrollbar_hover);
    }
}
