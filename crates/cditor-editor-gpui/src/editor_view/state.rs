use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use cditor_session::EditorSessionHandle;
use gpui::{App, AppContext, Bounds, Context, Entity, FocusHandle, Pixels, Subscription, Window};

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
    pub(crate) link_edit: Option<crate::input::link_edit::LinkEditState>,
    pub(crate) code_theme_menu_block_id: Option<BlockId>,
    pub(crate) code_copy_feedback_block_id: Option<BlockId>,
    pub(crate) code_copy_feedback_generation: u64,
    pub(crate) collapsed_code_blocks: HashSet<BlockId>,
    pub(crate) collapsed_code_block_heights: HashMap<BlockId, f64>,
    /// 收起/展开进行中的高度补间。空表示所有代码块都已稳定。
    ///
    /// 折叠高度要喂进布局引擎，下方每个块的位置都跟着它走，所以补间期间每帧推一个
    /// 新高度。落定后条目移除，块回到由 `collapsed_code_blocks` 决定的静态高度。
    pub(crate) code_collapse_tweens: HashMap<BlockId, crate::features::code::CodeCollapseTween>,
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
    pub(crate) editor_context_menu: Option<Entity<PopupMenu>>,
    pub(crate) editor_context_menu_position: Option<(f32, f32)>,
    pub(crate) editor_context_menu_dismiss_subscription: Option<Subscription>,
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
    pub(crate) fullscreen_video_block_id: Option<BlockId>,
    pub(crate) fullscreen_video_requested_window: bool,
    pub(crate) fullscreen_video_observed_window: bool,
}

impl OverlayUiState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn dismiss_topmost_formatting_layer(&mut self) -> bool {
        if self.editor_context_menu.take().is_some() {
            self.editor_context_menu_position = None;
            self.editor_context_menu_dismiss_subscription = None;
            return true;
        }
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
    pub(crate) link_edit: FocusHandle,
    pub(crate) ai_prompt: FocusHandle,
    pub(crate) caret_blink: Entity<CaretBlink>,
    pub(crate) caret_motion: crate::text::CaretMotion,
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
            link_edit: cx.focus_handle(),
            ai_prompt: cx.focus_handle(),
            caret_blink,
            caret_motion: crate::text::CaretMotion::default(),
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
    pub(crate) hitbox_id: Option<gpui::HitboxId>,
    pub(crate) candidate_bounds: Option<PlatformImeCandidateBounds>,
    pub(crate) character_coordinates_identity: Option<PlatformCharacterCoordinatesIdentity>,
    pub(crate) preferred_navigation_x: Option<(cditor_core::ids::SurfaceId, f32)>,
    pending_focus_target: Option<GuiPlatformInputTarget>,
    pending_focus_dismissal: bool,
    /// UIKit owns this range while native handles are being dragged. Keep it
    /// separate from the document caret so UIKit does not collapse its own
    /// selection on the next `selectedTextRange` query.
    native_selection_range: Option<(GuiPlatformInputTarget, std::ops::Range<usize>, bool)>,
    native_selection_candidate: Option<(GuiPlatformInputTarget, usize)>,
}

impl PlatformInputState {
    pub(crate) fn begin_registration_frame(&mut self, target: Option<GuiPlatformInputTarget>) {
        self.session_identity = None;
        self.layout_identity = None;
        self.element_bounds = None;
        self.hitbox_id = None;
        self.target = target;
        if self
            .pending_focus_target
            .is_some_and(|pending| Some(pending) != target)
        {
            self.pending_focus_target = None;
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn request_focus(&mut self, target: GuiPlatformInputTarget) {
        self.target = Some(target);
        self.set_native_selection_target(target);
        self.pending_focus_dismissal = false;
        self.pending_focus_target = Some(target);
    }

    pub(crate) fn take_focus_request(&mut self) -> Option<GuiPlatformInputTarget> {
        self.pending_focus_target.take()
    }

    pub(crate) fn clear_focus_request(&mut self, target: GuiPlatformInputTarget) {
        if self.pending_focus_target == Some(target) {
            self.pending_focus_target = None;
        }
    }

    pub(crate) fn request_focus_dismissal(&mut self) {
        self.pending_focus_target = None;
        self.pending_focus_dismissal = true;
    }

    pub(crate) fn cancel_focus_dismissal(&mut self) {
        self.pending_focus_dismissal = false;
    }

    pub(crate) fn take_focus_dismissal_request(&mut self) -> bool {
        std::mem::take(&mut self.pending_focus_dismissal)
    }

    pub(crate) fn set_native_selection(
        &mut self,
        target: GuiPlatformInputTarget,
        range: std::ops::Range<usize>,
        reversed: bool,
    ) {
        self.native_selection_range = Some((target, range, reversed));
        self.native_selection_candidate = None;
    }

    pub(crate) fn native_selection_for(
        &self,
        target: Option<GuiPlatformInputTarget>,
    ) -> Option<gpui::UTF16Selection> {
        let (selection_target, range, reversed) = self.native_selection_range.as_ref()?;
        (Some(*selection_target) == target).then(|| gpui::UTF16Selection {
            range: range.clone(),
            reversed: *reversed,
        })
    }

    pub(crate) fn clear_native_selection(&mut self) {
        self.native_selection_range = None;
        self.native_selection_candidate = None;
    }

    pub(crate) fn set_native_selection_target(&mut self, target: GuiPlatformInputTarget) {
        if self
            .native_selection_range
            .as_ref()
            .is_some_and(|(selection_target, _, _)| *selection_target != target)
        {
            self.clear_native_selection();
        }
        if self
            .native_selection_candidate
            .as_ref()
            .is_some_and(|(candidate_target, _)| *candidate_target != target)
        {
            self.native_selection_candidate = None;
        }
    }

    pub(crate) fn set_native_selection_candidate(
        &mut self,
        target: GuiPlatformInputTarget,
        offset_utf16: usize,
    ) {
        self.native_selection_candidate = Some((target, offset_utf16));
    }

    pub(crate) fn native_selection_candidate_for(
        &self,
        target: Option<GuiPlatformInputTarget>,
    ) -> Option<usize> {
        let (candidate_target, offset) = self.native_selection_candidate?;
        (Some(candidate_target) == target).then_some(offset)
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
    rendered_editor_viewport_bounds: Option<Bounds<Pixels>>,
    pending_editor_viewport_bounds: Option<Bounds<Pixels>>,
    #[cfg(test)]
    editor_viewport_correction_requests: usize,
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
            rendered_editor_viewport_bounds: None,
            pending_editor_viewport_bounds: None,
            #[cfg(test)]
            editor_viewport_correction_requests: 0,
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
    /// Supplies the final host bounds before Cditor builds the current frame.
    /// Unlike the post-layout observer path, this never requests another
    /// frame: the pending measurement is consumed by the render that follows
    /// immediately in `CditorHostElement::prepaint`.
    pub(crate) fn prepare_editor_viewport_for_render(&mut self, bounds: Bounds<Pixels>) {
        self.pending_editor_viewport_bounds = Some(bounds);
    }

    pub(crate) fn rendered_editor_viewport_bounds(&self) -> Option<Bounds<Pixels>> {
        self.rendered_editor_viewport_bounds
    }

    #[cfg(test)]
    pub(crate) fn editor_viewport_correction_requests(&self) -> usize {
        self.editor_viewport_correction_requests
    }

    pub(crate) fn note_editor_viewport_rendered(&mut self, bounds: Option<Bounds<Pixels>>) {
        self.rendered_editor_viewport_bounds = bounds;
        self.pending_editor_viewport_bounds = None;
    }

    pub(crate) fn take_pending_editor_viewport_bounds(&mut self) -> Option<Bounds<Pixels>> {
        self.pending_editor_viewport_bounds.take()
    }

    /// Returns true once for each host layout that differs from the viewport
    /// used to build the current document projection.
    pub(crate) fn request_editor_viewport_refresh(&mut self, bounds: Bounds<Pixels>) -> bool {
        if self.rendered_editor_viewport_bounds == Some(bounds) {
            self.pending_editor_viewport_bounds = None;
            return false;
        }
        if self.pending_editor_viewport_bounds == Some(bounds) {
            return false;
        }
        self.pending_editor_viewport_bounds = Some(bounds);
        #[cfg(test)]
        {
            self.editor_viewport_correction_requests += 1;
        }
        true
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
    pub(crate) host_active: bool,
}

impl EditorStatusUiState {
    pub(crate) fn new(readonly: bool, requested_readonly: bool) -> Self {
        Self {
            readonly,
            requested_readonly,
            readonly_reason: None,
            dirty: false,
            save_status: super::save_status_for_mode(readonly),
            host_active: true,
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
    fn native_selection_is_owned_by_the_registered_surface() {
        let block = GuiPlatformInputTarget::BlockText { block_id: 7 };
        let cell = GuiPlatformInputTarget::TableCell {
            block_id: 7,
            row: 0,
            col: 0,
        };
        let mut input = PlatformInputState::default();

        input.set_native_selection(block, 3..11, false);
        assert_eq!(
            input.native_selection_for(Some(block)).map(|s| s.range),
            Some(3..11)
        );
        assert!(input.native_selection_for(Some(cell)).is_none());

        input.begin_registration_frame(None);
        assert_eq!(
            input.native_selection_for(Some(block)).map(|s| s.range),
            Some(3..11)
        );

        input.set_native_selection_target(cell);
        assert!(input.native_selection_for(Some(block)).is_none());

        input.set_native_selection_candidate(cell, 4);
        assert_eq!(input.native_selection_candidate_for(Some(cell)), Some(4));
        input.clear_native_selection();
        assert!(input.native_selection_candidate_for(Some(cell)).is_none());
    }

    #[test]
    fn native_selection_candidate_is_scoped_to_each_document_text_surface() {
        let targets = [
            GuiPlatformInputTarget::BlockText { block_id: 7 },
            GuiPlatformInputTarget::TableCell {
                block_id: 7,
                row: 1,
                col: 2,
            },
            GuiPlatformInputTarget::ImageCaption { block_id: 8 },
            GuiPlatformInputTarget::CollectionTitle { block_id: 9 },
        ];
        let mut input = PlatformInputState::default();

        for (index, target) in targets.into_iter().enumerate() {
            let offset = index + 3;
            input.set_native_selection_candidate(target, offset);

            assert_eq!(
                input.native_selection_candidate_for(Some(target)),
                Some(offset)
            );
            for other_target in targets.into_iter().filter(|other| *other != target) {
                assert!(
                    input
                        .native_selection_candidate_for(Some(other_target))
                        .is_none()
                );
            }
        }
    }

    #[test]
    fn clearing_native_selection_does_not_change_document_registration() {
        let block = GuiPlatformInputTarget::BlockText { block_id: 7 };
        let mut input = PlatformInputState::default();
        input.begin_registration_frame(Some(block));
        input.set_native_selection(block, 2..5, true);
        input.clear_native_selection();

        assert_eq!(input.target, Some(block));
        assert!(input.native_selection_for(Some(block)).is_none());
    }

    #[test]
    fn auxiliary_target_switch_clears_document_native_selection_state() {
        let block = GuiPlatformInputTarget::BlockText { block_id: 7 };
        let auxiliary_targets = [
            GuiPlatformInputTarget::ai_prompt(7),
            GuiPlatformInputTarget::code_language(7),
            GuiPlatformInputTarget::table_menu_query(7),
        ];
        let mut input = PlatformInputState::default();

        for target in auxiliary_targets {
            input.set_native_selection(block, 2..5, false);
            input.set_native_selection_candidate(block, 3);
            input.set_native_selection_target(target);

            assert!(input.native_selection_for(Some(block)).is_none());
            assert!(input.native_selection_candidate_for(Some(block)).is_none());
            assert!(input.native_selection_for(Some(target)).is_none());
            assert!(input.native_selection_candidate_for(Some(target)).is_none());
        }
    }

    #[test]
    fn registration_frame_discards_stale_surface_hitbox() {
        let block = GuiPlatformInputTarget::BlockText { block_id: 7 };
        let mut input = PlatformInputState::default();
        input.target = Some(block);
        input.hitbox_id = Some(gpui::HitboxId::placeholder());

        input.begin_registration_frame(Some(block));

        assert_eq!(input.target, Some(block));
        assert!(input.hitbox_id.is_none());
    }

    #[test]
    fn auxiliary_focus_request_is_consumed_once() {
        let target = GuiPlatformInputTarget::ai_prompt(7);
        let mut input = PlatformInputState::default();

        input.request_focus(target);

        assert_eq!(input.target, Some(target));
        assert_eq!(input.take_focus_request(), Some(target));
        assert_eq!(input.take_focus_request(), None);
    }

    #[test]
    fn registration_target_change_discards_stale_focus_request() {
        let prompt = GuiPlatformInputTarget::ai_prompt(7);
        let code = GuiPlatformInputTarget::code_language(7);
        let mut input = PlatformInputState::default();

        input.request_focus(prompt);
        input.begin_registration_frame(Some(code));

        assert_eq!(input.target, Some(code));
        assert_eq!(input.take_focus_request(), None);
    }

    #[test]
    fn auxiliary_focus_dismissal_supersedes_activation_and_is_consumed_once() {
        let prompt = GuiPlatformInputTarget::ai_prompt(7);
        let mut input = PlatformInputState::default();

        input.request_focus(prompt);
        input.request_focus_dismissal();

        assert_eq!(input.take_focus_request(), None);
        assert!(input.take_focus_dismissal_request());
        assert!(!input.take_focus_dismissal_request());
    }

    #[test]
    fn completed_document_activation_cancels_stale_auxiliary_dismissal() {
        let mut input = PlatformInputState::default();

        input.request_focus_dismissal();
        input.cancel_focus_dismissal();

        assert!(!input.take_focus_dismissal_request());
    }

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
        status.host_active = false;
        status.readonly_reason = Some(EditorReadonlyReason::NewerDocumentSchema {
            written_major: 3,
            supported_major: 2,
        });
        status.dirty = true;
        status.save_status = EditorSaveStatus::Failed("injected".to_owned());

        status.reset_for_session(false);

        assert!(!status.readonly);
        assert!(!status.requested_readonly);
        assert!(!status.host_active);
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
            fullscreen_video_block_id: Some(9),
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
        assert!(overlay.fullscreen_video_block_id.is_none());
        assert!(!overlay.fullscreen_video_requested_window);
        assert!(!overlay.fullscreen_video_observed_window);
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
    fn host_viewport_refresh_is_single_shot_until_the_new_bounds_are_rendered() {
        let mut interaction = InteractionUiState::default();
        let initial = gpui::Bounds::new(
            gpui::point(gpui::px(20.0), gpui::px(40.0)),
            gpui::size(gpui::px(900.0), gpui::px(700.0)),
        );
        let resized = gpui::Bounds::new(
            gpui::point(gpui::px(20.0), gpui::px(40.0)),
            gpui::size(gpui::px(620.0), gpui::px(700.0)),
        );

        assert!(interaction.request_editor_viewport_refresh(initial));
        assert!(!interaction.request_editor_viewport_refresh(initial));
        assert_eq!(
            interaction.take_pending_editor_viewport_bounds(),
            Some(initial)
        );

        interaction.note_editor_viewport_rendered(Some(initial));
        assert!(!interaction.request_editor_viewport_refresh(initial));
        assert!(interaction.request_editor_viewport_refresh(resized));
        assert!(!interaction.request_editor_viewport_refresh(resized));

        interaction.note_editor_viewport_rendered(Some(resized));
        assert!(!interaction.request_editor_viewport_refresh(resized));

        assert_eq!(interaction.take_pending_editor_viewport_bounds(), None);
    }

    #[test]
    fn prepared_host_viewport_is_consumed_before_the_resize_frame_is_rendered() {
        let mut interaction = InteractionUiState::default();
        let previous = gpui::Bounds::new(
            gpui::point(gpui::px(20.0), gpui::px(40.0)),
            gpui::size(gpui::px(620.0), gpui::px(700.0)),
        );
        let resized = gpui::Bounds::new(
            gpui::point(gpui::px(20.0), gpui::px(40.0)),
            gpui::size(gpui::px(900.0), gpui::px(700.0)),
        );

        interaction.note_editor_viewport_rendered(Some(previous));
        interaction.prepare_editor_viewport_for_render(resized);

        assert_eq!(
            interaction.take_pending_editor_viewport_bounds(),
            Some(resized)
        );
        interaction.note_editor_viewport_rendered(Some(resized));

        // The compatibility observer sees the same final bounds in prepaint,
        // so it must not schedule the correction frame that caused the flash.
        assert!(!interaction.request_editor_viewport_refresh(resized));
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
