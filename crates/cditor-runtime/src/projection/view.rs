//! Runtime adapters for the stable editor projection protocol.
//!
//! Scroll, render-window, and diagnostics remain viewport algorithm outputs
//! during Phase 2. Document-facing block, table, and AI read models are owned
//! by `cditor-editor-protocol`.

use cditor_editor_protocol::projection::EditorProjection;
use cditor_viewport::{
    debug_overlay::DebugOverlaySnapshot, scroll::VirtualScrollState, window::RenderWindow,
};

pub type EditorViewProjection =
    EditorProjection<VirtualScrollState, RenderWindow, DebugOverlaySnapshot>;
pub type PayloadWindowFailureView =
    cditor_editor_protocol::projection::PayloadWindowFailureProjection;
pub type AiPreviewKind = cditor_editor_protocol::projection::AiPreviewKind;
pub type AiPreviewStatus = cditor_editor_protocol::projection::AiPreviewStatus;
pub type AiPreviewSnapshot = cditor_editor_protocol::projection::AiPreviewProjection;
pub type TableCellPosition = cditor_editor_protocol::projection::TableCellPosition;
pub type TableCellSpansSnapshot = cditor_editor_protocol::projection::TableCellSpansSnapshot;
pub type TablePayloadSnapshot = cditor_editor_protocol::projection::TablePayloadSnapshot;
pub type TableViewState = cditor_editor_protocol::projection::TableProjection;
pub type TableVisibleCell = cditor_editor_protocol::projection::TableCellProjection;
pub type ViewBlockSnapshot = cditor_editor_protocol::projection::BlockProjection;
