pub mod cditor_v2_view;

mod input;
mod input_trace;
mod interaction;
mod lifecycle;
mod persistence_bridge;
mod render;
mod state;
mod text_hit;

pub use cditor_v2_view::CditorV2View;
pub(crate) use cditor_v2_view::GuiPlatformInputTarget;
