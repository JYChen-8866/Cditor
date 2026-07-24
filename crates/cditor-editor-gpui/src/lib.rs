pub(crate) mod app;
pub mod block;
pub(crate) mod cache;
pub mod clipboard_assets;
pub mod diagnostics;
pub mod document;
pub mod editor_view;
pub(crate) mod features;
pub mod image_loader;
pub mod image_preview;
pub mod input;
pub(crate) mod interaction;
pub mod menu_metrics;
pub(crate) mod overlays;
pub mod persistence;
pub mod platform;
pub(crate) mod presentation;
pub mod scroll;
pub mod skeleton;
pub(crate) mod surfaces;
pub mod text;
pub mod theme;

pub use editor_view::CditorV2View;

#[cfg(test)]
pub(crate) mod test_support;
