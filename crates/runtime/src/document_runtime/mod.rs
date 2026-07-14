mod ai;
mod block_attrs;
mod capabilities;
mod clipboard;
mod cold_start;
mod composition;
mod constructors;
mod focus;
mod folding;
mod inline_color;
mod inline_format;
mod layout_heights;
mod markdown_paste;
mod media;
mod payload_hydration;
mod payload_window;
mod projection;
mod scroll;
mod selection;
mod state;
mod structure_delete;
mod structure_edit;
mod structure_index;
mod structure_move;
mod structure_payload;
mod structure_transactions;
mod table;
mod text_edit;
mod text_navigation;
mod text_payload;
mod text_target;
mod undo_redo;
mod whiteboard;

pub use ai::{
    AiApplyMode, AiRequestDispatch, AiRequestPresentation, AiSessionSnapshot, AiSessionStatus,
    AiStreamApplyResult, RuntimeAiTarget,
};
pub use cold_start::{
    DocumentRuntimeColdStartData, DocumentRuntimeColdStartReport, DocumentRuntimeIndexSource,
};
pub use selection::DocumentTextSelectionFragment;

use self::{selection::FocusedTextSelection, table::TableRuntime};

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ops::Range,
    sync::OnceLock,
    time::Instant,
};

use super::{
    AiPreviewKind, AiPreviewSnapshot, AiPreviewStatus, EditorViewProjection, TableCellPosition,
    TableViewState, TableVisibleCell, ViewBlockSnapshot,
};
use crate::content::payload_window::{
    PayloadWindowApplyDecision, PayloadWindowLoadRequest, PayloadWindowLoadResult,
};
use crate::{
    CompositionState, EditingSession, InputTarget, ListProjectionCache, PayloadWindow,
    PieceTableTextModel, SingleCharInputHotPath,
};
use cditor_core::document::{BlockIndexRecord, DocumentIndex, VisibleDocumentIndex};
use cditor_core::edit::{
    DocumentSelection, EditOperation, EditTransaction, EditTransactionKind, InternalTextOffset,
    SelectionRange, TextOffsetMap, TextPosition,
};
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::layout::{
    BlockHeightIndex, BlockLayoutMeta, DEFAULT_LAYOUT_WIDTH_PX, HeightConfidence, HeightEstimate,
    IMAGE_BLOCK_ESTIMATED_HEIGHT_PX, PageLayoutIndex, PagePolicy, estimate_block_height,
    estimate_text_payload_height, text_line_height_for_kind,
};
use cditor_core::rich_text::{
    BlockAttrs, BlockPayload, BlockPayloadRecord, BlockPayloadView, ClipboardBlock,
    ClipboardBlockFragment, ClipboardFragmentBoundary, ClipboardSelection, ImagePayload,
    InlineColorTarget, InlineMark, InlineSpan, MarkdownImportOptions, ParsedMarkdownDocument,
    RichBlockKind, RichBlockRecord, RichTextDocument, TableCellAlign, TableCellMerge, TableRange,
    TableTrackSize, block_kind_shortcut_with_marker_len, code_fence_shortcut,
    import_markdown_block_incremental, kind_tag_for_rich_block_kind, looks_like_markdown_paste,
    markdown_inline_shortcut_spans, parse_callout_marker, parse_markdown_document,
    plain_text_from_spans, rich_block_kind_from_tag,
};
use cditor_editor::debug_overlay::DebugOverlaySnapshot;
use cditor_editor::scroll::{
    CaretAnchor, HeightCorrectionPriority, PendingHeightCorrection, ScrollOrigin, ScrollbarDragEnd,
    ScrollbarDragSession, ScrollbarDragUpdate, ScrollbarPolicy, ScrollbarVisualState,
    VirtualScrollState,
};
use cditor_editor::window::{
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
        eprintln!("[cditor][input][runtime][{event}] {details}");
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
        eprintln!("[cditor][table][runtime][{event}] {details}");
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
        eprintln!("[cditor][block-color][runtime][{event}] {details}");
    }
}

pub use selection::RichTextSelectionSnapshot;
pub use state::{DocumentRuntime, GlobalScrollTarget};
use state::{
    EnterSplitMode, FocusedTableCell, PendingMeasuredHeight, RuntimeUndoEvent,
    StructureMoveUndoStep, StructurePasteUndoStep, TextSnapshot,
};
pub use table::TableClipboardSnapshot;

use inline_color::set_color_mark_for_range;
use table::{default_table_payload, ensure_table_payload_for_kind};
use text_payload::{
    append_plain_text_to_payload, backspace_at_start_resets_kind_to_paragraph,
    newline_sibling_kind_for_v1, next_grapheme_boundary, payload_for_kind_from_plain_text,
    prepend_plain_text_to_payload, previous_char_boundary, previous_grapheme_boundary,
    replace_rich_text_spans_with_spans, safe_char_range, slice_rich_text_spans,
    split_payload_for_enter, sync_payload_from_model_after_replace, text_payload_for_existing,
    toggle_mark_for_range, uses_soft_tab,
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

fn normalize_payload_record_for_kind(mut record: BlockPayloadRecord) -> BlockPayloadRecord {
    record.payload = table::ensure_table_payload_for_kind(&record.kind, record.payload);
    record
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
            eprintln!("[cditor][timing] {label} count={count} elapsed_ms={elapsed_ms:.2}");
        } else {
            eprintln!("[cditor][timing] {label} elapsed_ms={elapsed_ms:.2}");
        }
    }
}

fn record_index_of(records: &[BlockIndexRecord], block_id: BlockId) -> Option<usize> {
    records.iter().position(|record| record.id == block_id)
}

fn record_subtree_end(records: &[BlockIndexRecord], index: usize) -> usize {
    let depth = records[index].depth;
    let mut end = index + 1;
    while end < records.len() && records[end].depth > depth {
        end += 1;
    }
    end
}

fn insertion_index_for_parent_sibling(
    records: &[BlockIndexRecord],
    parent_id: Option<BlockId>,
    sibling_index: usize,
) -> usize {
    let direct_children = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| (record.parent_id == parent_id).then_some((index, record.id)))
        .collect::<Vec<_>>();
    if let Some((index, _)) = direct_children.get(sibling_index).copied() {
        return index;
    }
    if let Some((index, _)) = direct_children.last().copied() {
        return record_subtree_end(records, index);
    }
    parent_id
        .and_then(|parent_id| record_index_of(records, parent_id))
        .map(|parent_index| parent_index + 1)
        .unwrap_or(records.len())
}

fn apply_subtree_depth_delta(
    records: &mut [BlockIndexRecord],
    old_root_depth: u16,
    new_root_depth: u16,
) {
    if new_root_depth >= old_root_depth {
        let delta = new_root_depth - old_root_depth;
        for record in records {
            record.depth = record.depth.saturating_add(delta);
        }
    } else {
        let delta = old_root_depth - new_root_depth;
        for record in records {
            record.depth = record.depth.saturating_sub(delta);
        }
    }
}

fn estimate_text_block_height_for_text(kind: &RichBlockKind, text: &str) -> f64 {
    estimate_text_payload_height(kind, text, DEFAULT_LAYOUT_WIDTH_PX).height
}

fn estimate_payload_height(payload: &BlockPayloadRecord, _index: usize) -> f64 {
    match (&payload.kind, &payload.payload) {
        (RichBlockKind::Table, BlockPayload::Table(table)) => {
            f64::from(table::table_payload_projected_height_px(table))
        }
        _ => estimate_block_height(&payload.kind, &payload.payload, DEFAULT_LAYOUT_WIDTH_PX).height,
    }
}

#[cfg(test)]
mod tests;
