use cditor_component::{InteractiveScrollbar, InteractiveScrollbarStyle, ScrollbarAxis};
use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div, px};

use crate::editor_view::{CditorV2View, CditorViewState};
use crate::scroll::ScrollbarVisualState;
use crate::theme::GuiTheme;

const GUI_SCROLLBAR_RIGHT_PX: f32 = 8.0;
const PAGE_SCROLLBAR_TOP_PX: f32 = 0.0;
const INTERNAL_SCROLLBAR_WIDTH_PX: f32 = 5.0;
const INTERNAL_SCROLLBAR_TRACK_WIDTH_PX: f32 = 12.0;
const INTERNAL_SCROLLBAR_VERTICAL_INSET_PX: f32 = 3.0;
const INTERNAL_SCROLLBAR_MIN_THUMB_HEIGHT_PX: f32 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GuiScrollbarDrag;

pub(crate) fn render_scrollbar(
    visual: ScrollbarVisualState,
    viewport_height_px: f32,
    theme: GuiTheme,
    view: Entity<CditorV2View>,
) -> AnyElement {
    if !visual.enabled || viewport_height_px <= 0.5 {
        return div().into_any_element();
    }

    let start_view = view.clone();
    let change_view = view.clone();
    let end_view = view;
    let visible_fraction = (visual.thumb_height / visual.track_height).clamp(0.0, 1.0) as f32;
    let scrollbar = InteractiveScrollbar::for_callback(
        ScrollbarAxis::Vertical,
        visual.scroll_ratio as f32,
        1.0,
        visible_fraction,
        shared_vertical_scrollbar_style(theme),
        move |ratio, _window, cx| {
            change_view.update(cx, |view, cx| {
                view.drag_gui_scrollbar_to_ratio(f64::from(ratio), cx);
            });
        },
    )
    .id("document-scrollbar")
    .on_drag_start(move |_window, cx| {
        start_view.update(cx, |view, cx| view.begin_gui_scrollbar_drag(cx));
    })
    .on_drag_end(move |_window, cx| {
        end_view.update(cx, |view, cx| view.finish_gui_scrollbar_drag(cx));
    });

    div()
        .absolute()
        .top(px(PAGE_SCROLLBAR_TOP_PX))
        .right(px(GUI_SCROLLBAR_RIGHT_PX))
        .w(px(INTERNAL_SCROLLBAR_TRACK_WIDTH_PX))
        .h(px(page_scrollbar_track_height(viewport_height_px)))
        .child(scrollbar)
        .into_any_element()
}

fn shared_vertical_scrollbar_style(theme: GuiTheme) -> InteractiveScrollbarStyle {
    InteractiveScrollbarStyle {
        idle_thickness_px: INTERNAL_SCROLLBAR_WIDTH_PX,
        active_thickness_px: 8.0,
        hit_thickness_px: INTERNAL_SCROLLBAR_TRACK_WIDTH_PX,
        min_thumb_extent_px: INTERNAL_SCROLLBAR_MIN_THUMB_HEIGHT_PX,
        track_inset_px: INTERNAL_SCROLLBAR_VERTICAL_INSET_PX,
        thumb: theme.scrollbar,
        thumb_hover: theme.scrollbar_hover,
    }
}

fn page_scrollbar_track_height(editor_viewport_height_px: f32) -> f32 {
    editor_viewport_height_px.max(0.0)
}

impl CditorV2View {
    pub(crate) fn begin_gui_scrollbar_drag(&mut self, cx: &mut Context<Self>) {
        self.pause_caret_blink(cx);
        let CditorViewState::Ready(session) = &self.state else {
            return;
        };
        let Ok(visual) = session.start_scrollbar_drag() else {
            return;
        };
        if !visual.enabled {
            return;
        }
        // Entering thumb-drag mode changes the foreground policy. Release a
        // wheel/render request that may still own the single visible lane so the
        // first pointer movement can use the local frame-critical load directly.
        let _ = session.reset_payload_window_tasks();
        self.interaction.scrollbar_drag = Some(GuiScrollbarDrag);
        cx.notify();
    }

    pub(crate) fn drag_gui_scrollbar_to_ratio(&mut self, ratio: f64, cx: &mut Context<Self>) {
        if self.interaction.scrollbar_drag.is_none() {
            return;
        }
        self.pause_caret_blink(cx);
        let Some(session) = self.ready_session().cloned() else {
            return;
        };
        let moved = session
            .drag_scrollbar_to_ratio(ratio)
            .is_ok_and(|update| update.is_some());
        if moved
            && let Ok(Some(storage_request)) = session.payload_storage_request()
            && let Ok(block_range) = session.current_foreground_payload_range()
        {
            self.schedule_scrollbar_payload_window(storage_request, block_range, cx);
        }
        cx.notify();
    }

    pub(crate) fn finish_gui_scrollbar_drag(&mut self, cx: &mut Context<Self>) {
        if self.interaction.scrollbar_drag.take().is_none() {
            return;
        }
        self.pause_caret_blink(cx);
        if let CditorViewState::Ready(session) = &self.state {
            let _ = session.end_scrollbar_drag();
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_and_internal_scrollbars_share_the_same_component_style() {
        let theme = GuiTheme::light();
        let style = shared_vertical_scrollbar_style(theme);

        assert_eq!(style.idle_thickness_px, INTERNAL_SCROLLBAR_WIDTH_PX);
        assert_eq!(style.active_thickness_px, 8.0);
        assert_eq!(style.hit_thickness_px, INTERNAL_SCROLLBAR_TRACK_WIDTH_PX);
        assert_eq!(style.thumb, theme.scrollbar);
        assert_eq!(style.thumb_hover, theme.scrollbar_hover);
    }

    #[test]
    fn page_scrollbar_track_belongs_to_the_full_editor_viewport() {
        assert_eq!(PAGE_SCROLLBAR_TOP_PX, 0.0);
        assert_eq!(page_scrollbar_track_height(824.0), 824.0);
        assert_eq!(page_scrollbar_track_height(-1.0), 0.0);
    }

    #[test]
    fn page_scrollbar_track_uses_a_distinct_semantic_surface() {
        for theme in [GuiTheme::light(), GuiTheme::dark()] {
            assert_ne!(theme.scrollbar_track, theme.page);
            assert_ne!(theme.scrollbar_track, theme.scrollbar);
            assert_eq!(INTERNAL_SCROLLBAR_TRACK_WIDTH_PX, 12.0);
        }
    }

    #[test]
    fn internal_scrollbar_style_fits_its_dedicated_track() {
        const {
            assert!(INTERNAL_SCROLLBAR_WIDTH_PX < 8.0);
            assert!(8.0 <= INTERNAL_SCROLLBAR_TRACK_WIDTH_PX);
        }
        assert_eq!(INTERNAL_SCROLLBAR_MIN_THUMB_HEIGHT_PX, 28.0);
        assert_eq!(INTERNAL_SCROLLBAR_VERTICAL_INSET_PX, 3.0);
    }

    #[test]
    fn internal_scrollbar_uses_a_dedicated_track_wider_than_its_thumb() {
        assert_eq!(INTERNAL_SCROLLBAR_TRACK_WIDTH_PX, 12.0);
        assert_eq!(INTERNAL_SCROLLBAR_WIDTH_PX, 5.0);
        const {
            assert!(INTERNAL_SCROLLBAR_TRACK_WIDTH_PX > INTERNAL_SCROLLBAR_WIDTH_PX);
        }
    }
}
