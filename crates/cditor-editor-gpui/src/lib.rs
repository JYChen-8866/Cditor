pub(crate) mod app;
mod block;
pub(crate) mod cache;
mod clipboard_assets;
mod component_sdk;
mod diagnostics;
mod document;
mod editor_view;
pub(crate) mod features;
mod image_loader;
mod image_preview;
mod input;
pub(crate) mod interaction;
mod menu_metrics;
pub(crate) mod overlays;
mod persistence;
mod platform;
pub(crate) mod presentation;
mod scroll;
mod skeleton;
pub(crate) mod surfaces;
mod text;
pub mod theme;

pub use component_sdk::{CditorComponent, CditorHandle, CditorViewContract, CditorViewFactory};
pub use editor_view::{CditorV2View, CditorViewState, EditorReadonlyReason};
pub use image_loader::{RemoteImageDataSource, configure_remote_image_data_source};
pub use input::bind_cditor_keys;
pub use persistence::{EditorLoadStateLabel, EditorSaveStatus};

#[cfg(test)]
pub(crate) mod test_support;
pub use text::CaretBlink;
