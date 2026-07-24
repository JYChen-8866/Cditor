use cditor_core::ids::{BlockId, SurfaceId};

use crate::editor_view::CditorV2View;
use crate::surfaces::table_cell::TableCellLayoutKey;
use crate::text::RichTextPlatformLayout;

fn table_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_TABLE")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

fn trace_table(event: &str, details: impl std::fmt::Display) {
    if table_trace_enabled() {
        eprintln!("[cditor][table][gui][{event}] {details}");
    }
}

pub(crate) fn queue_rendered_media_height(
    view: &CditorV2View,
    block_id: BlockId,
    content_version: u64,
    measured_height: f64,
) -> bool {
    view.ready_session()
        .and_then(|session| {
            session
                .queue_measured_block_height(block_id, content_version, measured_height)
                .ok()
        })
        .unwrap_or(false)
}

pub(crate) fn accept_text_layout(view: &mut CditorV2View, layout: RichTextPlatformLayout) -> bool {
    let pinned_surface = view
        .input
        .layout_identity
        .map(|identity| identity.surface_id);
    if let Some(position) = layout.table_cell_position {
        trace_table(
            "cache.table_cell",
            format_args!(
                "block={} row={} col={} content_version={} bounds=({}, {}, {}, {}) text_len={} lines={} accessibility={}",
                layout.block_id,
                position.row,
                position.col,
                layout.content_version,
                f32::from(layout.bounds.left()),
                f32::from(layout.bounds.top()),
                f32::from(layout.bounds.size.width),
                f32::from(layout.bounds.size.height),
                layout.snapshot.text().len(),
                layout.snapshot.line_count(),
                layout.accessibility.is_some()
            ),
        );
        view.cache.table_cell_layouts.insert(
            TableCellLayoutKey {
                block_id: layout.block_id,
                row: position.row,
                col: position.col,
            },
            layout,
            pinned_surface,
        );
        return false;
    }
    if !matches!(layout.surface_id, SurfaceId::Block(_)) {
        view.cache
            .text_surface_layouts
            .insert(layout.surface_id, layout, pinned_surface);
        return false;
    }
    let block_id = layout.block_id;
    let content_version = layout.content_version;
    let measured_height = layout.measured_height;
    view.cache
        .text_layouts
        .insert(block_id, layout, pinned_surface);
    if view.ready_session().is_some_and(|session| {
        session
            .text_block_context(block_id)
            .ok()
            .flatten()
            .is_some_and(|context| context.kind == cditor_core::rich_text::RichBlockKind::Mermaid)
    }) {
        // Mermaid owns a stable preview/source box and reports its rendered
        // media height separately. Source text shaping must not overwrite it.
        return false;
    }
    queue_rendered_media_height(view, block_id, content_version, measured_height)
}
