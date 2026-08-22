mod ai;
mod ai_session_port;
mod ai_utils;
mod block_attrs;
mod capabilities;
mod clipboard;
mod clipboard_blocks;
mod cold_start;
mod columns;
mod command;
mod command_errors;
mod command_query;
mod command_query_typed;
mod command_selection;
mod composition;
mod constructors;
mod document_state;
mod domain_state;
mod editing_state;
mod focus;
mod focus_transition;
mod folding;
mod format_transaction;
mod history_state;
mod import_plan;
mod inline_color;
mod inline_format;
mod inline_link;
mod inline_link_selection;
mod layout_heights;
mod layout_state;
mod local_transaction;
mod markdown_paste;
mod markdown_transaction;
mod media;
mod page_local_layout;
mod payload_cache;
mod payload_hydration;
mod payload_window;
mod platform_text_edit;
mod projection;
mod queries;
mod realtime;
mod scroll;
mod selection;
mod selection_blocks;
mod selection_materialization;
mod selection_state;
mod selection_transaction;
mod selection_unified;
mod slash_command;
mod split_height;
mod state;
mod structure_delete;
mod structure_edit;
mod structure_index;
mod structure_insert;
mod structure_move;
mod structure_payload;
mod table;
mod text_edit;
mod text_navigation;
mod text_payload;
mod text_surface;
mod text_target;
mod transaction_apply;
mod transaction_apply_domain;
mod transaction_apply_domain_validation;
mod transaction_apply_payload;
mod transaction_apply_structure;
mod transaction_apply_structure_text;
mod transaction_state;
mod typing_marks;
mod undo_redo;
mod whiteboard;

pub use ai::{
    AgentBlockOutline, AiApplyMode, AiRequestDispatch, AiRequestPresentation, AiSessionSnapshot,
    AiSessionStatus, AiStreamApplyResult, RuntimeAiTarget,
};
pub use ai_session_port::{AiSessionOutcome, AiSessionRequest};
pub use cditor_viewport::window::WindowMemoryPressure;
pub use cold_start::{
    DocumentRuntimeColdStartData, DocumentRuntimeColdStartReport, DocumentRuntimeIndexSource,
};
pub use focus_transition::CompositionFocusTransition;
pub use import_plan::ImportApplicationReport;
pub use realtime::{RealtimeInput, RealtimeInputError, RealtimeInputOutcome, RealtimeInputRequest};
pub use selection::DocumentTextSelectionFragment;
pub use selection_materialization::{
    SelectionMaterializationApplyDecision, SelectionMaterializationRequest,
};
pub use text_surface::{
    RichTextDelta, TextSurface, TextSurfaceCapabilities, TextSurfaceEditResult,
    TextSurfaceRegistry, TextSurfaceRole, TextSurfaceSnapshot, TextSurfaceSnapshotIdentity,
};
pub use transaction_apply::{AppliedTransaction, TransactionApplyError};

use self::{
    editing_state::TypingMarkOverride,
    history_state::{
        HistoryState, RuntimeUndoEvent, TextSnapshot, TypingUndoGroup, TypingUndoRequest,
        UndoScrollSnapshot,
    },
    layout_state::{
        LayoutState, PendingMeasuredHeight, ProjectionState, ProjectionWindowDecision,
        ProjectionWindowTarget, StableProjectionSnapshot,
    },
    selection::FocusedTextSelection,
    selection_state::{FocusedTableCell, VisualCaretPosition},
    structure_insert::EnterSplitMode,
    table::TableRuntime,
    transaction_state::TransactionState,
};

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ops::Range,
    sync::{Arc, OnceLock},
    time::Duration,
};
use web_time::Instant;

use super::{
    AiPreviewKind, AiPreviewSnapshot, AiPreviewStatus, EditorViewProjection, TableCellPosition,
    TableCellSpansSnapshot, TablePayloadSnapshot, TableViewState, TableVisibleCell,
    ViewBlockSnapshot,
};
use crate::content::payload_preparation::{
    PreparedPayloadRecord, normalize_payload_record_for_kind,
};
use crate::content::payload_window::{
    PayloadLoadPriority, PayloadWindowApplyDecision, PayloadWindowLoadRequest,
    PayloadWindowLoadResult,
};
use crate::{
    CompositionBaseSelection, CompositionState, EditingSession, InputSessionIdentity, InputTarget,
    ListProjectionCache, PayloadWindow, PieceTableTextModel, SingleCharInputHotPath,
};
use cditor_core::clipboard::{
    ClipboardBlock, ClipboardBlockFragment, ClipboardFragmentBoundary, ClipboardSelection,
};
use cditor_core::document::{BlockIndexRecord, DocumentIndex, VisibleDocumentIndex};
use cditor_core::edit::{
    AssetSnapshot, CollectionRecordSnapshot, CommentThreadSnapshot, DocumentSelection,
    EditOperation, EditTransaction, EditTransactionKind, InnerSelectionAnchor, InternalTextOffset,
    NormalizedSelection, ScrollAnchor, SelectionRange, TableEditOperation, TextAffinity,
    TextEditOperation, TextOffsetMap, TextPosition, TransactionPermission,
    TransactionPermissionSet, TransactionPrecondition, UndoStack,
};
use cditor_core::ids::{AssetId, BlockId, CollectionId, CommentThreadId, DocumentId, SurfaceId};
use cditor_core::import_plan::ImportedBlockDocument;
use cditor_core::layout::{
    BlockHeightIndex, BlockLayoutMeta, HeightConfidence, HeightEstimate,
    IMAGE_BLOCK_ESTIMATED_HEIGHT_PX, PAGE_POLICY_VERSION, PageLayoutIdentity, PageLayoutIndex,
    PageLocalHeightIndex, PagePolicy, estimate_block_height, estimate_text_payload_height,
    layout_width_for_kind, text_line_height_for_kind,
};
use cditor_core::rich_text::TableCellAlign;
use cditor_core::rich_text::{
    AssetRef, BlockAttrs, BlockPayload, BlockPayloadRecord, BlockPayloadView, CoverPositionY,
    DocumentMetadata, ImagePayload, InlineColorTarget, InlineMark, InlineSpan, PageCover, PageIcon,
    RichBlockKind, RichBlockRecord, RichTextDocument, TableCellMerge, TableRange, TableTrackSize,
    VideoPayload, block_kind_shortcut_with_marker_len, code_fence_shortcut,
    kind_tag_for_rich_block_kind, markdown_inline_shortcut_spans, parse_callout_marker,
    plain_text_from_spans, rich_block_kind_from_tag,
};
use cditor_viewport::debug_overlay::DebugOverlaySnapshot;
use cditor_viewport::scroll::{
    CaretAnchor, HeightCorrectionPriority, PendingHeightCorrection, ScrollAccumulator,
    ScrollOrigin, ScrollbarDragEnd, ScrollbarDragSession, ScrollbarDragUpdate, ScrollbarPolicy,
    ScrollbarVisualState, VirtualScrollState,
};
use cditor_viewport::window::{
    PlaceholderWindow, RenderWindow, ScrollDirection, WindowPlanDecision, WindowPlanRequest,
    WindowPlanner, WindowPlannerPolicy,
};
fn input_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_INPUT")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

fn trace_input(event: &str, details: impl std::fmt::Display) {
    if input_trace_enabled() {
        crate::diagnostics::write_stderr(format_args!(
            "[cditor][input][runtime][{event}] {details}"
        ));
    }
}

fn image_resize_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_IMAGE_RESIZE")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

fn trace_image_resize(event: &str, details: impl std::fmt::Display) {
    if image_resize_trace_enabled() {
        crate::diagnostics::write_stderr(format_args!(
            "[cditor][image-resize][runtime][{event}] {details}"
        ));
    }
}

fn flash_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_FLASH")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

fn trace_flash(event: &str, details: impl std::fmt::Display) {
    if flash_trace_enabled() {
        crate::diagnostics::write_stderr(format_args!(
            "[cditor][flash][runtime][{event}] {details}"
        ));
    }
}

fn table_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_TABLE")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

fn trace_table(event: &str, details: impl std::fmt::Display) {
    if table_trace_enabled() {
        crate::diagnostics::write_stderr(format_args!(
            "[cditor][table][runtime][{event}] {details}"
        ));
    }
}

fn block_color_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_BLOCK_COLOR")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

fn trace_block_color(event: &str, details: impl std::fmt::Display) {
    if block_color_trace_enabled() {
        crate::diagnostics::write_stderr(format_args!(
            "[cditor][block-color][runtime][{event}] {details}"
        ));
    }
}

pub use selection::RichTextSelectionSnapshot;
pub use state::{DocumentRuntime, GlobalScrollTarget};
pub use table::TableClipboardSnapshot;

use inline_color::set_color_mark_for_range;
use inline_link::{replace_range_with_linked_text, set_link_mark_for_range};
use local_transaction::{LocalBlockOperationsTransaction, LocalInsertBlocksTransaction};
use table::{default_table_payload, ensure_table_payload_for_kind};
use text_payload::{
    append_plain_text_to_payload, backspace_at_start_resets_kind_to_paragraph, merge_inline_spans,
    newline_sibling_kind_for_v1, next_grapheme_boundary, payload_for_kind_from_plain_text,
    prepend_plain_text_to_payload, previous_char_boundary, previous_grapheme_boundary,
    replace_rich_text_spans_preserving_marks, replace_rich_text_spans_with_spans, safe_char_range,
    slice_rich_text_spans, split_payload_for_enter, sync_payload_from_model_after_replace,
    text_payload_for_existing_after_replace, toggle_mark_for_range, uses_soft_tab,
};
use text_target::{FocusedTextEdit, normalized_grapheme_offset, normalized_grapheme_range};
use whiteboard::default_whiteboard_payload;

fn push_unique(block_ids: &mut Vec<BlockId>, block_id: BlockId) {
    if !block_ids.contains(&block_id) {
        block_ids.push(block_id);
    }
}

fn editable_text_for_payload(payload: &BlockPayload) -> Option<String> {
    match payload {
        BlockPayload::RichText { spans } => {
            Some(cditor_core::rich_text::plain_text_from_spans(spans))
        }
        BlockPayload::Code { text, .. } => Some(text.clone()),
        BlockPayload::Html { html, .. } => Some(html.clone()),
        _ => None,
    }
}

fn editable_text_len_for_payload(payload: &BlockPayload) -> Option<usize> {
    match payload {
        BlockPayload::RichText { spans } => Some(spans.iter().map(|span| span.text.len()).sum()),
        BlockPayload::Code { text, .. } => Some(text.len()),
        BlockPayload::Html { html, .. } => Some(html.len()),
        _ => None,
    }
}

fn sync_text_model_for_payload(
    text_models: &mut HashMap<BlockId, PieceTableTextModel>,
    payload: &BlockPayloadRecord,
) {
    if let Some(text) = editable_text_for_payload(&payload.payload) {
        text_models.insert(payload.block_id, PieceTableTextModel::new(text));
    } else {
        text_models.remove(&payload.block_id);
    }
}

fn large_demo_page_policy() -> PagePolicy {
    PagePolicy {
        max_blocks: 128,
        target_height: 3_000.0,
        max_estimated_cost: 512,
        max_text_bytes: 32 * 1024,
        max_inline_runs: 2_000,
        max_complex_blocks: 8,
    }
}

fn log_runtime_timing(label: &str, start: Instant, count: Option<usize>) {
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    if elapsed_ms >= 1.0 {
        if let Some(count) = count {
            crate::diagnostics::write_stderr(format_args!(
                "[cditor][timing] {label} count={count} elapsed_ms={elapsed_ms:.2}"
            ));
        } else {
            crate::diagnostics::write_stderr(format_args!(
                "[cditor][timing] {label} elapsed_ms={elapsed_ms:.2}"
            ));
        }
    }
}

fn estimate_text_block_height_for_text(kind: &RichBlockKind, text: &str) -> f64 {
    estimate_text_payload_height(kind, text, layout_width_for_kind(kind)).height
}

fn estimate_payload_height(payload: &BlockPayloadRecord, _index: usize) -> f64 {
    match (&payload.kind, &payload.payload) {
        (RichBlockKind::Table, BlockPayload::Table(table)) => {
            f64::from(table::table_payload_projected_height_px(table))
        }
        _ => {
            estimate_block_height(
                &payload.kind,
                &payload.payload,
                layout_width_for_kind(&payload.kind),
            )
            .height
        }
    }
}

#[cfg(test)]
mod tests;
