use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use cditor_session::EditorSessionHandle;
use gpui::{App, AppContext, Context, Entity, FocusHandle, Subscription, Window};

use cditor_component::PopupMenu;

use cditor_core::block::GutterBlockDragState;
use cditor_core::ids::BlockId;

use crate::features::code::highlight::{DEFAULT_CODE_HIGHLIGHT_THEME_LIGHT, code_theme_for_mode};
use crate::input::BlockDragSelectionController;
use crate::input::{AiPromptState, CodeLanguageEditState};
use crate::interaction::geometry::{
    DocumentViewportOrigin, ProjectedBlockRect, ProjectedTableCellRect,
};
use crate::interaction::image_resize::GuiImageResizeDrag;
use crate::interaction::scrollbar::GuiScrollbarDrag;
use crate::interaction::selection_drag::GuiTextDragSelection;
use crate::interaction::table_mode::GuiTableInteractionMode;
use crate::interaction::table_reorder::GuiTableReorderDrag;
use crate::interaction::table_resize::GuiTableResizeDrag;
use crate::interaction::table_scroll::GuiTableScrollState;
#[cfg(feature = "whiteboard")]
use crate::overlays::WhiteboardEditorSession;
use crate::overlays::{GuiToast, SlashMenuState};
use crate::persistence::EditorSaveStatus;
use crate::scroll::ScrollAccumulator;
use crate::surfaces::table_cell::TableCellLayoutKey;
use crate::text::{CaretBlink, TextPlatformLayoutIdentity};
use crate::theme::GuiTheme;

use super::{CditorV2View, GuiPlatformInputTarget, SelectionToolbarDelay, ai::default_ai_provider};
use crate::app::{
    main_thread_scheduler::EditorMainThreadScheduler, worker_admission::EditorWorkerAdmission,
};

pub(crate) struct EditorDiagnosticsState {
    pub(crate) show_debug: bool,
}

impl EditorDiagnosticsState {
    pub(crate) const fn new(show_debug: bool) -> Self {
        Self { show_debug }
    }
}

pub(crate) struct FeatureUiState {
    pub(crate) ai_provider: Arc<dyn cditor_ai::AiProvider>,
    pub(crate) asset_provider: Option<Arc<dyn cditor_sdk::providers::AssetProvider>>,
    pub(crate) block_link_provider: Option<
        Arc<dyn Fn(BlockId) -> cditor_core::internal_link::BlockLinkPresentation + Send + Sync>,
    >,
    pub(crate) link_opener: Option<Arc<dyn Fn(&str, &mut Window, &mut App) -> bool>>,
    pub(crate) ai_enabled: bool,
    pub(crate) code_highlight_theme: &'static str,
    pub(crate) search_decorations: crate::features::search::SearchDecorationState,
    #[cfg(feature = "whiteboard")]
    pub(crate) whiteboard_editor: Option<WhiteboardEditorSession>,
}

impl Default for FeatureUiState {
    fn default() -> Self {
        Self {
            ai_provider: default_ai_provider(),
            asset_provider: None,
            block_link_provider: None,
            link_opener: None,
            ai_enabled: true,
            code_highlight_theme: DEFAULT_CODE_HIGHLIGHT_THEME_LIGHT,
            search_decorations: Default::default(),
            #[cfg(feature = "whiteboard")]
            whiteboard_editor: None,
        }
    }
}

impl FeatureUiState {
    /// Update code theme based on global theme mode
    pub(crate) fn sync_code_theme_with_global(&mut self, is_dark: bool) {
        self.code_highlight_theme = code_theme_for_mode(is_dark);
    }

    pub(crate) fn reset_session(&mut self) {
        self.search_decorations.replace(Vec::new());
        self.search_decorations.set_jump_highlight(None);
        #[cfg(feature = "whiteboard")]
        {
            self.whiteboard_editor = None;
        }
    }
}

#[derive(Default)]
pub(crate) struct OverlayUiState {
    pub(crate) ai_prompt: Option<AiPromptState>,
    pub(crate) ai_preview_scroll_handle: gpui::ScrollHandle,
    pub(crate) code_language_edit: Option<CodeLanguageEditState>,
    pub(crate) code_theme_menu_block_id: Option<BlockId>,
    pub(crate) code_copy_feedback_block_id: Option<BlockId>,
    pub(crate) code_copy_feedback_generation: u64,
    pub(crate) collapsed_code_blocks: HashSet<BlockId>,
    pub(crate) collapsed_code_block_heights: HashMap<BlockId, f64>,
    pub(crate) slash_menu: Option<SlashMenuState>,
    pub(crate) slash_popup_menu: Option<Entity<PopupMenu>>,
    pub(crate) slash_popup_menu_dismiss_subscription: Option<Subscription>,
    pub(crate) slash_callout_popup_menu: Option<Entity<PopupMenu>>,
    pub(crate) slash_callout_popup_menu_dismiss_subscription: Option<Subscription>,
    pub(crate) toast: Option<GuiToast>,
    pub(crate) table_menu_ui: crate::features::table::menu::TableMenuUiState,
    pub(crate) gutter_toolbar_block_id: Option<BlockId>,
    pub(crate) gutter_popup_menu: Option<Entity<PopupMenu>>,
    pub(crate) gutter_popup_menu_dismiss_subscription: Option<Subscription>,
    pub(crate) block_transform_popup_menu: Option<Entity<PopupMenu>>,
    pub(crate) block_transform_popup_menu_dismiss_subscription: Option<Subscription>,
    pub(crate) selection_toolbar_delay: SelectionToolbarDelay,
    pub(crate) block_transform_menu_open: bool,
    pub(crate) color_menu_open: bool,
    pub(crate) color_menu_hover_generation: u64,
    pub(crate) color_menu_scroll_handle: gpui::ScrollHandle,
    pub(crate) ai_actions_scroll_handle: gpui::ScrollHandle,
    pub(crate) last_color_action: Option<crate::overlays::ColorMenuAction>,
    pub(crate) page_icon_menu_open: bool,
    pub(crate) page_icon_menu_custom_tab: bool,
    pub(crate) page_icon_menu_scroll_handle: gpui::ScrollHandle,
}

impl OverlayUiState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn dismiss_topmost_formatting_layer(&mut self) -> bool {
        if self.color_menu_open {
            self.color_menu_open = false;
            self.color_menu_hover_generation = self.color_menu_hover_generation.wrapping_add(1);
            return true;
        }
        if self.block_transform_menu_open {
            self.block_transform_menu_open = false;
            self.block_transform_popup_menu = None;
            self.block_transform_popup_menu_dismiss_subscription = None;
            return true;
        }
        if self.gutter_toolbar_block_id.take().is_some() {
            self.gutter_popup_menu = None;
            self.gutter_popup_menu_dismiss_subscription = None;
            self.selection_toolbar_delay = SelectionToolbarDelay::default();
            return true;
        }
        false
    }
}

pub(crate) struct FocusUiState {
    pub(crate) editor: FocusHandle,
    pub(crate) code_language: FocusHandle,
    pub(crate) ai_prompt: FocusHandle,
    pub(crate) caret_blink: Entity<CaretBlink>,
    pub(crate) sdk_observers_registered: bool,
    pub(crate) last_emitted_selection: Option<cditor_sdk::document::DocumentSelection>,
    document_epoch: DocumentInteractionEpoch,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DocumentInteractionEpoch(u64);

#[derive(Default)]
pub(crate) struct EditorSchedulingState {
    pub(crate) main_thread: EditorMainThreadScheduler,
    pub(crate) workers: EditorWorkerAdmission,
    pub(crate) layout_correction_frame_scheduled: bool,
    payload_cache_trim_scheduled: bool,
    payload_cache_trim_requested_while_scheduled: bool,
}

impl EditorSchedulingState {
    pub(crate) fn request_payload_cache_trim(&mut self) -> bool {
        if self.payload_cache_trim_scheduled {
            self.payload_cache_trim_requested_while_scheduled = true;
            return false;
        }
        self.payload_cache_trim_scheduled = true;
        true
    }

    pub(crate) fn finish_payload_cache_trim_wait(&mut self) -> bool {
        self.payload_cache_trim_scheduled = false;
        std::mem::take(&mut self.payload_cache_trim_requested_while_scheduled)
    }

    pub(crate) fn schedule_layout_correction_frame(&mut self) -> bool {
        if self.layout_correction_frame_scheduled {
            return false;
        }
        self.layout_correction_frame_scheduled = true;
        true
    }

    pub(crate) fn finish_layout_correction_frame(&mut self) {
        self.layout_correction_frame_scheduled = false;
    }
}

impl DocumentInteractionEpoch {
    pub(crate) const fn current(self) -> u64 {
        self.0
    }

    fn advance(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }

    pub(crate) const fn matches(self, epoch: u64) -> bool {
        self.0 == epoch
    }
}

impl FocusUiState {
    pub(crate) fn new(cx: &mut Context<CditorV2View>) -> Self {
        let caret_blink = cx.new(|_| CaretBlink::new());
        Self {
            editor: cx.focus_handle(),
            code_language: cx.focus_handle(),
            ai_prompt: cx.focus_handle(),
            caret_blink,
            sdk_observers_registered: false,
            last_emitted_selection: None,
            document_epoch: DocumentInteractionEpoch::default(),
        }
    }

    pub(crate) fn reset_session_projection(&mut self) {
        self.document_epoch.advance();
        self.last_emitted_selection = None;
    }

    pub(crate) const fn document_epoch(&self) -> DocumentInteractionEpoch {
        self.document_epoch
    }
}

#[derive(Default)]
pub(crate) struct PlatformInputState {
    pub(crate) target: Option<GuiPlatformInputTarget>,
    pub(crate) session_identity: Option<cditor_runtime::InputSessionIdentity>,
    pub(crate) layout_identity: Option<TextPlatformLayoutIdentity>,
    pub(crate) element_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    pub(crate) candidate_bounds: Option<PlatformImeCandidateBounds>,
    pub(crate) character_coordinates_identity: Option<PlatformCharacterCoordinatesIdentity>,
    pub(crate) preferred_navigation_x: Option<(cditor_core::ids::SurfaceId, f32)>,
}

impl PlatformInputState {
    pub(crate) fn begin_registration_frame(&mut self, target: Option<GuiPlatformInputTarget>) {
        self.session_identity = None;
        self.layout_identity = None;
        self.element_bounds = None;
        self.target = target;
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlatformImeCandidateBounds {
    pub(crate) target: GuiPlatformInputTarget,
    pub(crate) bounds: gpui::Bounds<gpui::Pixels>,
    pub(crate) element_bounds: gpui::Bounds<gpui::Pixels>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlatformCharacterCoordinatesIdentity {
    pub(crate) target: GuiPlatformInputTarget,
    pub(crate) session_identity: Option<cditor_runtime::InputSessionIdentity>,
    pub(crate) layout_identity: TextPlatformLayoutIdentity,
    pub(crate) element_bounds: gpui::Bounds<gpui::Pixels>,
}

pub(crate) struct InteractionUiState {
    /// Theme used to build the projection currently presented on screen.
    pub(crate) presented_theme: GuiTheme,
    pub(crate) last_input_at: Option<web_time::Instant>,
    pub(crate) last_wheel_delta_y: f64,
    pub(crate) scroll_accumulator: ScrollAccumulator,
    wheel_frame_scheduled: bool,
    initial_viewport_frame_requested: bool,
    pub(crate) editor_viewport_handle: gpui::ScrollHandle,
    pub(crate) code_scroll_handles: HashMap<BlockId, gpui::ScrollHandle>,
    pub(crate) code_caret_reveal_after_line_break: HashSet<BlockId>,
    pub(crate) table_scroll_state: GuiTableScrollState,
    pub(crate) scrollbar_drag: Option<GuiScrollbarDrag>,
    pub(crate) text_drag_selection: Option<GuiTextDragSelection>,
    pub(crate) text_drag_auto_scroll_scheduled: bool,
    pub(crate) block_drag_selection: BlockDragSelectionController,
    pub(crate) table_interaction_mode: GuiTableInteractionMode,
    pub(crate) hovered_block_id: Option<BlockId>,
    pub(crate) action_block_id: Option<BlockId>,
    pub(crate) gutter_block_drag: Option<GutterBlockDragState>,
    pub(crate) gutter_drag_auto_scroll_scheduled: bool,
    pub(crate) image_resize_drag: Option<GuiImageResizeDrag>,
    pub(crate) table_resize_drag: Option<GuiTableResizeDrag>,
    pub(crate) table_reorder_drag: Option<GuiTableReorderDrag>,
    /// Scroll offset used to paint the projection currently on screen.
    /// This can intentionally trail the model scroll while a remote window is
    /// preparing; all hit testing must consume this value, not session scroll.
    pub(crate) presented_scroll_top: f64,
    /// Window-space origin of document coordinate `(0, 0)` for the projection
    /// currently on screen. Text layouts only own block-local geometry.
    pub(crate) document_viewport_origin: Option<DocumentViewportOrigin>,
    pub(crate) projected_block_rects: Vec<ProjectedBlockRect>,
    pub(crate) projected_table_cells: HashMap<TableCellLayoutKey, ProjectedTableCellRect>,
}

impl Default for InteractionUiState {
    fn default() -> Self {
        Self {
            presented_theme: GuiTheme::light(),
            last_input_at: None,
            last_wheel_delta_y: 0.0,
            scroll_accumulator: Default::default(),
            wheel_frame_scheduled: false,
            initial_viewport_frame_requested: false,
            editor_viewport_handle: Default::default(),
            code_scroll_handles: Default::default(),
            code_caret_reveal_after_line_break: Default::default(),
            table_scroll_state: Default::default(),
            scrollbar_drag: None,
            text_drag_selection: None,
            text_drag_auto_scroll_scheduled: false,
            block_drag_selection: Default::default(),
            table_interaction_mode: GuiTableInteractionMode::Idle,
            hovered_block_id: None,
            action_block_id: None,
            gutter_block_drag: None,
            gutter_drag_auto_scroll_scheduled: false,
            image_resize_drag: None,
            table_resize_drag: None,
            table_reorder_drag: None,
            presented_scroll_top: 0.0,
            document_viewport_origin: None,
            projected_block_rects: Vec::new(),
            projected_table_cells: HashMap::new(),
        }
    }
}

impl InteractionUiState {
    pub(crate) fn request_initial_viewport_frame(&mut self) -> bool {
        if self.initial_viewport_frame_requested {
            return false;
        }
        self.initial_viewport_frame_requested = true;
        true
    }

    pub(crate) fn note_viewport_measured(&mut self) {
        self.initial_viewport_frame_requested = false;
    }

    pub(crate) fn note_input(&mut self) {
        self.last_input_at = Some(web_time::Instant::now());
    }

    pub(crate) fn schedule_wheel_frame(&mut self) -> bool {
        if self.wheel_frame_scheduled {
            return false;
        }
        self.wheel_frame_scheduled = true;
        true
    }

    pub(crate) fn finish_wheel_frame(&mut self) {
        self.wheel_frame_scheduled = false;
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn cancel_document_drags(&mut self) -> bool {
        let had_active_drag = self.text_drag_selection.is_some()
            || self.gutter_block_drag.is_some()
            || self.image_resize_drag.is_some()
            || self.table_resize_drag.is_some()
            || self.table_reorder_drag.is_some()
            || self.table_interaction_mode.is_dragging();
        if !had_active_drag {
            return false;
        }
        self.text_drag_selection = None;
        self.text_drag_auto_scroll_scheduled = false;
        self.gutter_block_drag = None;
        self.gutter_drag_auto_scroll_scheduled = false;
        self.image_resize_drag = None;
        self.table_resize_drag = None;
        self.table_reorder_drag = None;
        self.table_interaction_mode = GuiTableInteractionMode::Idle;
        self.action_block_id = None;
        true
    }
}

pub(crate) struct EditorStatusUiState {
    pub(crate) readonly: bool,
    pub(crate) requested_readonly: bool,
    pub(crate) readonly_reason: Option<EditorReadonlyReason>,
    pub(crate) dirty: bool,
    pub(crate) save_status: EditorSaveStatus,
}

impl EditorStatusUiState {
    pub(crate) fn new(readonly: bool, requested_readonly: bool) -> Self {
        Self {
            readonly,
            requested_readonly,
            readonly_reason: None,
            dirty: false,
            save_status: super::save_status_for_mode(readonly),
        }
    }

    pub(crate) fn reset_for_session(&mut self, readonly: bool) {
        self.readonly = readonly;
        self.readonly_reason = None;
        self.dirty = false;
        self.save_status = super::save_status_for_mode(readonly);
    }

    pub(crate) fn reset_after_load_failure(&mut self) {
        self.readonly_reason = None;
        self.readonly = self.requested_readonly;
        self.dirty = false;
        self.save_status = super::save_status_for_mode(self.readonly);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_cache_trim_requests_coalesce_until_the_wait_finishes() {
        let mut scheduling = EditorSchedulingState::default();

        assert!(scheduling.request_payload_cache_trim());
        assert!(!scheduling.request_payload_cache_trim());

        assert!(scheduling.finish_payload_cache_trim_wait());
        assert!(scheduling.request_payload_cache_trim());
        assert!(!scheduling.finish_payload_cache_trim_wait());
    }

    #[test]
    fn layout_correction_frame_requests_are_coalesced() {
        let mut scheduling = EditorSchedulingState::default();

        assert!(scheduling.schedule_layout_correction_frame());
        assert!(!scheduling.schedule_layout_correction_frame());
        scheduling.finish_layout_correction_frame();
        assert!(scheduling.schedule_layout_correction_frame());
    }

    #[test]
    fn status_reset_preserves_host_request_and_clears_transient_save_state() {
        let mut status = EditorStatusUiState::new(true, false);
        status.readonly_reason = Some(EditorReadonlyReason::NewerDocumentSchema {
            written_major: 3,
            supported_major: 2,
        });
        status.dirty = true;
        status.save_status = EditorSaveStatus::Failed("injected".to_owned());

        status.reset_for_session(false);

        assert!(!status.readonly);
        assert!(!status.requested_readonly);
        assert!(status.readonly_reason.is_none());
        assert!(!status.dirty);
        assert_eq!(status.save_status, EditorSaveStatus::LocallySaved);
    }

    #[test]
    fn load_failure_restores_the_requested_readonly_mode() {
        let mut status = EditorStatusUiState::new(true, false);
        status.readonly_reason = Some(EditorReadonlyReason::NewerOperationSchema {
            written_major: 3,
            supported_major: 2,
        });

        status.reset_after_load_failure();

        assert!(!status.readonly);
        assert!(status.readonly_reason.is_none());
        assert_eq!(status.save_status, EditorSaveStatus::LocallySaved);
    }

    #[test]
    fn platform_input_reset_discards_session_bound_navigation_state() {
        let element_bounds = gpui::Bounds::new(
            gpui::point(gpui::px(12.0), gpui::px(24.0)),
            gpui::size(gpui::px(320.0), gpui::px(24.0)),
        );
        let target = GuiPlatformInputTarget::BlockText { block_id: 7 };
        let session_identity = cditor_runtime::InputSessionIdentity {
            session_id: 1,
            target_generation: 1,
            selection_generation: 2,
            composition_generation: 3,
            target: cditor_runtime::InputTarget::BlockText { block_id: 7 },
            content_version: 4,
        };
        let layout_identity = TextPlatformLayoutIdentity {
            surface_id: cditor_core::ids::SurfaceId::Block(7),
            content_version: 4,
            layout_version: 5,
            wrap_width_bits: 320.0_f32.to_bits(),
            text_align: cditor_core::rich_text::TextAlign::Start,
        };
        let mut input = PlatformInputState {
            candidate_bounds: Some(PlatformImeCandidateBounds {
                target,
                bounds: gpui::Bounds::new(
                    gpui::point(gpui::px(40.0), gpui::px(28.0)),
                    gpui::size(gpui::px(1.0), gpui::px(20.0)),
                ),
                element_bounds,
            }),
            character_coordinates_identity: Some(PlatformCharacterCoordinatesIdentity {
                target,
                session_identity: Some(session_identity),
                layout_identity,
                element_bounds,
            }),
            preferred_navigation_x: Some((cditor_core::ids::SurfaceId::Block(7), 42.0)),
            ..Default::default()
        };

        input.reset();

        assert!(input.target.is_none());
        assert!(input.session_identity.is_none());
        assert!(input.layout_identity.is_none());
        assert!(input.element_bounds.is_none());
        assert!(input.candidate_bounds.is_none());
        assert!(input.character_coordinates_identity.is_none());
        assert!(input.preferred_navigation_x.is_none());
    }

    #[test]
    fn platform_input_registration_frame_preserves_candidate_geometry_cache() {
        let target = GuiPlatformInputTarget::BlockText { block_id: 7 };
        let bounds = gpui::Bounds::new(
            gpui::point(gpui::px(40.0), gpui::px(28.0)),
            gpui::size(gpui::px(1.0), gpui::px(20.0)),
        );
        let element_bounds = gpui::Bounds::new(
            gpui::point(gpui::px(12.0), gpui::px(24.0)),
            gpui::size(gpui::px(320.0), gpui::px(24.0)),
        );
        let mut input = PlatformInputState {
            candidate_bounds: Some(PlatformImeCandidateBounds {
                target,
                bounds,
                element_bounds,
            }),
            session_identity: Some(cditor_runtime::InputSessionIdentity {
                session_id: 1,
                target_generation: 1,
                selection_generation: 1,
                composition_generation: 1,
                target: cditor_runtime::InputTarget::BlockText { block_id: 7 },
                content_version: 1,
            }),
            layout_identity: Some(TextPlatformLayoutIdentity {
                surface_id: cditor_core::ids::SurfaceId::Block(7),
                content_version: 1,
                layout_version: 1,
                wrap_width_bits: 320.0_f32.to_bits(),
                text_align: cditor_core::rich_text::TextAlign::Start,
            }),
            element_bounds: Some(element_bounds),
            target: Some(target),
            ..Default::default()
        };

        input.begin_registration_frame(None);

        assert!(input.session_identity.is_none());
        assert!(input.layout_identity.is_none());
        assert!(input.element_bounds.is_none());
        assert!(input.target.is_none());
        assert_eq!(
            input.candidate_bounds,
            Some(PlatformImeCandidateBounds {
                target,
                bounds,
                element_bounds,
            })
        );
    }

    #[test]
    fn interaction_reset_discards_drag_scroll_and_hit_test_state() {
        let mut interaction = InteractionUiState {
            last_wheel_delta_y: 18.0,
            text_drag_auto_scroll_scheduled: true,
            gutter_drag_auto_scroll_scheduled: true,
            hovered_block_id: Some(7),
            action_block_id: Some(8),
            code_caret_reveal_after_line_break: std::iter::once(10).collect(),
            table_interaction_mode: GuiTableInteractionMode::EditingCell {
                block_id: 9,
                row: 1,
                col: 2,
            },
            ..Default::default()
        };
        assert!(interaction.schedule_wheel_frame());

        interaction.reset();

        assert_eq!(interaction.last_wheel_delta_y, 0.0);
        assert!(interaction.schedule_wheel_frame());
        assert!(!interaction.schedule_wheel_frame());
        interaction.finish_wheel_frame();
        assert!(interaction.schedule_wheel_frame());
        assert!(!interaction.text_drag_auto_scroll_scheduled);
        assert!(!interaction.gutter_drag_auto_scroll_scheduled);
        assert!(interaction.hovered_block_id.is_none());
        assert!(interaction.action_block_id.is_none());
        assert!(interaction.code_caret_reveal_after_line_break.is_empty());
        assert!(interaction.document_viewport_origin.is_none());
        assert!(matches!(
            interaction.table_interaction_mode,
            GuiTableInteractionMode::Idle
        ));
        assert!(interaction.projected_block_rects.is_empty());
    }

    #[test]
    fn cancelling_document_drag_discards_preview_without_creating_a_commit() {
        let mut interaction = InteractionUiState {
            text_drag_auto_scroll_scheduled: true,
            gutter_drag_auto_scroll_scheduled: true,
            action_block_id: Some(7),
            table_interaction_mode: GuiTableInteractionMode::Resizing {
                block_id: 7,
                axis: crate::features::table::TableAxis::Column,
                index: 1,
            },
            ..Default::default()
        };

        assert!(interaction.cancel_document_drags());
        assert!(!interaction.cancel_document_drags());
        assert!(!interaction.text_drag_auto_scroll_scheduled);
        assert!(!interaction.gutter_drag_auto_scroll_scheduled);
        assert!(interaction.action_block_id.is_none());
        assert_eq!(
            interaction.table_interaction_mode,
            GuiTableInteractionMode::Idle
        );
    }

    #[test]
    fn overlay_reset_discards_document_bound_transient_state() {
        let mut overlay = OverlayUiState {
            code_theme_menu_block_id: Some(7),
            collapsed_code_blocks: std::iter::once(7).collect(),
            collapsed_code_block_heights: HashMap::from([(7, 386.0)]),
            gutter_toolbar_block_id: Some(8),
            block_transform_menu_open: true,
            color_menu_open: true,
            color_menu_hover_generation: 9,
            ..Default::default()
        };
        overlay.table_menu_ui.query = "status".to_owned();
        overlay.table_menu_ui.caret_offset = 6;
        overlay.table_menu_ui.marked_range = Some(0..6);

        overlay.reset();

        assert!(overlay.code_theme_menu_block_id.is_none());
        assert!(overlay.collapsed_code_blocks.is_empty());
        assert!(overlay.collapsed_code_block_heights.is_empty());
        assert!(overlay.gutter_toolbar_block_id.is_none());
        assert!(!overlay.block_transform_menu_open);
        assert!(!overlay.color_menu_open);
        assert_eq!(overlay.color_menu_hover_generation, 0);
        assert!(overlay.table_menu_ui.query.is_empty());
        assert_eq!(overlay.table_menu_ui.caret_offset, 0);
        assert!(overlay.table_menu_ui.marked_range.is_none());
    }

    #[test]
    fn formatting_overlay_escape_dismisses_only_the_topmost_layer() {
        let mut overlay = OverlayUiState {
            gutter_toolbar_block_id: Some(7),
            block_transform_menu_open: true,
            color_menu_open: true,
            color_menu_hover_generation: 3,
            ..Default::default()
        };

        assert!(overlay.dismiss_topmost_formatting_layer());
        assert!(!overlay.color_menu_open);
        assert!(overlay.block_transform_menu_open);
        assert_eq!(overlay.color_menu_hover_generation, 4);

        assert!(overlay.dismiss_topmost_formatting_layer());
        assert!(!overlay.block_transform_menu_open);
        assert_eq!(overlay.gutter_toolbar_block_id, Some(7));

        assert!(overlay.dismiss_topmost_formatting_layer());
        assert!(overlay.gutter_toolbar_block_id.is_none());
        assert!(!overlay.dismiss_topmost_formatting_layer());
    }

    #[test]
    fn feature_session_reset_preserves_host_configuration() {
        let mut features = FeatureUiState {
            ai_enabled: false,
            code_highlight_theme: "host-theme",
            ..Default::default()
        };

        features.reset_session();

        assert!(!features.ai_enabled);
        assert_eq!(features.code_highlight_theme, "host-theme");
        #[cfg(feature = "whiteboard")]
        assert!(features.whiteboard_editor.is_none());
    }

    #[test]
    fn diagnostics_state_preserves_the_host_debug_choice() {
        assert!(EditorDiagnosticsState::new(true).show_debug);
        assert!(!EditorDiagnosticsState::new(false).show_debug);
    }

    #[test]
    fn document_interaction_epoch_never_reuses_the_previous_session_identity() {
        let mut epoch = DocumentInteractionEpoch::default();
        let previous = epoch.current();

        epoch.advance();

        assert!(!epoch.matches(previous));
        assert!(epoch.matches(previous + 1));
    }

    #[test]
    fn initial_viewport_retry_is_single_shot_until_a_measurement_arrives() {
        let mut interaction = InteractionUiState::default();

        assert!(interaction.request_initial_viewport_frame());
        assert!(!interaction.request_initial_viewport_frame());

        interaction.note_viewport_measured();
        assert!(interaction.request_initial_viewport_frame());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorReadonlyReason {
    NewerDocumentSchema {
        written_major: u64,
        supported_major: u32,
    },
    NewerOperationSchema {
        written_major: u32,
        supported_major: u32,
    },
}

impl EditorReadonlyReason {
    pub fn message(&self) -> String {
        match self {
            Self::NewerDocumentSchema {
                written_major,
                supported_major,
            } => format!(
                "只读：文档格式 v{written_major} 高于当前支持的 v{supported_major}，请升级 Cditor 后编辑。"
            ),
            Self::NewerOperationSchema {
                written_major,
                supported_major,
            } => format!(
                "只读：恢复日志格式 v{written_major} 高于当前支持的 v{supported_major}，请升级 Cditor 后恢复。"
            ),
        }
    }
}

pub enum CditorViewState {
    Ready(EditorSessionHandle),
    Loading {
        message: String,
        progress: Option<u8>,
    },
    LoadFailed {
        message: String,
    },
}

impl CditorViewState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_load_failed(&self) -> bool {
        matches!(self, Self::LoadFailed { .. })
    }

    pub fn apply_loaded_session(&mut self, session: EditorSessionHandle) {
        *self = Self::Ready(session);
    }

    pub fn apply_load_progress(&mut self, message: impl Into<String>, progress: u8) -> bool {
        let Self::Loading {
            message: current_message,
            progress: current_progress,
        } = self
        else {
            return false;
        };
        let progress = progress.min(100);
        if current_progress.is_some_and(|current| progress < current) {
            return false;
        }
        *current_message = message.into();
        *current_progress = Some(progress);
        true
    }

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        *self = Self::LoadFailed {
            message: message.into(),
        };
    }
}
