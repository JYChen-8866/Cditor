use std::time::Instant;

use gpui::prelude::FluentBuilder;
use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Render, Styled, Window,
    div, rgb,
};

use crate::editor::scroll::HeightCorrectionPriority;
use crate::gui::GuiTheme;
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::app::interaction::geometry::projected_block_rects_from_projection;
use crate::gui::app::interaction::scrollbar::{render_scrollbar, scrollbar_policy};
use crate::gui::document::{
    DocumentBlockActionProjection, DocumentDebugHeader, DocumentEditorView,
};
use crate::gui::image_preview::render_image_preview_overlay;
use crate::gui::persistence::{EditorLoadStateLabel, render_load_state, render_save_indicator};

impl Render for CditorV2View {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let render_start = Instant::now();
        let theme = GuiTheme::light();
        let focus = self.focus.clone();
        if !focus.is_focused(window) {
            window.focus(&focus, cx);
        }

        let view = cx.entity();
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
            .text_color(rgb(theme.text))
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .py_2()
                    .bg(rgb(theme.page))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child("CDitor V2 Runtime GUI · 输入文本会写入当前 V2 DocumentRuntime · Tab 切换调试信息")
                    .child(render_save_indicator(&self.save_status, theme)),
            );

        match &mut self.state {
            CditorViewState::Ready(runtime) => {
                self.scroll_accumulator.maybe_mark_idle(Instant::now());
                let height_correction_priority = if self.scrollbar_drag.is_some() {
                    HeightCorrectionPriority::DeferUntilIdle
                } else {
                    self.scroll_accumulator.height_correction_priority()
                };
                let flush_start = Instant::now();
                let height_changed = runtime
                    .flush_pending_height_corrections_with_priority(height_correction_priority)
                    .unwrap_or(false);
                let flush_ms = flush_start.elapsed().as_secs_f64() * 1000.0;
                let projection_start = Instant::now();
                let projection = runtime.projection_for_window_planned();
                let focused_block_id = runtime.focused_block_id();
                let scrollbar_policy = scrollbar_policy(runtime);
                let scrollbar_visual = runtime.scrollbar_visual_state(scrollbar_policy);
                let projection_ms = projection_start.elapsed().as_secs_f64() * 1000.0;
                self.projected_block_rects = projected_block_rects_from_projection(&projection);
                let drag_overlay = self.block_drag_overlay_snapshot();
                let block_action = DocumentBlockActionProjection {
                    action_block_id: self.action_block_id,
                    dragging: self
                        .gutter_block_drag
                        .is_some_and(|drag| drag.exceeded_threshold),
                };
                eprintln!(
                    "[cditor][render] scroll_top={:.2} blocks={} window={:?} placeholder={} height_changed={} height_priority={:?} flush_ms={:.2} projection_ms={:.2}",
                    projection.scroll.global_scroll_top,
                    projection.blocks.len(),
                    projection.render_window.block_range,
                    projection.placeholder_window_height.is_some(),
                    height_changed,
                    height_correction_priority,
                    flush_ms,
                    projection_ms
                );
                let document_editor = DocumentEditorView::new(theme);
                let scrollbar_dragging = self.scrollbar_drag.is_some();
                let debug_header = DocumentDebugHeader::from_projection(
                    &projection,
                    self.last_wheel_delta_y,
                    focused_block_id,
                );
                root = root
                    .when(self.show_debug, |this| {
                        this.child(debug_header.render(theme))
                    })
                    .child(document_editor.render(
                        &projection,
                        view,
                        self.focus.clone(),
                        self.hovered_block_id,
                        drag_overlay,
                        block_action,
                        self.image_resize_preview(),
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

        let elapsed_ms = render_start.elapsed().as_secs_f64() * 1000.0;
        if elapsed_ms >= 1.0 {
            eprintln!("[cditor][render] total_elapsed_ms={elapsed_ms:.2}");
        }
        root
    }
}
