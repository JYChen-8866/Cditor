use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{AppContext, Context, Pixels, Point, Window};

use cditor_core::ids::{BlockId, SurfaceId};

use crate::app::input::text_drag::GuiTextDragSelection;
use crate::app::input_trace::trace_input;
#[cfg(test)]
use crate::app::interaction::geometry::ProjectedBlockRect;
use crate::app::interaction::table_mode::GuiTableInteractionMode;
use crate::app::platform_layout_cache::PlatformLayoutCache;
use crate::block::{CodeHighlightCache, MermaidRenderCache, WhiteboardThumbnailCache};

use crate::input::{AiPromptState, CodeLanguageEditState};
use crate::overlay::GuiToast;
use crate::overlay::SlashMenuState;
use crate::overlay::WhiteboardEditorSession;

use crate::persistence::EditorSaveStatus;
use crate::text::RichTextPlatformLayout;
#[cfg(test)]
use cditor_runtime::DocumentRuntime;
use cditor_session::EditorSessionHandle;

pub(in crate::app) mod ai;
mod block_actions;
mod code_language;
mod code_theme;
mod folding;
mod formatting;
mod platform_input;
mod slash_menu;
mod table_actions;
pub(crate) mod text_surface;
mod whiteboard;

pub(in crate::app) use super::persistence_bridge::save_status_for_mode;
pub use super::state::{CditorViewState, EditorReadonlyReason};
use super::state::{EditorStatusUiState, FocusUiState, InteractionUiState, PlatformInputState};
pub(crate) use crate::app::interaction::table_scroll::TableScrollSnapshot;
pub(in crate::app) use block_actions::block_focus_offset_after_missed_hit_test;
pub(in crate::app) use formatting::{
    SelectionToolbarDelay, floating_toolbar_passes_selection_delay, formatting_toolbar_context,
    formatting_toolbar_state,
};
pub(crate) use platform_input::GuiPlatformInputTarget;
#[cfg(test)]
pub(crate) use platform_input::platform_input_registration_allows;

pub struct CditorV2View {
    pub(in crate::app) state: CditorViewState,
    pub(in crate::app) focus: FocusUiState,
    pub(in crate::app) input: PlatformInputState,
    pub(in crate::app) ai_provider: Arc<dyn cditor_ai::AiProvider>,
    pub(in crate::app) ai_enabled: bool,
    pub(in crate::app) ai_prompt: Option<AiPromptState>,
    pub(in crate::app) ai_preview_scroll_handle: gpui::ScrollHandle,
    pub(in crate::app) show_debug: bool,
    pub(in crate::app) status: EditorStatusUiState,
    pub(in crate::app) interaction: InteractionUiState,
    pub(in crate::app) text_layouts: PlatformLayoutCache<BlockId>,
    pub(in crate::app) table_cell_layouts: PlatformLayoutCache<TableCellLayoutKey>,
    pub(in crate::app) text_surface_layouts: PlatformLayoutCache<SurfaceId>,
    pub(in crate::app) code_highlights: CodeHighlightCache,
    pub(in crate::app) mermaid_renders: MermaidRenderCache,
    pub(in crate::app) mermaid_source_blocks: std::collections::HashSet<BlockId>,
    pub(in crate::app) whiteboard_thumbnails: WhiteboardThumbnailCache,
    pub(in crate::app) whiteboard_editor: Option<WhiteboardEditorSession>,
    pub(in crate::app) code_language_edit: Option<CodeLanguageEditState>,
    pub(in crate::app) code_theme_menu_block_id: Option<BlockId>,
    pub(in crate::app) code_highlight_theme: &'static str,
    pub(in crate::app) slash_menu: Option<SlashMenuState>,
    pub(in crate::app) toast: Option<GuiToast>,
    pub(in crate::app) table_menu_ui: crate::block::table::menu::TableMenuUiState,
    pub(in crate::app) gutter_toolbar_block_id: Option<BlockId>,
    pub(in crate::app) selection_toolbar_delay: SelectionToolbarDelay,
    pub(in crate::app) block_transform_menu_open: bool,
    pub(in crate::app) color_menu_open: bool,
    pub(in crate::app) color_menu_hover_generation: u64,
    pub(in crate::app) color_menu_scroll_handle: gpui::ScrollHandle,
    pub(in crate::app) last_color_action: Option<crate::overlay::ColorMenuAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::app) struct TableCellLayoutKey {
    pub block_id: BlockId,
    pub row: usize,
    pub col: usize,
}

fn table_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_TABLE")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

fn trace_table(event: &str, details: impl std::fmt::Display) {
    if table_trace_enabled() {
        eprintln!("[cditor][table][gui][{event}] {details}");
    }
}

impl CditorV2View {
    pub(crate) fn toggle_mermaid_source_from_gui(
        &mut self,
        block_id: BlockId,
        cx: &mut Context<Self>,
    ) {
        crate::block::media::invalidate_rendered_media_height_report(block_id);
        if !self.mermaid_source_blocks.remove(&block_id) {
            self.mermaid_source_blocks.insert(block_id);
        }
        cx.notify();
    }

    pub(crate) fn copy_code_block_from_gui(&mut self, block_id: BlockId, cx: &mut Context<Self>) {
        if matches!(
            self.dispatch_command(
                cditor_editor_protocol::command::CditorCommand::CopyBlockText { block_id },
                cditor_editor_protocol::command::CommandSource::Toolbar,
                cx,
            ),
            Ok(outcome) if outcome.status == cditor_editor_protocol::command::CommandOutcomeStatus::Applied
        ) {
            self.show_toast("已将代码拷贝到剪贴板", Duration::from_secs(3), cx);
        }
    }

    fn show_toast(
        &mut self,
        message: impl Into<String>,
        duration: Duration,
        cx: &mut Context<Self>,
    ) {
        self.toast = Some(GuiToast::new(message, duration));
        let dismiss_after = cx.background_spawn(async move {
            std::thread::sleep(duration);
        });
        cx.spawn(async move |view, cx| {
            let _ = dismiss_after.await;
            let _ = view.update(cx, |view, cx| {
                let should_clear = view
                    .toast
                    .as_ref()
                    .is_some_and(|toast| !toast.is_alive(Instant::now()));
                if should_clear {
                    view.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn queue_rendered_media_height(
        &mut self,
        block_id: BlockId,
        content_version: u64,
        measured_height: f64,
        _cx: &mut Context<Self>,
    ) -> bool {
        self.ready_session()
            .and_then(|session| {
                session
                    .queue_measured_block_height(block_id, content_version, measured_height)
                    .ok()
            })
            .unwrap_or(false)
    }

    pub(crate) fn update_text_layout_cache(&mut self, cache: RichTextPlatformLayout) -> bool {
        let pinned_surface = self
            .input
            .layout_identity
            .map(|identity| identity.surface_id);
        if let Some(position) = cache.table_cell_position {
            trace_table(
                "cache.table_cell",
                format_args!(
                    "block={} row={} col={} content_version={} bounds=({}, {}, {}, {}) text_len={} lines={} accessibility={}",
                    cache.block_id,
                    position.row,
                    position.col,
                    cache.content_version,
                    f32::from(cache.bounds.left()),
                    f32::from(cache.bounds.top()),
                    f32::from(cache.bounds.size.width),
                    f32::from(cache.bounds.size.height),
                    cache.snapshot.text().len(),
                    cache.snapshot.line_count(),
                    cache.accessibility.is_some()
                ),
            );
            self.table_cell_layouts.insert(
                TableCellLayoutKey {
                    block_id: cache.block_id,
                    row: position.row,
                    col: position.col,
                },
                cache,
                pinned_surface,
            );
            return false;
        }
        if !matches!(cache.surface_id, SurfaceId::Block(_)) {
            self.text_surface_layouts
                .insert(cache.surface_id, cache, pinned_surface);
            return false;
        }
        let block_id = cache.block_id;
        let content_version = cache.content_version;
        let measured_height = cache.measured_height;
        self.text_layouts.insert(block_id, cache, pinned_surface);
        if self.ready_session().is_some_and(|session| {
            session
                .text_block_context(block_id)
                .ok()
                .flatten()
                .is_some_and(|context| {
                    context.kind == cditor_core::rich_text::RichBlockKind::Mermaid
                })
        }) {
            // Mermaid owns a stable preview/source box and reports its rendered
            // media height separately. Source text shaping must not overwrite it.
            return false;
        }
        self.ready_session()
            .and_then(|session| {
                session
                    .queue_measured_block_height(block_id, content_version, measured_height)
                    .ok()
            })
            .unwrap_or(false)
    }

    pub(crate) fn ready_session(&self) -> Option<&EditorSessionHandle> {
        match &self.state {
            CditorViewState::Ready(session) => Some(session),
            CditorViewState::Loading { .. } | CditorViewState::LoadFailed { .. } => None,
        }
    }

    pub(crate) fn focus_block_from_gui_at_position(
        &mut self,
        block_id: cditor_core::ids::BlockId,
        position: impl Into<Option<Point<Pixels>>>,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus.editor, cx);
        if self.interaction.table_interaction_mode.block_id().is_some() {
            self.interaction.table_interaction_mode = GuiTableInteractionMode::Idle;
            self.table_menu_ui = Default::default();
        }
        self.clear_gutter_action();
        let position = position.into();
        let text_position = position
            .and_then(|position| self.text_position_for_block_at_position(block_id, position));
        let click_selection =
            if let Some(kind) = crate::app::text_hit::selection_kind_for_click_count(click_count) {
                position.and_then(|position| {
                    let session = self.ready_session()?;
                    let current = session
                        .surface_version(SurfaceId::Block(block_id))
                        .ok()
                        .flatten()?;
                    let cache = self.current_text_layout_cache(current, block_id)?;
                    let local_x = f32::from(position.x - cache.bounds.left());
                    let local_y = f32::from(position.y - cache.bounds.top());
                    Some(cache.snapshot.selection_at_point(local_x, local_y, kind))
                })
            } else {
                None
            };
        trace_input(
            "focus_block_from_gui_at_position",
            format_args!(
                "block={block_id} position={position:?} resolved_position={text_position:?}"
            ),
        );
        if let CditorViewState::Ready(session) = &self.state {
            if let Some(selection) = click_selection {
                let _ = session.dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                    cditor_editor_protocol::command::CditorCommand::SetDocumentSelection {
                        selection: cditor_core::edit::DocumentSelection {
                            anchor: cditor_core::edit::TextPosition {
                                block_id,
                                offset: selection.anchor.offset,
                                affinity: selection.anchor.affinity,
                            },
                            focus: cditor_core::edit::TextPosition {
                                block_id,
                                offset: selection.focus.offset,
                                affinity: selection.focus.affinity,
                            },
                        },
                    },
                    cditor_editor_protocol::command::CommandSource::Toolbar,
                ));
                self.interaction.text_drag_selection = None;
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if let Some(text_position) = text_position {
                let _ = session.dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                    cditor_editor_protocol::command::CditorCommand::SetDocumentSelection {
                        selection: cditor_core::edit::DocumentSelection::caret(
                            cditor_core::edit::TextPosition {
                                block_id,
                                offset: text_position.offset,
                                affinity: text_position.affinity,
                            },
                        ),
                    },
                    cditor_editor_protocol::command::CommandSource::Toolbar,
                ));
                self.interaction.text_drag_selection = Some(GuiTextDragSelection {
                    anchor_block_id: block_id,
                    anchor_position: text_position,
                    pointer_position: position.unwrap_or_default(),
                });
            } else {
                let focused_block_id = session
                    .document_snapshot()
                    .ok()
                    .and_then(|snapshot| snapshot.focused_block_id);
                let caret_offset = session
                    .text_block_context(block_id)
                    .ok()
                    .flatten()
                    .and_then(|context| context.caret);
                let anchor_offset = block_focus_offset_after_missed_hit_test(
                    focused_block_id,
                    block_id,
                    caret_offset,
                );
                let _ = session.dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                    cditor_editor_protocol::command::CditorCommand::SetDocumentSelection {
                        selection: cditor_core::edit::DocumentSelection::caret(
                            cditor_core::edit::TextPosition::downstream(block_id, anchor_offset),
                        ),
                    },
                    cditor_editor_protocol::command::CommandSource::Toolbar,
                ));
                self.interaction.text_drag_selection = Some(GuiTextDragSelection {
                    anchor_block_id: block_id,
                    anchor_position: crate::text::ParleyTextPosition::downstream(anchor_offset),
                    pointer_position: position.unwrap_or_default(),
                });
            }
        }
        cx.notify();
    }

    pub(crate) fn toggle_todo_from_gui(&mut self, block_id: BlockId, cx: &mut Context<Self>) {
        let _ = self.dispatch_command(
            cditor_editor_protocol::command::CditorCommand::ToggleTodo { block_id },
            cditor_editor_protocol::command::CommandSource::Toolbar,
            cx,
        );
    }

    pub(crate) fn focus_down_placer_from_gui(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus.editor, cx);
        if self.status.readonly {
            return;
        }
        let result = self.dispatch_command(
            cditor_editor_protocol::command::CditorCommand::EnsureTrailingParagraph,
            cditor_editor_protocol::command::CommandSource::Toolbar,
            cx,
        );
        if result.is_ok()
            && let Some(session) = self.ready_session()
        {
            let _ = session.ensure_focused_block_visible();
        }
        match result {
            Ok(_) => {
                cx.notify();
            }
            Err(error) => {
                self.status.save_status = EditorSaveStatus::Failed(error.to_string());
                cx.notify();
            }
        }
    }

    pub(crate) fn hover_block_from_gui(
        &mut self,
        block_id: BlockId,
        dragging: bool,
        cx: &mut Context<Self>,
    ) {
        let hover_changed = self.interaction.hovered_block_id != Some(block_id);
        self.interaction.hovered_block_id = Some(block_id);
        let mut selection_changed = false;
        if dragging
            && self.interaction.block_drag_selection.is_dragging()
            && let CditorViewState::Ready(session) = &self.state
        {
            selection_changed = self
                .interaction
                .block_drag_selection
                .update(block_id, session);
        }
        if hover_changed || selection_changed {
            cx.notify();
        }
    }

    pub(in crate::app) fn clear_gutter_action(&mut self) {
        self.interaction.action_block_id = None;
        self.gutter_toolbar_block_id = None;
        self.block_transform_menu_open = false;
        self.color_menu_open = false;
        self.color_menu_hover_generation = self.color_menu_hover_generation.wrapping_add(1);
        self.interaction.gutter_block_drag = None;
        self.interaction.gutter_drag_auto_scroll_scheduled = false;
    }

    pub(crate) fn dismiss_gutter_toolbar_from_gui(&mut self, cx: &mut Context<Self>) -> bool {
        if self.gutter_toolbar_block_id.is_none() {
            return false;
        }
        self.clear_gutter_action();
        cx.notify();
        true
    }
}

#[cfg(test)]
#[path = "cditor_v2_view_tests.rs"]
mod cditor_v2_view_tests;

#[cfg(test)]
#[path = "cditor_v2_view_interaction_tests.rs"]
mod cditor_v2_view_interaction_tests;
