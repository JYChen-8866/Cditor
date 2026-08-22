use gpui::{
    Context, DismissEvent, ExternalPaths, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Render, StatefulInteractiveElement, Styled, Window, canvas, div,
    rgb,
};

use crate::document::{
    DocumentBlockActionProjection, DocumentEditorView, DocumentLayoutMetrics,
    PageDecorationSnapshot,
};
use crate::editor_view::{
    CditorV2View, CditorViewState, GuiPlatformInputTarget, floating_toolbar_passes_selection_delay,
    formatting_toolbar_context, formatting_toolbar_state,
};
use crate::image_preview::render_image_preview_overlay;
use crate::input::GuiInputCommand;
use crate::input::actions::{
    Backspace, Backtab, CDITOR_KEY_CONTEXT, Cancel, Copy, Cut, Delete, Duplicate, MoveDown,
    MoveLeft, MoveRight, MoveToDocumentEnd, MoveToDocumentStart, MoveToLineEnd, MoveToLineStart,
    MoveToNextWord, MoveToPreviousWord, MoveUp, Newline, NewlineBelow, Paste, Redo, SelectAll,
    SelectDown, SelectLeft, SelectRight, SelectToDocumentEnd, SelectToDocumentStart,
    SelectToLineEnd, SelectToLineStart, SelectToNextWord, SelectToPreviousWord, SelectUp,
    SoftLineBreak, Tab, ToggleBold, ToggleInlineCode, ToggleItalic, ToggleUnderline, Undo,
};
use crate::input::platform_adapter::{
    activate_mobile_text_input, finish_auxiliary_text_input, mobile_manual_focus,
};
use crate::input::routing::BoundInputAction;
use crate::interaction::geometry::projected_block_rects_from_projection;
use crate::interaction::scrollbar::render_scrollbar;
use crate::menu_metrics::EditorViewport;
#[cfg(feature = "whiteboard")]
use crate::overlays::render_whiteboard_editor;
use crate::overlays::{
    GUTTER_MENU_WIDTH_PX, build_block_transform_popup_menu, build_slash_callout_popup_menu,
    build_slash_popup_menu, gutter_popup_menu_style, render_ai_preview_overlay, render_ai_prompt,
    render_editor_context_menu, render_floating_toolbar, render_gutter_popup_menu,
    render_slash_menu, render_toast, show_editor_context_menu, update_gutter_popup_menu,
    update_slash_popup_menu,
};
use crate::persistence::{EditorLoadStateLabel, render_load_state};
use crate::platform::editor_ui_font_family;
use crate::scroll::HeightCorrectionPriority;
use crate::surfaces::table_cell::projected_table_cells_from_projection;
use crate::theme::{active_theme, is_dark_mode};
use cditor_runtime::AiRequestPresentation;
use cditor_session::RenderFrameRequest;

#[path = "render/ai_preview.rs"]
mod ai_preview;
use ai_preview::projected_ai_preview_block_anchor;
#[path = "render/internal_scroll.rs"]
mod internal_scroll;
use internal_scroll::prepare_internal_scroll_projection;
#[path = "render/payload_scheduling.rs"]
mod payload_scheduling;
use payload_scheduling::payload_frame_plan;
#[cfg(test)]
#[path = "render/focus_tests.rs"]
mod focus_tests;
mod status;

impl Render for CditorV2View {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let frame_started = web_time::Instant::now();
        self.run_main_thread_applies(frame_started, cx);
        let theme = active_theme(cx);
        self.interaction.presented_theme = theme;

        // Keep the Drafft whiteboard (canvas + chrome) in sync with the editor
        // theme. This must run before the whiteboard early-return below; the
        // board renders during this same pass and reads this global.
        #[cfg(feature = "whiteboard")]
        cditor_whiteboard_gpui::WhiteboardTheme::set(
            cx,
            cditor_whiteboard_gpui::WhiteboardTheme {
                page: theme.page,
                text: theme.text,
                muted: theme.muted,
                border: theme.border,
                panel: theme.panel,
                hover: theme.hover_surface,
                accent: theme.action_accent,
                on_accent: theme.checkbox_checked_text,
                ink: theme.text,
                grid: theme.border,
                danger: theme.danger,
            },
        );

        // Sync code theme with global theme
        self.features.sync_code_theme_with_global(is_dark_mode(cx));

        let focus = self.focus.editor.clone();
        if self.input.take_focus_dismissal_request() {
            finish_auxiliary_text_input(&focus, window, cx);
        } else if let Some(target) = self.input.take_focus_request() {
            let focus = match target {
                GuiPlatformInputTarget::AiPrompt { block_id }
                    if self
                        .overlay
                        .ai_prompt
                        .as_ref()
                        .is_some_and(|prompt| prompt.block_id == block_id) =>
                {
                    Some(self.focus.ai_prompt.clone())
                }
                GuiPlatformInputTarget::CodeLanguage { block_id }
                    if self
                        .overlay
                        .code_language_edit
                        .as_ref()
                        .is_some_and(|edit| edit.block_id == block_id) =>
                {
                    Some(self.focus.code_language.clone())
                }
                GuiPlatformInputTarget::LinkText { block_id }
                | GuiPlatformInputTarget::LinkUrl { block_id }
                    if self
                        .overlay
                        .link_edit
                        .as_ref()
                        .is_some_and(|edit| edit.block_id == block_id) =>
                {
                    Some(self.focus.link_edit.clone())
                }
                GuiPlatformInputTarget::TableMenuQuery { block_id }
                    if self
                        .interaction
                        .table_interaction_mode
                        .axis_selection()
                        .is_some_and(|selection| selection.block_id == block_id) =>
                {
                    Some(self.focus.editor.clone())
                }
                _ => None,
            };
            if let Some(focus) = focus {
                window.focus(&focus, cx);
                activate_mobile_text_input(window);
            }
        }
        self.set_caret_blink_enabled(focus.is_focused(window), cx);
        self.sdk_register_focus_observers(window, cx);
        self.sdk_emit_selection_if_changed(cx);
        self.begin_platform_input_registration_frame();

        #[cfg(feature = "whiteboard")]
        if let Some(session) = self.features.whiteboard_editor.as_ref() {
            let whiteboard = render_whiteboard_editor(session, theme, cx.entity());
            self.record_frame_telemetry(
                window.window_handle().window_id(),
                frame_started.elapsed(),
            );
            return div()
                .id("cditor-v2-whiteboard-root")
                .relative()
                .size_full()
                .child(whiteboard)
                .into_any_element();
        }

        let measured_editor_viewport_bounds = self
            .interaction
            .take_pending_editor_viewport_bounds()
            .unwrap_or_else(|| self.interaction.editor_viewport_handle.bounds());
        let measured_editor_viewport = EditorViewport::from_bounds(measured_editor_viewport_bounds);
        let editor_viewport = measured_editor_viewport
            .unwrap_or_else(|| EditorViewport::from_size(window.viewport_size()));
        let editor_viewport_origin = measured_editor_viewport_bounds.origin;
        self.interaction.note_editor_viewport_rendered(
            measured_editor_viewport.map(|_| measured_editor_viewport_bounds),
        );

        let view = cx.entity();
        let code_language_edit = self.overlay.code_language_edit.clone();
        let code_theme_menu_block_id = self.overlay.code_theme_menu_block_id;
        let code_highlight_theme = self.features.code_highlight_theme;
        let mermaid_source_blocks = self.cache.mermaid_source_blocks.clone();
        let mut formatting_context =
            formatting_toolbar_context(self.ready_session(), self.overlay.gutter_toolbar_block_id);
        let embedded_ai_prompt = self.overlay.ai_prompt.as_ref().is_some_and(|prompt| {
            self.overlay.gutter_toolbar_block_id == Some(prompt.block_id)
                || (prompt.presentation == AiRequestPresentation::Automatic
                    && formatting_context
                        .as_ref()
                        .is_some_and(|context| context.has_active_document_text_selection()))
        });
        let selection_toolbar_ready = self.sync_selection_toolbar_delay(cx);
        let accepts_external_video_drop = self.state.is_ready() && !self.status.readonly;
        // Cditor owns the moment an editable session begins. Registering the
        // native selection/IME handler must never open the iOS keyboard as a
        // side effect of rendering or scrolling.
        let mut root = mobile_manual_focus(
            div()
                .id("cditor-v2-root")
                .font_family(editor_ui_font_family())
                .relative()
                .overflow_hidden()
                .track_scroll(&self.interaction.editor_viewport_handle)
                .key_context(CDITOR_KEY_CONTEXT)
                .track_focus(&self.focus.editor),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|view, _event, _window, _cx| {
                view.overlay.page_icon_menu_open = false;
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                show_editor_context_menu(
                    view,
                    f32::from(event.position.x - editor_viewport_origin.x),
                    f32::from(event.position.y - editor_viewport_origin.y),
                    editor_viewport.width,
                    editor_viewport.height,
                    theme,
                    window,
                    cx,
                );
            }),
        )
        .can_drop(move |value, _window, _cx| {
            accepts_external_video_drop
                && value
                    .downcast_ref::<ExternalPaths>()
                    .is_some_and(crate::features::video::accepts_external_video_paths)
        })
        .on_drop::<ExternalPaths>(cx.listener(|view, paths, window, cx| {
            view.handle_external_video_drop(paths, window.mouse_position(), cx);
        }))
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
        .on_action(cx.listener(|view, _: &Cancel, window, cx| {
            if view.overlay.fullscreen_video_block_id.is_some() {
                view.exit_fullscreen_video(window, cx);
            } else {
                view.handle_bound_input_action(BoundInputAction::Cancel, cx)
            }
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
        .on_action(cx.listener(|view, _: &MoveToPreviousWord, _window, cx| {
            view.handle_bound_input_action(
                BoundInputAction::MoveToPreviousWord {
                    extend_selection: false,
                },
                cx,
            )
        }))
        .on_action(cx.listener(|view, _: &MoveToNextWord, _window, cx| {
            view.handle_bound_input_action(
                BoundInputAction::MoveToNextWord {
                    extend_selection: false,
                },
                cx,
            )
        }))
        .on_action(cx.listener(|view, _: &SelectToPreviousWord, _window, cx| {
            view.handle_bound_input_action(
                BoundInputAction::MoveToPreviousWord {
                    extend_selection: true,
                },
                cx,
            )
        }))
        .on_action(cx.listener(|view, _: &SelectToNextWord, _window, cx| {
            view.handle_bound_input_action(
                BoundInputAction::MoveToNextWord {
                    extend_selection: true,
                },
                cx,
            )
        }))
        .on_action(cx.listener(|view, _: &MoveToDocumentStart, _window, cx| {
            view.handle_bound_input_action(
                BoundInputAction::MoveToDocumentStart {
                    extend_selection: false,
                },
                cx,
            )
        }))
        .on_action(cx.listener(|view, _: &MoveToDocumentEnd, _window, cx| {
            view.handle_bound_input_action(
                BoundInputAction::MoveToDocumentEnd {
                    extend_selection: false,
                },
                cx,
            )
        }))
        .on_action(cx.listener(|view, _: &SelectToDocumentStart, _window, cx| {
            view.handle_bound_input_action(
                BoundInputAction::MoveToDocumentStart {
                    extend_selection: true,
                },
                cx,
            )
        }))
        .on_action(cx.listener(|view, _: &SelectToDocumentEnd, _window, cx| {
            view.handle_bound_input_action(
                BoundInputAction::MoveToDocumentEnd {
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

        // GPUI writes ScrollHandle bounds during prepaint. A cached embedded
        // view can therefore render once with the previous host width after a
        // dock or split-pane changes size. Observe the final root bounds and
        // invalidate exactly once so the next frame projects the document
        // against the new viewport instead of waiting for unrelated input.
        let viewport_observer = cx.entity();
        root = root.child(
            canvas(
                move |bounds, _, cx| {
                    viewport_observer.update(cx, |view, cx| {
                        if view.interaction.request_editor_viewport_refresh(bounds) {
                            cx.notify();
                        }
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        );

        #[cfg(feature = "mobile-text-session")]
        let mut root = root.on_pointer_cancel(cx.listener(
            |view, _event: &gpui::PointerCancelEvent, _window, cx| {
                if view.interaction.cancel_document_drags() {
                    cx.notify();
                }
            },
        ));

        if measured_editor_viewport.is_none() && self.state.is_ready() {
            // A fresh ScrollHandle receives its bounds during prepaint, after
            // this render pass. Laying out a ready document against the whole
            // window here makes embedded editors paint wide and then contract
            // into their dock on the next frame. Paint only the stable root on
            // this pass and project the document once its own bounds exist.
            crate::diagnostics::flash::trace(
                "frame.skipped",
                format_args!(
                    "reason=viewport-not-measured window_size={:?} — document area renders BLANK this frame",
                    window.viewport_size()
                ),
            );
            crate::text::sync_automatic_text_layout_pins(&[]);
            self.record_frame_telemetry(
                window.window_handle().window_id(),
                frame_started.elapsed(),
            );
            return root.into_any_element();
        }

        if self.state.is_ready()
            && !DocumentLayoutMetrics::viewport_width_is_usable(editor_viewport.width)
        {
            // A split-pane resize can expose a transient 1px-wide editor. Do
            // not turn that host-layout artifact into exact layouts for every
            // visible block; the next usable resize frame will project them.
            crate::diagnostics::flash::trace(
                "frame.skipped",
                format_args!(
                    "reason=viewport-width-unusable width={:?} — document area renders BLANK this frame",
                    editor_viewport.width
                ),
            );
            crate::text::sync_automatic_text_layout_pins(&[]);
            self.record_frame_telemetry(
                window.window_handle().window_id(),
                frame_started.elapsed(),
            );
            return root.into_any_element();
        }

        let mut pending_table_scroll_offsets = Vec::new();
        let payload_storage_request = self
            .ready_session()
            .and_then(|session| session.payload_storage_request().ok().flatten());
        let mut pending_payload_window_range = None;
        let mut pending_payload_prefetch_range = None;
        let document_snapshot = self
            .ready_session()
            .and_then(|session| session.document_snapshot().ok());
        let page_decorations = PageDecorationSnapshot {
            cover: document_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.cover.clone()),
            icon: document_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.icon.clone()),
            revision: document_snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.revision),
        };
        let document_layout = self.document_layout_metrics(editor_viewport.width);

        match &mut self.state {
            CditorViewState::Ready(session) => {
                let viewport_height =
                    (editor_viewport.height - document_layout.top_inset_px).max(1.0) as f64;
                self.interaction
                    .scroll_accumulator
                    .maybe_mark_idle(web_time::Instant::now());
                let height_correction_priority = if self.interaction.scrollbar_drag.is_some() {
                    HeightCorrectionPriority::DeferUntilIdle
                } else {
                    self.interaction
                        .scroll_accumulator
                        .height_correction_priority()
                };
                let frame = session
                    .render_frame(RenderFrameRequest {
                        viewport_height,
                        include_diagnostics: self.diagnostics.show_debug,
                        height_correction_priority,
                        min_scrollbar_thumb_height: 24.0,
                    })
                    .expect("ready editor session must project a render frame");
                crate::text::sync_automatic_text_layout_pins(&frame.automatic_text_layout_pins);
                let projection = frame.projection;
                self.interaction.presented_scroll_top = projection.scroll.global_scroll_top;
                self.sync_document_viewport_origin(editor_viewport, document_layout);
                self.prewarm_primary_text_layouts(
                    &projection,
                    document_layout,
                    editor_viewport.height,
                    theme,
                    window,
                    cx,
                );
                // Scheduling follows the desired viewport even while the
                // projection intentionally presents an older stable window.
                let payload_plan =
                    payload_frame_plan(&projection, payload_storage_request.is_some());
                pending_payload_window_range = payload_plan.visible;
                pending_payload_prefetch_range = self
                    .interaction
                    .scrollbar_drag
                    .is_none()
                    .then_some(payload_plan.prefetch)
                    .flatten();
                self.cache.code_highlights.sync_visible_window(
                    &projection,
                    self.features.code_highlight_theme,
                    &self.scheduling.workers,
                    cx,
                );
                self.cache.mermaid_renders.sync_visible_window(
                    &projection,
                    &mermaid_source_blocks,
                    theme,
                    &self.scheduling.workers,
                    cx,
                );
                self.cache.video_playbacks.sync_visible_window(
                    &projection,
                    self.features.asset_provider.clone(),
                    cx,
                );
                let deferred_whiteboard_entities = {
                    let scheduler = &mut self.scheduling.main_thread;
                    self.cache.whiteboard_thumbnails.sync_visible_window(
                        &projection,
                        theme,
                        self.status.readonly,
                        |cost| {
                            scheduler.try_admit_inline(
                                cditor_runtime::MainThreadWorkKind::WindowSwap,
                                cost,
                            )
                        },
                        cx,
                    )
                };
                if deferred_whiteboard_entities {
                    cx.notify();
                }
                let scrollbar_visual = frame.scrollbar_visual;
                let table_resize_preview = self.table_resize_preview();
                self.interaction.projected_block_rects =
                    projected_block_rects_from_projection(&projection, theme, document_layout);
                self.interaction.projected_table_cells =
                    projected_table_cells_from_projection(&projection, table_resize_preview);
                let drag_overlay = self.block_drag_overlay_snapshot();
                let table_axis_selection = self.projected_table_axis_visual_selection();
                let table_axis_menu_selection = self.projected_table_axis_selection();
                let table_cell_selection = self.projected_table_cell_selection();
                let table_range_selection = self.projected_table_range_selection();
                let block_action = DocumentBlockActionProjection {
                    action_block_id: self.interaction.action_block_id,
                    dragging: self
                        .interaction
                        .gutter_block_drag
                        .is_some_and(|drag| drag.exceeded_threshold)
                        || self.interaction.table_interaction_mode.is_dragging(),
                };
                let image_caption_states = projection
                    .blocks
                    .iter()
                    .filter_map(|block| {
                        let cditor_core::rich_text::BlockPayloadView::Loaded(payload) =
                            &block.payload
                        else {
                            return None;
                        };
                        if !matches!(
                            payload.payload,
                            cditor_core::rich_text::BlockPayload::Image(_)
                        ) {
                            return None;
                        }
                        self.text_surface_render_state(crate::surfaces::caption::surface_id(
                            block.block_id,
                        ))
                        .map(|state| (block.block_id, state))
                    })
                    .collect::<std::collections::HashMap<_, _>>();
                let document_editor = DocumentEditorView::new(theme);
                let internal_scroll = prepare_internal_scroll_projection(self, &projection);
                pending_table_scroll_offsets.extend(internal_scroll.corrected_table_scroll_offsets);
                root = root
                    .child(document_editor.render(
                        &projection,
                        &page_decorations,
                        self.page_chrome_extras.clone(),
                        !self.embedded_composer,
                        view.clone(),
                        &image_caption_states,
                        &self.scheduling.workers,
                        self.features.asset_provider.clone(),
                        self.focus.editor.clone(),
                        self.focus.code_language.clone(),
                        self.interaction.hovered_block_id,
                        drag_overlay,
                        block_action,
                        table_axis_selection,
                        table_axis_menu_selection,
                        table_cell_selection,
                        &self.overlay.table_menu_ui,
                        editor_viewport.width,
                        editor_viewport.height,
                        document_layout,
                        self.status.readonly,
                        self.image_resize_preview(),
                        table_resize_preview,
                        self.table_reorder_preview(),
                        table_range_selection,
                        code_language_edit.as_ref(),
                        code_theme_menu_block_id,
                        code_highlight_theme,
                        self.overlay.ai_prompt.is_some(),
                        &internal_scroll.table_scroll_snapshots,
                        &internal_scroll.code_scroll_handles,
                        &internal_scroll.code_caret_reveal_after_line_break,
                        &self.overlay.collapsed_code_blocks,
                        &self.cache.code_highlights,
                        &self.features.search_decorations,
                        &self.cache.mermaid_renders,
                        &mermaid_source_blocks,
                        &self.cache.video_playbacks,
                        &self.cache.whiteboard_thumbnails,
                        self.overlay.page_icon_menu_open,
                        self.overlay.page_icon_menu_custom_tab,
                        self.overlay.page_icon_menu_scroll_handle.clone(),
                        cx,
                    ))
                    .child(render_scrollbar(
                        scrollbar_visual,
                        editor_viewport.height,
                        theme,
                        view.clone(),
                    ));
                let ai_preview_block_anchor = projected_ai_preview_block_anchor(
                    &projection,
                    theme,
                    document_layout,
                    editor_viewport.width,
                );
                let ai_preview_text_anchor = projection.ai_preview.as_ref().and_then(|preview| {
                    let range = preview
                        .replacement_range
                        .clone()
                        .unwrap_or(preview.anchor_offset..preview.anchor_offset);
                    self.text_range_bounds_for_block(preview.block_id, range)
                });
                if let Some(ai_preview) = render_ai_preview_overlay(
                    projection.ai_preview.as_ref(),
                    ai_preview_text_anchor,
                    ai_preview_block_anchor,
                    theme,
                    view.clone(),
                    &self.overlay.ai_preview_scroll_handle,
                    editor_viewport,
                ) {
                    root = root.child(ai_preview);
                }
            }
            CditorViewState::Loading { message, progress } => {
                crate::diagnostics::flash::trace(
                    "frame.skipped",
                    format_args!("reason=state-loading message={message:?}"),
                );
                crate::text::sync_automatic_text_layout_pins(&[]);
                root = root.child(render_load_state(
                    &EditorLoadStateLabel::Loading {
                        detail: message.clone(),
                        progress: *progress,
                    },
                    theme,
                ));
            }
            CditorViewState::LoadFailed { message } => {
                crate::diagnostics::flash::trace(
                    "frame.skipped",
                    format_args!("reason=state-load-failed message={message:?}"),
                );
                crate::text::sync_automatic_text_layout_pins(&[]);
                root = root.child(render_load_state(
                    &EditorLoadStateLabel::Failed(message.clone()),
                    theme,
                ));
            }
        }
        self.refresh_text_overlay_anchors();
        if let Some(context) = formatting_context.as_mut() {
            context.global_scroll_top = self.interaction.presented_scroll_top;
        }
        let formatting_text_selection_bounds = formatting_context
            .as_ref()
            .and_then(|context| self.projected_toolbar_text_selection_bounds(context));
        let mut formatting_toolbar = formatting_toolbar_state(
            formatting_context.as_ref(),
            formatting_text_selection_bounds,
            self.status.readonly,
            self.overlay.slash_menu.is_some()
                || self.overlay.link_edit.is_some()
                || code_language_edit.is_some()
                || code_theme_menu_block_id.is_some()
                || (self.overlay.ai_prompt.is_some() && !embedded_ai_prompt),
            editor_viewport,
            self.overlay.gutter_toolbar_block_id.filter(|_| {
                self.interaction
                    .gutter_block_drag
                    .is_none_or(|drag| !drag.exceeded_threshold)
            }),
            self.overlay.block_transform_menu_open,
            self.overlay.color_menu_open,
            self.overlay.last_color_action,
            &self.interaction.projected_block_rects,
        );
        if formatting_toolbar.as_ref().is_some_and(|toolbar| {
            !floating_toolbar_passes_selection_delay(
                toolbar.has_text_selection,
                selection_toolbar_ready,
            )
        }) {
            formatting_toolbar = None;
        }
        if let Some(toolbar) = formatting_toolbar.as_mut() {
            toolbar.ai_enabled &= self.features.ai_enabled;
        }
        if formatting_toolbar.is_none() {
            self.overlay.color_menu_open = false;
        }
        if !pending_table_scroll_offsets.is_empty()
            && let Some(session) = self.ready_session()
        {
            for (block_id, offset_x) in pending_table_scroll_offsets {
                let _ = session.set_table_horizontal_scroll_offset(block_id, offset_x);
            }
        }
        if let Some(storage_request) = payload_storage_request {
            if let Some(block_range) = pending_payload_window_range {
                self.schedule_storage_payload_window(storage_request.clone(), block_range, cx);
            }
            if let Some(block_range) = pending_payload_prefetch_range {
                self.schedule_storage_payload_prefetch(storage_request, block_range, cx);
            }
        }
        if let Some(toolbar) = formatting_toolbar {
            if toolbar.show_delete {
                let block_transform_popup_menu = if toolbar.block_transform_menu_open {
                    if let Some(menu) = self.overlay.block_transform_popup_menu.clone() {
                        Some(menu)
                    } else {
                        let menu = build_block_transform_popup_menu(
                            window,
                            cx,
                            gutter_popup_menu_style(theme),
                            view.clone(),
                            toolbar.block_id.expect("gutter toolbar has a block target"),
                            toolbar.block_transform,
                            toolbar.callout_variant,
                            toolbar.block_transform_availability,
                        );
                        let subscription = cx.subscribe(
                            &menu,
                            |view: &mut CditorV2View, _, _: &DismissEvent, cx| {
                                view.overlay.block_transform_menu_open = false;
                                view.overlay.block_transform_popup_menu = None;
                                view.overlay.block_transform_popup_menu_dismiss_subscription = None;
                                cx.notify();
                            },
                        );
                        let menu_focus = menu.read(cx).menu_focus_handle();
                        menu_focus.focus(window, cx);
                        self.overlay.block_transform_popup_menu = Some(menu.clone());
                        self.overlay.block_transform_popup_menu_dismiss_subscription =
                            Some(subscription);
                        Some(menu)
                    }
                } else {
                    self.overlay.block_transform_popup_menu = None;
                    self.overlay.block_transform_popup_menu_dismiss_subscription = None;
                    None
                };
                let menu = if let Some(menu) = self.overlay.gutter_popup_menu.clone() {
                    menu
                } else {
                    let menu = cditor_component::PopupMenu::build(window, cx, |menu, _, _| {
                        menu.style(gutter_popup_menu_style(theme))
                            .action_context(self.focus.editor.clone())
                            .min_w(gpui::px(GUTTER_MENU_WIDTH_PX))
                            .max_w(gpui::px(GUTTER_MENU_WIDTH_PX))
                    });
                    let subscription =
                        cx.subscribe(&menu, |view: &mut CditorV2View, _, _: &DismissEvent, cx| {
                            view.clear_gutter_action();
                            cx.notify();
                        });
                    let menu_focus = menu.read(cx).menu_focus_handle();
                    menu_focus.focus(window, cx);
                    self.overlay.gutter_popup_menu = Some(menu.clone());
                    self.overlay.gutter_popup_menu_dismiss_subscription = Some(subscription);
                    menu
                };
                menu.update(cx, |menu, _| {
                    update_gutter_popup_menu(
                        menu,
                        toolbar,
                        theme,
                        view.clone(),
                        self.overlay
                            .ai_prompt
                            .as_ref()
                            .filter(|_| embedded_ai_prompt)
                            .cloned(),
                        self.focus.ai_prompt.clone(),
                        self.overlay.color_menu_scroll_handle.clone(),
                        self.overlay.ai_actions_scroll_handle.clone(),
                        block_transform_popup_menu,
                    );
                });
                root = root.child(render_gutter_popup_menu(toolbar, menu));
            } else {
                root = root.child(render_floating_toolbar(
                    toolbar,
                    theme,
                    view,
                    self.overlay
                        .ai_prompt
                        .as_ref()
                        .filter(|_| embedded_ai_prompt),
                    self.focus.ai_prompt.clone(),
                    &self.overlay.color_menu_scroll_handle,
                    &self.overlay.ai_actions_scroll_handle,
                ));
            }
        } else if self.overlay.gutter_toolbar_block_id.is_none() {
            self.overlay.gutter_popup_menu = None;
            self.overlay.gutter_popup_menu_dismiss_subscription = None;
        }
        root = root.children(self.render_status_overlays(theme, cx.entity()));
        if let Some(preview_overlay) = render_image_preview_overlay(window, cx) {
            root = root.child(preview_overlay);
        }
        let slash_state = self.overlay.slash_menu.clone();
        let slash_callout_popup_menu = if slash_state
            .as_ref()
            .is_some_and(|menu| menu.callout_submenu_open)
        {
            if let Some(menu) = self.overlay.slash_callout_popup_menu.clone() {
                Some(menu)
            } else {
                let slash_view = cx.entity();
                let menu = build_slash_callout_popup_menu(
                    window,
                    cx,
                    theme,
                    slash_view,
                    self.focus.editor.clone(),
                );
                let subscription =
                    cx.subscribe(&menu, |view: &mut CditorV2View, _, _: &DismissEvent, cx| {
                        if let Some(slash_menu) = view.overlay.slash_menu.as_mut() {
                            slash_menu.callout_submenu_open = false;
                        }
                        view.overlay.slash_callout_popup_menu = None;
                        view.overlay.slash_callout_popup_menu_dismiss_subscription = None;
                        cx.notify();
                    });
                let menu_focus = menu.read(cx).menu_focus_handle();
                menu_focus.focus(window, cx);
                self.overlay.slash_callout_popup_menu = Some(menu.clone());
                self.overlay.slash_callout_popup_menu_dismiss_subscription = Some(subscription);
                Some(menu)
            }
        } else {
            self.overlay.slash_callout_popup_menu = None;
            self.overlay.slash_callout_popup_menu_dismiss_subscription = None;
            None
        };
        if let Some(slash_state) = slash_state {
            let slash_popup_menu = if let Some(menu) = self.overlay.slash_popup_menu.clone() {
                menu
            } else {
                let menu = build_slash_popup_menu(window, cx, theme, self.focus.editor.clone());
                let subscription =
                    cx.subscribe(&menu, |view: &mut CditorV2View, _, _: &DismissEvent, cx| {
                        view.cancel_slash_menu(cx);
                    });
                self.overlay.slash_popup_menu = Some(menu.clone());
                self.overlay.slash_popup_menu_dismiss_subscription = Some(subscription);
                menu
            };
            let slash_view = cx.entity();
            slash_popup_menu.update(cx, |menu, _| {
                update_slash_popup_menu(
                    menu,
                    slash_state.clone(),
                    theme,
                    slash_view,
                    editor_viewport,
                    slash_callout_popup_menu,
                );
            });
            root = root.child(render_slash_menu(
                &slash_state,
                slash_popup_menu,
                editor_viewport,
            ));
        } else {
            self.overlay.slash_popup_menu = None;
            self.overlay.slash_popup_menu_dismiss_subscription = None;
        }
        if !embedded_ai_prompt && let Some(prompt) = self.overlay.ai_prompt.as_ref() {
            root = root.child(render_ai_prompt(
                prompt,
                theme,
                cx.entity(),
                self.focus.ai_prompt.clone(),
                editor_viewport,
            ));
        }
        if let Some(link_edit) = self.overlay.link_edit.as_ref() {
            root = root.child(crate::overlays::render_link_edit_popup(
                link_edit,
                theme,
                cx.entity(),
                self.focus.link_edit.clone(),
                editor_viewport.width,
                editor_viewport.height,
            ));
        }
        if let Some(toast) = self
            .overlay
            .toast
            .as_ref()
            .filter(|toast| toast.is_alive(web_time::Instant::now()))
        {
            root = root.child(render_toast(toast, theme));
        }
        self.reconcile_fullscreen_video_window(window);
        if let Some(block_id) = self.overlay.fullscreen_video_block_id {
            let window_viewport = window.viewport_size();
            if let Some(overlay) = crate::features::video::render_fullscreen_video_overlay(
                block_id,
                &self.cache.video_playbacks,
                theme,
                cx.entity(),
                window_viewport.width.as_f32(),
                window_viewport.height.as_f32(),
                cx,
            ) {
                root = root.child(overlay);
            } else {
                self.exit_fullscreen_video(window, cx);
            }
        }
        if let (Some(menu), Some(position)) = (
            self.overlay.editor_context_menu.clone(),
            self.overlay.editor_context_menu_position,
        ) {
            root = root.child(render_editor_context_menu(position, menu));
        }
        self.record_frame_telemetry(window.window_handle().window_id(), frame_started.elapsed());

        root.into_any_element()
    }
}
