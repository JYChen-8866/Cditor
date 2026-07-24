use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, MouseDownEvent, ParentElement,
    Styled, Window, div, px, rgb,
};

use crate::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::document::DEFAULT_DOCUMENT_TOP_INSET_PX;
use crate::scroll::ScrollbarVisualState;
use crate::theme::GuiTheme;

const GUI_SCROLLBAR_WIDTH_PX: f32 = 10.0;
const GUI_SCROLLBAR_RIGHT_PX: f32 = 8.0;
const GUI_SCROLLBAR_THUMB_INSET_PX: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GuiScrollbarDrag {
    pub(crate) pointer_y_offset_in_thumb: f64,
}

pub(crate) fn render_scrollbar(
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
        .top(px(DEFAULT_DOCUMENT_TOP_INSET_PX))
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
    pub(crate) fn on_scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let CditorViewState::Ready(session) = &self.state else {
            return;
        };
        let Ok(visual) = session.start_scrollbar_drag() else {
            return;
        };
        if !visual.enabled {
            return;
        }
        let pointer_y = scrollbar_local_pointer_y(f64::from(event.position.y));
        let inside_thumb =
            visual.thumb_top <= pointer_y && pointer_y <= visual.thumb_top + visual.thumb_height;
        let pointer_y_offset_in_thumb = if inside_thumb {
            (pointer_y - visual.thumb_top).clamp(0.0, visual.thumb_height)
        } else {
            visual.thumb_height / 2.0
        };
        self.interaction.scrollbar_drag = Some(GuiScrollbarDrag {
            pointer_y_offset_in_thumb,
        });
        let _ = session.drag_scrollbar(pointer_y - pointer_y_offset_in_thumb);
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn finish_gui_scrollbar_drag(&mut self, cx: &mut Context<Self>) {
        if self.interaction.scrollbar_drag.take().is_none() {
            return;
        }
        if let CditorViewState::Ready(session) = &self.state {
            let _ = session.end_scrollbar_drag();
        }
        cx.stop_propagation();
        cx.notify();
    }
}

pub(crate) fn scrollbar_local_pointer_y(window_pointer_y: f64) -> f64 {
    window_pointer_y - f64::from(DEFAULT_DOCUMENT_TOP_INSET_PX)
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

    #[test]
    fn scrollbar_pointer_coordinates_exclude_editor_top_inset() {
        assert_eq!(scrollbar_local_pointer_y(52.0), 20.0);
    }
}
