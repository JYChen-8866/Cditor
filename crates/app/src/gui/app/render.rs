use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Render, Styled, Window,
    div, rgb,
};

use crate::gui::GuiTheme;
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::app::interaction::geometry::projected_block_rects_from_projection;
use crate::gui::app::interaction::scrollbar::{render_scrollbar, scrollbar_policy};
use crate::gui::document::{DocumentBlockActionProjection, DocumentEditorView};
use crate::gui::image_preview::render_image_preview_overlay;
use crate::gui::overlay::{render_slash_menu, render_toast};
use crate::gui::persistence::{EditorLoadStateLabel, render_load_state};
use cditor_editor::scroll::HeightCorrectionPriority;

impl Render for CditorV2View {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiTheme::light();
        let focus = self.focus.clone();
        if !focus.is_focused(window) && !self.code_language_focus.is_focused(window) {
            window.focus(&focus, cx);
        }
        self.begin_platform_input_registration_frame();

        let view = cx.entity();
        let code_language_edit = self.code_language_edit.clone();
        let mut root = div()
            .id("cditor-v2-root")
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .on_mouse_move(cx.listener(Self::on_scrollbar_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_scrollbar_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_scrollbar_mouse_up))
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.surface))
            .text_color(rgb(theme.text));

        match &mut self.state {
            CditorViewState::Ready(runtime) => {
                let viewport_height = f32::from(window.viewport_size().height) as f64;
                let _ = runtime.sync_viewport_height(viewport_height);
                self.scroll_accumulator
                    .maybe_mark_idle(std::time::Instant::now());
                let height_correction_priority = if self.scrollbar_drag.is_some() {
                    HeightCorrectionPriority::DeferUntilIdle
                } else {
                    self.scroll_accumulator.height_correction_priority()
                };
                let _ = runtime
                    .flush_pending_height_corrections_with_priority(height_correction_priority);
                let projection = runtime.projection_for_window_planned();
                let scrollbar_policy = scrollbar_policy(runtime);
                let scrollbar_visual = runtime.scrollbar_visual_state(scrollbar_policy);
                self.projected_block_rects = projected_block_rects_from_projection(&projection);
                let drag_overlay = self.block_drag_overlay_snapshot();
                let table_axis_selection = self.selected_table_axis;
                let block_action = DocumentBlockActionProjection {
                    action_block_id: self.action_block_id,
                    dragging: self
                        .gutter_block_drag
                        .is_some_and(|drag| drag.exceeded_threshold),
                };
                let document_editor = DocumentEditorView::new(theme);
                let scrollbar_dragging = self.scrollbar_drag.is_some();
                root = root
                    .child(document_editor.render(
                        &projection,
                        view,
                        self.focus.clone(),
                        self.code_language_focus.clone(),
                        self.hovered_block_id,
                        drag_overlay,
                        block_action,
                        table_axis_selection,
                        self.image_resize_preview(),
                        self.table_resize_preview(),
                        self.table_reorder_preview(),
                        self.selected_table_cell_range,
                        code_language_edit.as_ref(),
                        cx,
                    ))
                    .child(render_scrollbar(
                        scrollbar_visual,
                        scrollbar_dragging,
                        cx.listener(Self::on_scrollbar_mouse_down),
                    ));
            }
            CditorViewState::Loading { message } => {
                root = root.child(render_load_state(
                    &EditorLoadStateLabel::Loading(message.clone()),
                    theme,
                ));
            }
            CditorViewState::LoadFailed { message } => {
                root = root.child(render_load_state(
                    &EditorLoadStateLabel::Failed(message.clone()),
                    theme,
                ));
            }
        }
        if let Some(preview_overlay) = render_image_preview_overlay(window, cx) {
            root = root.child(preview_overlay);
        }
        if let Some(menu) = self.slash_menu.as_ref() {
            let viewport = window.viewport_size();
            root = root.child(render_slash_menu(
                menu,
                theme,
                cx.entity(),
                f32::from(viewport.width),
                f32::from(viewport.height),
            ));
        }
        if let Some(toast) = self
            .toast
            .as_ref()
            .filter(|toast| toast.is_alive(std::time::Instant::now()))
        {
            root = root.child(render_toast(toast));
        }

        root
    }
}
