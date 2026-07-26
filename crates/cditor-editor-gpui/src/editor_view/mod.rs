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

pub(crate) use self::state::OverlayUiState;
pub use self::state::{CditorViewState, EditorReadonlyReason};
use self::state::{
    EditorDiagnosticsState, EditorSchedulingState, EditorStatusUiState, FeatureUiState,
    FocusUiState, InteractionUiState, PlatformInputState,
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
}

#[cfg(test)]
#[path = "cditor_v2_view_tests.rs"]
mod cditor_v2_view_tests;

#[cfg(test)]
#[path = "cditor_v2_view_interaction_tests.rs"]
mod cditor_v2_view_interaction_tests;
