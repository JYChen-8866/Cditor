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
mod memory_pressure;
mod menu_metrics;
pub(crate) mod overlays;
mod persistence;
mod platform;
pub(crate) mod presentation;
mod provider_io;
mod scroll;
mod skeleton;
pub(crate) mod surfaces;
mod text;
pub mod theme;

pub use component_sdk::{CditorComponent, CditorHandle, CditorViewContract, CditorViewFactory};
pub use document::page_chrome::custom_page_icon_asset;
pub use editor_view::{CditorHostElement, CditorV2View, CditorViewState, EditorReadonlyReason};
pub use image_loader::{RemoteImageDataSource, configure_remote_image_data_source};
pub use input::{bind_cditor_keys, cditor_key_bindings};
pub use memory_pressure::{
    CditorMemoryPressure, CditorMemoryTrimReport, CditorViewMemoryTrimReport,
};
pub use persistence::{EditorLoadStateLabel, EditorSaveStatus};

#[cfg(test)]
pub(crate) mod test_support;
pub use text::CaretBlink;

/// Trim process-wide editor caches at an explicit background or memory
/// pressure boundary. Only completed/reconstructible image and exact-raster
/// resources are reclaimed; in-flight work is invalidated only for a critical
/// pressure event, and the document/editor model is never touched.
///
/// The returned report is useful for boundary diagnostics. GPU retirement is
/// deferred until the current GPUI effect completes so the scene being
/// replaced cannot sample a tile after it has been released.
pub fn trim_process_reconstructible_caches(
    pressure: CditorMemoryPressure,
    cx: &mut gpui::App,
) -> CditorMemoryTrimReport {
    let images = image_loader::trim_image_cache(pressure);
    let raster = text::trim_exact_raster_cache(pressure);
    let mut retired = images.retired_images;
    for image in raster.retired_images {
        if !retired
            .iter()
            .any(|candidate| std::sync::Arc::ptr_eq(candidate, &image))
        {
            retired.push(image);
        }
    }
    let retired_count = retired.len();
    if !retired.is_empty() {
        cx.defer(move |cx| {
            for image in retired {
                cx.drop_image(image, None);
            }
        });
    }
    CditorMemoryTrimReport {
        image_entries_evicted: images.evicted_entries,
        image_bytes_evicted: images.evicted_decoded_bytes,
        exact_raster_entries_evicted: raster.evicted_entries,
        exact_raster_bytes_evicted: raster.evicted_estimated_bytes,
        invalidated_image_loads: images.invalidated_loads,
        retired_images: retired_count,
    }
}
