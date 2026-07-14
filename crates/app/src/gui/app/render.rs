use gpui::{
    Bounds, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Render, Styled,
    Window, div, point, px, rgb, size,
};

use crate::gui::GuiTheme;
use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState, formatting_toolbar_state};
use crate::gui::app::input::actions::BoundInputAction;
use crate::gui::app::interaction::geometry::{
    fallback_text_metrics_for_block, projected_block_rects_from_projection,
};
use crate::gui::app::interaction::scrollbar::{render_scrollbar, scrollbar_policy};
use crate::gui::app::interaction::table_scroll::TableScrollSnapshot;
use crate::gui::document::DEFAULT_DOCUMENT_PAGE_WIDTH_PX;
use crate::gui::document::DEFAULT_DOCUMENT_TOP_INSET_PX;
use crate::gui::document::{DocumentBlockActionProjection, DocumentEditorView};
use crate::gui::image_preview::render_image_preview_overlay;
use crate::gui::input::GuiInputCommand;
use crate::gui::input::actions::{
    Backspace, Backtab, CDITOR_KEY_CONTEXT, Cancel, Copy, Cut, Delete, Duplicate, MoveDown,
    MoveLeft, MoveRight, MoveToLineEnd, MoveToLineStart, MoveUp, Newline, NewlineBelow, Paste,
    Redo, SelectAll, SelectDown, SelectLeft, SelectRight, SelectToLineEnd, SelectToLineStart,
    SelectUp, SoftLineBreak, Tab, ToggleBold, ToggleInlineCode, ToggleItalic, ToggleUnderline,
    Undo,
};
use crate::gui::overlay::table::{table_hscroll_scroll_max, table_hscroll_track_width};
use crate::gui::overlay::{
    render_ai_preview_overlay, render_ai_prompt, render_floating_toolbar, render_slash_menu,
    render_toast, render_whiteboard_editor,
};
use crate::gui::persistence::{EditorLoadStateLabel, render_load_state};
use cditor_editor::scroll::HeightCorrectionPriority;
use cditor_runtime::AiRequestPresentation;

impl Render for CditorV2View {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiTheme::light();
        let focus = self.focus.clone();
        if self.ai_prompt.is_some() {
            if !self.ai_prompt_focus.is_focused(window) {
                window.focus(&self.ai_prompt_focus, cx);
            }
        } else if self.whiteboard_editor.is_none()
            && !focus.is_focused(window)
            && !self.code_language_focus.is_focused(window)
        {
            window.focus(&focus, cx);
        }
        self.begin_platform_input_registration_frame();

        let view = cx.entity();
        let code_language_edit = self.code_language_edit.clone();
        let code_theme_menu_block_id = self.code_theme_menu_block_id;
        let code_highlight_theme = self.code_highlight_theme;
        let mermaid_source_blocks = self.mermaid_source_blocks.clone();
        let embedded_ai_prompt = self.ai_prompt.as_ref().is_some_and(|prompt| {
            self.gutter_toolbar_block_id == Some(prompt.block_id)
                || (prompt.presentation == AiRequestPresentation::Automatic
                    && self
                        .ready_runtime_ref()
                        .is_some_and(|runtime| runtime.has_document_text_selection()))
        });
        let formatting_toolbar = formatting_toolbar_state(
            self.ready_runtime_ref(),
            &self.text_layouts,
            self.readonly,
            self.slash_menu.is_some()
                || code_language_edit.is_some()
                || code_theme_menu_block_id.is_some()
                || (self.ai_prompt.is_some() && !embedded_ai_prompt),
            window.viewport_size(),
            self.gutter_toolbar_block_id.filter(|_| {
                self.gutter_block_drag
                    .is_none_or(|drag| !drag.exceeded_threshold)
            }),
            self.block_transform_menu_open,
            self.color_menu_open,
            self.last_color_action,
            &self.projected_block_rects,
            self.ready_runtime_ref()
                .map(|runtime| runtime.scroll.global_scroll_top)
                .unwrap_or(0.0),
        );
        if formatting_toolbar.is_none() {
            self.color_menu_open = false;
        }
        let mut root = div()
            .id("cditor-v2-root")
            .relative()
            .key_context(CDITOR_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|view, _: &Newline, _window, cx| {
                view.handle_bound_input_action(BoundInputAction::Newline, cx)
            }))
            .on_action(cx.listener(|view, _: &SoftLineBreak, _window, cx| {
                view.handle_bound_input_action(BoundInputAction::SoftLineBreak, cx)
            }))
            .on_action(cx.listener(|view, _: &NewlineBelow, _window, cx| {
                view.handle_bound_input_action(BoundInputAction::NewlineBelow, cx)
            }))
            .on_action(cx.listener(|view, _: &Tab, _window, cx| {
                view.handle_bound_input_action(BoundInputAction::Tab { backwards: false }, cx)
            }))
            .on_action(cx.listener(|view, _: &Backtab, _window, cx| {
                view.handle_bound_input_action(BoundInputAction::Tab { backwards: true }, cx)
            }))
            .on_action(cx.listener(|view, _: &Cancel, _window, cx| {
                view.handle_bound_input_action(BoundInputAction::Cancel, cx)
            }))
            .on_action(cx.listener(|view, _: &MoveLeft, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveLeft {
                        extend_selection: false,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &MoveRight, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveRight {
                        extend_selection: false,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &MoveUp, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveUp {
                        extend_selection: false,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &MoveDown, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveDown {
                        extend_selection: false,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &SelectLeft, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveLeft {
                        extend_selection: true,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &SelectRight, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveRight {
                        extend_selection: true,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &SelectUp, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveUp {
                        extend_selection: true,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &SelectDown, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveDown {
                        extend_selection: true,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &MoveToLineStart, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveToLineStart {
                        extend_selection: false,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &MoveToLineEnd, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveToLineEnd {
                        extend_selection: false,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &SelectToLineStart, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveToLineStart {
                        extend_selection: true,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &SelectToLineEnd, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::MoveToLineEnd {
                        extend_selection: true,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &Backspace, _window, cx| {
                view.handle_bound_input_action(BoundInputAction::DeleteBackward, cx)
            }))
            .on_action(cx.listener(|view, _: &Delete, _window, cx| {
                view.handle_bound_input_action(BoundInputAction::DeleteForward, cx)
            }))
            .on_action(cx.listener(|view, _: &Duplicate, _window, cx| {
                view.handle_bound_input_action(BoundInputAction::Duplicate, cx)
            }))
            .on_action(cx.listener(|view, _: &SelectAll, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::SelectAllFocusedText),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &Copy, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::CopySelection),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &Cut, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::CutSelection),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &Paste, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::PasteClipboard),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &Undo, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::UndoFocusedBlock),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &Redo, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::RedoFocusedBlock),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &ToggleBold, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::ToggleBold),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &ToggleItalic, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::ToggleItalic),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &ToggleUnderline, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::ToggleUnderline),
                    cx,
                )
            }))
            .on_action(cx.listener(|view, _: &ToggleInlineCode, _window, cx| {
                view.handle_bound_input_action(
                    BoundInputAction::Command(GuiInputCommand::ToggleInlineCode),
                    cx,
                )
            }))
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

        let mut pending_table_scroll_offsets = Vec::new();
        let postgres_payload_pool = self
            .postgres_persistence
            .target()
            .map(|target| target.pool.clone());
        let mut pending_payload_window_load = None;
        let mut pending_payload_window_range = None;

        match &mut self.state {
            CditorViewState::Ready(runtime) => {
                let viewport_height = (f32::from(window.viewport_size().height)
                    - DEFAULT_DOCUMENT_TOP_INSET_PX)
                    .max(1.0) as f64;
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
                let has_missing_payloads = projection.render_window.is_placeholder()
                    || projection.blocks.iter().any(|block| block.placeholder);
                if postgres_payload_pool.is_some() && has_missing_payloads {
                    pending_payload_window_range =
                        Some(projection.render_window.block_range.clone());
                }
                self.code_highlights.sync_visible_window(
                    &projection,
                    self.code_highlight_theme,
                    cx,
                );
                self.mermaid_renders
                    .sync_visible_window(&projection, theme, cx);
                self.whiteboard_thumbnails
                    .sync_visible_window(&projection, theme, cx);
                let scrollbar_policy = scrollbar_policy(runtime);
                let scrollbar_visual = runtime.scrollbar_visual_state(scrollbar_policy);
                self.projected_block_rects = projected_block_rects_from_projection(&projection);
                let drag_overlay = self.block_drag_overlay_snapshot();
                let table_axis_selection = self.projected_table_axis_selection();
                let table_range_selection = self.projected_table_range_selection();
                let block_action = DocumentBlockActionProjection {
                    action_block_id: self.action_block_id,
                    dragging: self
                        .gutter_block_drag
                        .is_some_and(|drag| drag.exceeded_threshold)
                        || self.table_interaction_mode.is_dragging(),
                };
                let document_editor = DocumentEditorView::new(theme);
                let scrollbar_dragging = self.scrollbar_drag.is_some();
                // Pre-create persistent horizontal scroll handles for every table
                // block in the current window, then pass a read-only snapshot down
                // the render chain so each table can track scroll + draw its bar.
                let table_blocks = projection
                    .blocks
                    .iter()
                    .filter(|block| {
                        matches!(block.kind, cditor_core::rich_text::RichBlockKind::Table)
                    })
                    .filter_map(|block| {
                        block.table_view.as_ref().map(|table_view| {
                            (
                                block.block_id,
                                table_view.width_px,
                                table_view.horizontal_scroll_offset_px,
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                let mut table_scroll_snapshots = std::collections::HashMap::new();
                for (block_id, table_width_px, offset_x) in table_blocks {
                    let handle = self.table_scroll_handle(block_id, offset_x);
                    let viewport_measurement =
                        self.stable_table_viewport_measurement(block_id, &handle);
                    let mut projected_offset_x = offset_x;
                    if let Some(measurement) = viewport_measurement {
                        let track_width_px =
                            table_hscroll_track_width(measurement.viewport_width_px, 0.0);
                        let max_offset_x = table_hscroll_scroll_max(table_width_px, track_width_px);
                        projected_offset_x =
                            crate::gui::app::interaction::table_scroll::clamped_table_scroll_offset_x(
                                offset_x,
                                max_offset_x,
                            );
                        if projected_offset_x != offset_x {
                            pending_table_scroll_offsets.push((block_id, projected_offset_x));
                        }
                    }
                    self.table_scroll_state
                        .sync_handle_offset_x(block_id, projected_offset_x);
                    table_scroll_snapshots.insert(
                        block_id,
                        TableScrollSnapshot {
                            handle,
                            viewport_measurement,
                            offset_x: projected_offset_x,
                        },
                    );
                }
                root = root
                    .child(document_editor.render(
                        &projection,
                        view.clone(),
                        self.focus.clone(),
                        self.code_language_focus.clone(),
                        self.hovered_block_id,
                        drag_overlay,
                        block_action,
                        table_axis_selection,
                        &self.table_menu_ui,
                        self.readonly,
                        self.image_resize_preview(),
                        self.table_resize_preview(),
                        self.table_reorder_preview(),
                        table_range_selection,
                        code_language_edit.as_ref(),
                        code_theme_menu_block_id,
                        code_highlight_theme,
                        self.ai_prompt.is_some(),
                        &table_scroll_snapshots,
                        &self.code_highlights,
                        &self.mermaid_renders,
                        &mermaid_source_blocks,
                        &self.whiteboard_thumbnails,
                        cx,
                    ))
                    .child(render_scrollbar(
                        scrollbar_visual,
                        scrollbar_dragging,
                        theme,
                        cx.listener(Self::on_scrollbar_mouse_down),
                    ));
                let ai_preview_block_anchor = projection.ai_preview.as_ref().and_then(|preview| {
                    let mut document_top = projection.before_window_height;
                    projection.blocks.iter().find_map(|block| {
                        let block_height = block.layout.effective_height();
                        let result = (block.block_id == preview.block_id).then(|| {
                            let metrics = fallback_text_metrics_for_block(block, theme);
                            ai_preview_block_anchor(
                                document_top,
                                block_height,
                                metrics.origin_x_in_block_px,
                                metrics.width_px,
                                f32::from(window.viewport_size().width),
                                projection.scroll.global_scroll_top,
                            )
                        });
                        document_top += block_height;
                        result
                    })
                });
                if let Some(ai_preview) = render_ai_preview_overlay(
                    projection.ai_preview.as_ref(),
                    &self.text_layouts,
                    ai_preview_block_anchor,
                    theme,
                    view.clone(),
                    &self.ai_preview_scroll_handle,
                    f32::from(window.viewport_size().width),
                    f32::from(window.viewport_size().height),
                ) {
                    root = root.child(ai_preview);
                }
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
        if !pending_table_scroll_offsets.is_empty() {
            if let Some(runtime) = self.ready_runtime() {
                for (block_id, offset_x) in pending_table_scroll_offsets {
                    let _ = runtime.set_table_horizontal_scroll_offset_px(block_id, offset_x);
                }
            }
        }
        if let (Some(pool), Some(block_range)) =
            (postgres_payload_pool, pending_payload_window_range)
        {
            let activated_resident_window = self.ready_runtime().is_some_and(|runtime| {
                runtime.activate_payload_window_if_resident(block_range.clone())
            });
            if activated_resident_window {
                // This frame was projected before the cached range became active.
                // Replace the placeholder without issuing another database query.
                cx.notify();
            } else {
                match self
                    .payload_window_load_scheduler
                    .request(std::time::Instant::now())
                {
                    crate::gui::persistence::PayloadWindowLoadSchedule::DispatchNow => {
                        pending_payload_window_load = self.ready_runtime().and_then(|runtime| {
                            runtime.plan_payload_window_load_if_needed(block_range)
                        });
                    }
                    crate::gui::persistence::PayloadWindowLoadSchedule::WakeAfter(delay) => {
                        self.schedule_postgres_payload_window_wake(delay, cx);
                    }
                    crate::gui::persistence::PayloadWindowLoadSchedule::WakeAlreadyScheduled => {}
                }
            }
            if let Some(request) = pending_payload_window_load {
                self.load_postgres_payload_window(pool, request, cx);
            }
        }
        if let Some(toolbar) = formatting_toolbar {
            root = root.child(render_floating_toolbar(
                toolbar,
                theme,
                view,
                self.ai_prompt.as_ref().filter(|_| embedded_ai_prompt),
                self.ai_prompt_focus.clone(),
                &self.color_menu_scroll_handle,
            ));
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
        if !embedded_ai_prompt {
            if let Some(prompt) = self.ai_prompt.as_ref() {
                let viewport = window.viewport_size();
                root = root.child(render_ai_prompt(
                    prompt,
                    theme,
                    cx.entity(),
                    self.ai_prompt_focus.clone(),
                    f32::from(viewport.width),
                    f32::from(viewport.height),
                ));
            }
        }
        if let Some(toast) = self
            .toast
            .as_ref()
            .filter(|toast| toast.is_alive(std::time::Instant::now()))
        {
            root = root.child(render_toast(toast, theme));
        }
        if let Some(session) = self.whiteboard_editor.as_ref() {
            root = root.child(render_whiteboard_editor(session, theme, cx.entity()));
        }

        root
    }
}

fn ai_preview_block_anchor(
    document_top: f64,
    block_height: f64,
    text_origin_x: f64,
    text_width: f64,
    viewport_width: f32,
    scroll_top: f64,
) -> Bounds<gpui::Pixels> {
    let page_left = ((viewport_width - DEFAULT_DOCUMENT_PAGE_WIDTH_PX) / 2.0).max(0.0);
    let top = (document_top - scroll_top) as f32 + DEFAULT_DOCUMENT_TOP_INSET_PX;
    let height = block_height.max(24.0) as f32;
    Bounds::new(
        point(px(page_left + text_origin_x as f32), px(top)),
        size(px(text_width as f32), px(height)),
    )
}

#[cfg(test)]
mod ai_preview_position_tests {
    use super::*;

    #[test]
    fn ai_panel_anchor_tracks_projected_block_after_scroll() {
        let anchor = ai_preview_block_anchor(920.0, 48.0, 42.0, 760.0, 1200.0, 600.0);
        assert_eq!(f32::from(anchor.left()), 212.0);
        assert_eq!(f32::from(anchor.top()), 352.0);
        assert_eq!(f32::from(anchor.bottom()), 400.0);
    }
}
