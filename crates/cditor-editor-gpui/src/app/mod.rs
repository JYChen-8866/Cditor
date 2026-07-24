pub mod cditor_v2_view;

mod command_router;
mod frame_telemetry;
mod lifecycle;
mod payload_cache;
mod persistence_bridge;
pub(crate) mod platform_layout_cache;
mod render;
mod sdk;
mod state;
mod text_hit;

pub use cditor_v2_view::CditorV2View;
pub(crate) use cditor_v2_view::GuiPlatformInputTarget;
pub(crate) use state::EditorReadonlyReason;
