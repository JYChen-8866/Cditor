use std::sync::Arc;

use cditor_session::EditorSessionHandle;
use gpui::{Context, FocusHandle};

use cditor_core::block::GutterBlockDragState;
use cditor_core::ids::BlockId;

use crate::app::platform_layout_cache::PlatformLayoutCache;
use crate::block::code::highlight::DEFAULT_CODE_HIGHLIGHT_THEME;
use crate::block::{CodeHighlightCache, MermaidRenderCache, WhiteboardThumbnailCache};
use crate::input::BlockDragSelectionController;
use crate::input::{AiPromptState, CodeLanguageEditState};
use crate::interaction::geometry::ProjectedBlockRect;
use crate::interaction::image_resize::GuiImageResizeDrag;
use crate::interaction::scrollbar::GuiScrollbarDrag;
use crate::interaction::selection_drag::GuiTextDragSelection;
use crate::interaction::table_mode::GuiTableInteractionMode;
use crate::interaction::table_reorder::GuiTableReorderDrag;
use crate::interaction::table_resize::GuiTableResizeDrag;
use crate::interaction::table_scroll::{GuiTableHScrollDrag, GuiTableScrollState};
use crate::overlay::{GuiToast, SlashMenuState, WhiteboardEditorSession};
use crate::persistence::EditorSaveStatus;
use crate::scroll::ScrollAccumulator;
use crate::surfaces::table_cell::TableCellLayoutKey;
use crate::text::TextPlatformLayoutIdentity;

use super::{CditorV2View, GuiPlatformInputTarget, SelectionToolbarDelay, ai::default_ai_provider};

pub(crate) struct RenderCacheState {
    pub(crate) text_layouts: PlatformLayoutCache<BlockId>,
    pub(crate) table_cell_layouts: PlatformLayoutCache<TableCellLayoutKey>,
    pub(crate) text_surface_layouts: PlatformLayoutCache<cditor_core::ids::SurfaceId>,
    pub(crate) code_highlights: CodeHighlightCache,
    pub(crate) mermaid_renders: MermaidRenderCache,
    pub(crate) mermaid_source_blocks: std::collections::HashSet<BlockId>,
    pub(crate) whiteboard_thumbnails: WhiteboardThumbnailCache,
}

impl Default for RenderCacheState {
    fn default() -> Self {
        Self {
            text_layouts: crate::app::platform_layout_cache::block_layout_cache(),
            table_cell_layouts: crate::app::platform_layout_cache::table_layout_cache(),
            text_surface_layouts: crate::app::platform_layout_cache::auxiliary_layout_cache(),
            code_highlights: Default::default(),
            mermaid_renders: Default::default(),
            mermaid_source_blocks: Default::default(),
            whiteboard_thumbnails: Default::default(),
        }
    }
}

impl RenderCacheState {
    pub(crate) fn reset_session(&mut self) {
        self.text_layouts.clear();
        self.table_cell_layouts.clear();
        self.text_surface_layouts.clear();
        self.code_highlights.clear();
        self.mermaid_renders.clear();
        self.mermaid_source_blocks.clear();
        self.whiteboard_thumbnails.clear();
    }
}

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
    pub(crate) ai_enabled: bool,
    pub(crate) code_highlight_theme: &'static str,
    pub(crate) whiteboard_editor: Option<WhiteboardEditorSession>,
}

impl Default for FeatureUiState {
    fn default() -> Self {
        Self {
            ai_provider: default_ai_provider(),
            ai_enabled: true,
            code_highlight_theme: DEFAULT_CODE_HIGHLIGHT_THEME,
            whiteboard_editor: None,
        }
    }
}

impl FeatureUiState {
    pub(crate) fn reset_session(&mut self) {
        self.whiteboard_editor = None;
    }
}

#[derive(Default)]
pub(crate) struct OverlayUiState {
    pub(crate) ai_prompt: Option<AiPromptState>,
    pub(crate) ai_preview_scroll_handle: gpui::ScrollHandle,
    pub(crate) code_language_edit: Option<CodeLanguageEditState>,
    pub(crate) code_theme_menu_block_id: Option<BlockId>,
    pub(crate) slash_menu: Option<SlashMenuState>,
    pub(crate) toast: Option<GuiToast>,
    pub(crate) table_menu_ui: crate::block::table::menu::TableMenuUiState,
    pub(crate) gutter_toolbar_block_id: Option<BlockId>,
    pub(crate) selection_toolbar_delay: SelectionToolbarDelay,
    pub(crate) block_transform_menu_open: bool,
    pub(crate) color_menu_open: bool,
    pub(crate) color_menu_hover_generation: u64,
    pub(crate) color_menu_scroll_handle: gpui::ScrollHandle,
    pub(crate) last_color_action: Option<crate::overlay::ColorMenuAction>,
}

impl OverlayUiState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

pub(crate) struct FocusUiState {
    pub(crate) editor: FocusHandle,
    pub(crate) code_language: FocusHandle,
    pub(crate) ai_prompt: FocusHandle,
    pub(crate) sdk_observers_registered: bool,
    pub(crate) last_emitted_selection: Option<cditor_api::document::DocumentSelection>,
}

impl FocusUiState {
    pub(crate) fn new(cx: &mut Context<CditorV2View>) -> Self {
        Self {
            editor: cx.focus_handle(),
            code_language: cx.focus_handle(),
            ai_prompt: cx.focus_handle(),
            sdk_observers_registered: false,
            last_emitted_selection: None,
        }
    }

    pub(crate) fn reset_session_projection(&mut self) {
        self.last_emitted_selection = None;
    }
}

#[derive(Default)]
pub(crate) struct PlatformInputState {
    pub(crate) target: Option<GuiPlatformInputTarget>,
    pub(crate) session_identity: Option<cditor_runtime::InputSessionIdentity>,
    pub(crate) layout_identity: Option<TextPlatformLayoutIdentity>,
    pub(crate) preferred_navigation_x: Option<(cditor_core::ids::SurfaceId, f32)>,
}

impl PlatformInputState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

pub(crate) struct InteractionUiState {
    pub(crate) last_wheel_delta_y: f64,
    pub(crate) scroll_accumulator: ScrollAccumulator,
    pub(crate) editor_viewport_handle: gpui::ScrollHandle,
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
    pub(crate) table_hscroll_drag: Option<GuiTableHScrollDrag>,
    pub(crate) projected_block_rects: Vec<ProjectedBlockRect>,
}

impl Default for InteractionUiState {
    fn default() -> Self {
        Self {
            last_wheel_delta_y: 0.0,
            scroll_accumulator: Default::default(),
            editor_viewport_handle: Default::default(),
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
            table_hscroll_drag: None,
            projected_block_rects: Vec::new(),
        }
    }
}

impl InteractionUiState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
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
        assert_eq!(status.save_status, EditorSaveStatus::Clean);
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
        assert_eq!(status.save_status, EditorSaveStatus::Clean);
    }

    #[test]
    fn platform_input_reset_discards_session_bound_navigation_state() {
        let mut input = PlatformInputState {
            preferred_navigation_x: Some((cditor_core::ids::SurfaceId::Block(7), 42.0)),
            ..Default::default()
        };

        input.reset();

        assert!(input.target.is_none());
        assert!(input.session_identity.is_none());
        assert!(input.layout_identity.is_none());
        assert!(input.preferred_navigation_x.is_none());
    }

    #[test]
    fn interaction_reset_discards_drag_scroll_and_hit_test_state() {
        let mut interaction = InteractionUiState {
            last_wheel_delta_y: 18.0,
            text_drag_auto_scroll_scheduled: true,
            gutter_drag_auto_scroll_scheduled: true,
            hovered_block_id: Some(7),
            action_block_id: Some(8),
            table_interaction_mode: GuiTableInteractionMode::EditingCell {
                block_id: 9,
                row: 1,
                col: 2,
            },
            ..Default::default()
        };

        interaction.reset();

        assert_eq!(interaction.last_wheel_delta_y, 0.0);
        assert!(!interaction.text_drag_auto_scroll_scheduled);
        assert!(!interaction.gutter_drag_auto_scroll_scheduled);
        assert!(interaction.hovered_block_id.is_none());
        assert!(interaction.action_block_id.is_none());
        assert!(matches!(
            interaction.table_interaction_mode,
            GuiTableInteractionMode::Idle
        ));
        assert!(interaction.projected_block_rects.is_empty());
    }

    #[test]
    fn overlay_reset_discards_document_bound_transient_state() {
        let mut overlay = OverlayUiState {
            code_theme_menu_block_id: Some(7),
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
        assert!(overlay.gutter_toolbar_block_id.is_none());
        assert!(!overlay.block_transform_menu_open);
        assert!(!overlay.color_menu_open);
        assert_eq!(overlay.color_menu_hover_generation, 0);
        assert!(overlay.table_menu_ui.query.is_empty());
        assert_eq!(overlay.table_menu_ui.caret_offset, 0);
        assert!(overlay.table_menu_ui.marked_range.is_none());
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
        assert!(features.whiteboard_editor.is_none());
    }

    #[test]
    fn render_cache_reset_discards_document_bound_cache_state() {
        let mut cache = RenderCacheState::default();
        cache.mermaid_source_blocks.insert(17);

        cache.reset_session();

        assert!(cache.text_layouts.is_empty());
        assert!(cache.table_cell_layouts.is_empty());
        assert!(cache.text_surface_layouts.is_empty());
        assert_eq!(cache.text_layouts.estimated_bytes(), 0);
        assert_eq!(cache.table_cell_layouts.estimated_bytes(), 0);
        assert_eq!(cache.text_surface_layouts.estimated_bytes(), 0);
        assert!(cache.mermaid_source_blocks.is_empty());
    }

    #[test]
    fn diagnostics_state_preserves_the_host_debug_choice() {
        assert!(EditorDiagnosticsState::new(true).show_debug);
        assert!(!EditorDiagnosticsState::new(false).show_debug);
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
    Loading { message: String },
    LoadFailed { message: String },
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

    pub fn apply_load_failed(&mut self, message: impl Into<String>) {
        *self = Self::LoadFailed {
            message: message.into(),
        };
    }
}
