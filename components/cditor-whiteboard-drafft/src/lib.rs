//! Isolated Drafft Ink integration for a native GPUI whiteboard.
//!
//! This crate owns the AGPL dependency boundary and the GPUI host bridge. It
//! deliberately does not depend on the existing `cditor-whiteboard` crate and
//! directly reuses Drafft Ink's public model and renderer APIs.

#[cfg(feature = "drafft-core")]
pub use drafftink_core as core;

#[cfg(feature = "drafft-vello")]
pub use drafftink_render as render;

#[cfg(feature = "drafft-core")]
mod font;
#[cfg(feature = "drafft-core")]
mod model_host;
#[cfg(feature = "drafft-core")]
mod paint;
#[cfg(feature = "drafft-core")]
mod view;

#[cfg(feature = "drafft-core")]
pub use font::{CANVAS_FONT_FAMILY, UI_FONT_FAMILY, bundled_fonts, cjk_fallback_fonts};
#[cfg(feature = "drafft-core")]
pub use model_host::document::{parse_document, parse_document_json, parse_library};
#[cfg(feature = "drafft-core")]
pub use model_host::{DrafftBoard, PointerOutcome};
#[cfg(feature = "drafft-core")]
pub use view::{
    DrafftBoardView, DrafftChromeMode, FocusRequestFn, SceneChangeFn, bind_drafft_keys,
};
