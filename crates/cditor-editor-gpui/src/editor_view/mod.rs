use gpui::{AnyView, Context};

use crate::cache::RenderCacheState;

pub(crate) mod ai;
mod block_actions;
mod folding;
mod formatting;
mod lifecycle;
mod platform_input;
mod render;
mod slash_menu;
mod state;

pub use self::state::{CditorViewState, EditorReadonlyReason};
use self::state::{
    EditorDiagnosticsState, EditorSchedulingState, EditorStatusUiState, FeatureUiState,
    FocusUiState, InteractionUiState, PlatformInputState,
};
pub(crate) use self::state::{
    OverlayUiState, PlatformCharacterCoordinatesIdentity, PlatformImeCandidateBounds,
};
pub(crate) use crate::app::persistence_bridge::save_status_for_mode;
pub(crate) use crate::interaction::table_scroll::TableScrollSnapshot;
pub(crate) use block_actions::block_focus_offset_after_missed_hit_test;
pub(crate) use formatting::{
    SelectionToolbarDelay, floating_toolbar_passes_selection_delay, formatting_toolbar_context,
    formatting_toolbar_state,
};
pub(crate) use platform_input::GuiPlatformInputTarget;
#[cfg(test)]
pub(crate) use platform_input::platform_input_registration_allows;

pub struct CditorV2View {
    pub(crate) state: CditorViewState,
    pub(crate) focus: FocusUiState,
    pub(crate) input: PlatformInputState,
    pub(crate) features: FeatureUiState,
    pub(crate) overlay: OverlayUiState,
    pub(crate) diagnostics: EditorDiagnosticsState,
    pub(crate) status: EditorStatusUiState,
    pub(crate) interaction: InteractionUiState,
    pub(crate) cache: RenderCacheState,
    pub(crate) scheduling: EditorSchedulingState,
    pub(crate) page_chrome_extras: Option<AnyView>,
    pub(crate) embedded_composer: bool,
}

impl CditorV2View {
    /// Returns the focus handle tracked by the editor root element.
    ///
    /// Hosts (for example a dock shell) can return this handle from their
    /// panel's `Focusable::focus_handle` so that activating the panel focuses
    /// the actual editor surface. Without this, keybindings such as Enter are
    /// dispatched through the shell's inert focus handle and never reach the
    /// editor.
    pub fn editor_focus_handle(&self) -> gpui::FocusHandle {
        self.focus.editor.clone()
    }

    /// Embeds a host-owned view in the page chrome action row.
    ///
    /// The extra view is rendered on the same line as the page icon and cover
    /// actions so hosts can surface decorations such as document tags.
    pub fn set_page_chrome_extra(&mut self, view: AnyView, cx: &mut Context<Self>) {
        self.page_chrome_extras = Some(view);
        cx.notify();
    }

    /// Removes the host-owned page chrome view, if any.
    pub fn clear_page_chrome_extra(&mut self, cx: &mut Context<Self>) {
        if self.page_chrome_extras.take().is_some() {
            cx.notify();
        }
    }

    pub fn set_embedded_composer(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.embedded_composer != enabled {
            self.embedded_composer = enabled;
            cx.notify();
        }
    }
}

#[cfg(test)]
#[path = "cditor_v2_view_tests.rs"]
mod cditor_v2_view_tests;

#[cfg(test)]
#[path = "cditor_v2_view_interaction_tests.rs"]
mod cditor_v2_view_interaction_tests;
