mod composition;
mod constructors;
mod focus;
mod layout_heights;
mod markdown_paste;
mod media;
mod payload_window;
mod scroll;
mod selection;
mod store_loading;
mod structure_edit;
mod text_edit;
mod undo_redo;

pub use store_loading::{
    DocumentRuntimeColdStartReport, DocumentRuntimeFromStoreOptions, DocumentRuntimeIndexSource,
};

use self::selection::FocusedTextSelection;

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ops::Range,
    sync::OnceLock,
    time::Instant,
};

use crate::core::document::{BlockIndexRecord, DocumentIndex, VisibleDocumentIndex};
use crate::core::edit::{
    DocumentSelection, EditOperation, EditTransaction, EditTransactionKind, InternalTextOffset,
    SelectionRange, TextOffsetMap, TextPosition,
};
use crate::core::ids::{BlockId, DocumentId};
use crate::core::layout::{
    BlockHeightIndex, BlockLayoutMeta, DEFAULT_LAYOUT_WIDTH_PX, HeightConfidence, HeightEstimate,
    IMAGE_BLOCK_ESTIMATED_HEIGHT_PX, PageLayoutIndex, PagePolicy, estimate_block_height,
    estimate_text_payload_height,
};
use crate::core::rich_text::{
    BlockAttrs, BlockPayload, BlockPayloadRecord, BlockPayloadView, ImagePayload, InlineMark,
    InlineSpan, MarkdownImportOptions, ParsedMarkdownDocument, RichBlockKind, RichBlockRecord,
    RichTextDocument, block_kind_shortcut_with_marker_len, code_fence_shortcut,
    import_markdown_block_incremental, kind_tag_for_rich_block_kind, looks_like_markdown_paste,
    markdown_inline_shortcut_spans, parse_markdown_document, rich_block_kind_from_tag,
};
use crate::editor::debug_overlay::DebugOverlaySnapshot;
use crate::editor::scroll::{
    CaretAnchor, HeightCorrectionPriority, PendingHeightCorrection, ScrollOrigin, ScrollbarDragEnd,
    ScrollbarDragSession, ScrollbarDragUpdate, ScrollbarPolicy, ScrollbarVisualState,
    VirtualScrollState,
};
use crate::editor::window::{
    PlaceholderWindow, RenderWindow, ScrollDirection, WindowPlanDecision, WindowPlanRequest,
    WindowPlanner, WindowPlannerPolicy,
};
use crate::runtime::payload_window::{
    PayloadWindowApplyDecision, PayloadWindowLoadRequest, PayloadWindowLoadResult,
};
use crate::runtime::{
    CompositionState, EditingSession, ListProjectionCache, PayloadWindow, PieceTableTextModel,
    SingleCharInputHotPath,
};
use crate::storage::layout_cache::{CacheSource, LayoutCacheKey};
use crate::storage::postgres::types::runtime_document_id_from_pg;
use crate::storage::postgres::{
    PgDocumentId, PostgresDocumentStore, PostgresLayoutCacheStore, PostgresPayloadStore,
    PostgresStorageError, PostgresStorageResult,
};

use super::{EditorViewProjection, ViewBlockSnapshot};

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

#[derive(Debug)]
pub struct DocumentRuntime {
    pub document_id: DocumentId,
    pub index: DocumentIndex,
    pub visible_index: VisibleDocumentIndex,
    pub height_index: BlockHeightIndex,
    pub page_layout: PageLayoutIndex,
    pub scroll: VirtualScrollState,
    pub editing: Option<EditingSession>,
    pub payload_window: PayloadWindow,
    text_models: HashMap<BlockId, PieceTableTextModel>,
    selected_block_ids: HashSet<BlockId>,
    list_projection_cache: ListProjectionCache,
    document_selection: Option<DocumentSelection>,
    focused_text_selection: Option<FocusedTextSelection>,
    undo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    redo_stacks: HashMap<BlockId, Vec<TextSnapshot>>,
    structure_undo_stack: Vec<StructureMoveUndoStep>,
    structure_redo_stack: Vec<StructureMoveUndoStep>,
    paste_undo_stack: Vec<StructurePasteUndoStep>,
    paste_redo_stack: Vec<StructurePasteUndoStep>,
    undo_events: Vec<RuntimeUndoEvent>,
    redo_events: Vec<RuntimeUndoEvent>,
    pending_structure_transactions: Vec<EditTransaction>,
    next_transaction_id: u64,
    hot_path: SingleCharInputHotPath,
    payload_window_generation: u64,
    window_planner: WindowPlanner,
    last_planned_scroll_top: f64,
    window_plan_clock_ms: u64,
    pending_measured_heights: HashMap<BlockId, PendingMeasuredHeight>,
    layout_dirty: bool,
    scrollbar_drag: Option<ScrollbarDragSession>,
    last_successful_projection: Option<EditorViewProjection>,
    demo_payload_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingMeasuredHeight {
    content_version: u64,
    height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnterSplitMode {
    InheritV1Kind,
    ForceParagraph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StructureMoveUndoStep {
    block_id: BlockId,
    old_parent_id: Option<BlockId>,
    old_sibling_index: usize,
    new_parent_id: Option<BlockId>,
    new_sibling_index: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct StructurePasteUndoStep {
    current_block_id: BlockId,
    before_current_record: BlockIndexRecord,
    before_current_payload: BlockPayloadRecord,
    after_current_record: BlockIndexRecord,
    after_current_payload: BlockPayloadRecord,
    inserted_records: Vec<BlockIndexRecord>,
    inserted_payloads: Vec<BlockPayloadRecord>,
    deleted_records: Vec<BlockIndexRecord>,
    deleted_payloads: Vec<BlockPayloadRecord>,
    before_focus: Option<(BlockId, usize)>,
    after_focus: Option<(BlockId, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeUndoEvent {
    Text(BlockId),
    StructureMove,
    StructurePaste,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalScrollTarget {
    pub global_scroll_top: f64,
    pub block_index: usize,
    pub block_id: BlockId,
    pub block_top: f64,
    pub offset_in_block: f64,
    pub page_index: usize,
    pub page_top: f64,
    pub offset_in_page: f64,
    pub precision: crate::editor::scroll::ScrollPrecision,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct TextSnapshot {
    text: String,
    content_version: u64,
}

impl DocumentRuntime {
    pub fn block_content_version(&self, block_id: BlockId) -> Option<u64> {
        self.payload_window
            .get(block_id)
            .map(|payload| payload.content_version)
    }

    pub fn block_payload_record(&self, block_id: BlockId) -> Option<BlockPayloadRecord> {
        self.payload_window
            .get(block_id)
            .cloned()
            .map(|payload| self.payload_with_composition_preview(block_id, payload))
    }

    pub fn projection_for_window(&self) -> EditorViewProjection {
        let page_range = self.current_page_window();
        let block_range = self.block_range_for_page_window(&page_range);
        self.projection_for_ranges(page_range, block_range)
    }

    pub fn projection_for_window_planned(&mut self) -> EditorViewProjection {
        let total_start = Instant::now();
        let (page_range, block_range) = if self.demo_payload_count.is_some() {
            self.demo_viewport_window_ranges()
        } else {
            let page_range = self.current_page_window_planned();
            let block_range = self.block_range_for_page_window(&page_range);
            (page_range, block_range)
        };
        self.ensure_demo_payload_window(&block_range);
        let projection = self.projection_for_ranges(page_range, block_range);
        let projection =
            if self.scrollbar_drag.is_some() && projection.render_window.is_placeholder() {
                self.last_successful_projection
                    .clone()
                    .unwrap_or(projection)
            } else {
                if !projection.render_window.is_placeholder() {
                    self.last_successful_projection = Some(projection.clone());
                }
                projection
            };
        log_runtime_timing(
            "runtime.projection_for_window_planned",
            total_start,
            Some(projection.blocks.len()),
        );
        projection
    }

    pub fn projection(&self) -> EditorViewProjection {
        self.projection_for_ranges(
            0..self.page_layout.page_count(),
            0..self.visible_index.total_visible_count(),
        )
    }

    fn demo_viewport_window_ranges(&self) -> (Range<usize>, Range<usize>) {
        let total_visible = self.visible_index.total_visible_count();
        if total_visible == 0 {
            return (0..0, 0..0);
        }
        let current = self
            .target_for_global_offset(self.scroll.global_scroll_top)
            .map(|target| target.block_index)
            .unwrap_or(0)
            .min(total_visible - 1);
        let viewport_end = self
            .height_index
            .block_at_offset(self.scroll.global_scroll_top + self.scroll.viewport_height)
            .map(|hit| hit.index)
            .unwrap_or(current)
            .min(total_visible - 1);
        let overscan = 48usize;
        let max_blocks = 320usize;
        let start = current.saturating_sub(overscan);
        let natural_end = viewport_end.saturating_add(overscan + 1).min(total_visible);
        let end = natural_end
            .min(start.saturating_add(max_blocks))
            .max(start + 1);
        let page = self
            .page_layout
            .page_for_block_index(current)
            .unwrap_or(0)
            .min(self.page_layout.page_count().saturating_sub(1));
        (
            page..page.saturating_add(1).min(self.page_layout.page_count()),
            start..end,
        )
    }

    fn ensure_demo_payload_window(&mut self, block_range: &Range<usize>) {
        let Some(count) = self.demo_payload_count else {
            return;
        };
        if block_range.is_empty() || self.payload_window_covers(block_range) {
            return;
        }

        let total_visible = self.visible_index.total_visible_count();
        let preload = 256usize;
        let start = block_range.start.saturating_sub(preload);
        let end = block_range.end.saturating_add(preload).min(total_visible);
        let payload_range = start..end;
        let start_time = Instant::now();
        let payloads = crate::runtime::demo_fixtures::large_mixed_demo_payload_records(
            payload_range.clone(),
            count,
        );
        let payload_count = payloads.len();

        self.payload_window = PayloadWindow::new(payload_range.clone());
        self.text_models.clear();
        for payload in payloads {
            self.text_models.insert(
                payload.block_id,
                PieceTableTextModel::new(payload.plain_text()),
            );
            self.payload_window.insert(payload);
        }
        eprintln!(
            "[cditor][timing] demo_payload_window range={:?} payloads={} elapsed_ms={:.2}",
            payload_range,
            payload_count,
            start_time.elapsed().as_secs_f64() * 1000.0
        );
    }

    fn block_range_for_page_window(&self, page_range: &Range<usize>) -> Range<usize> {
        let total_visible = self.visible_index.total_visible_count();
        let page_count = self.page_layout.page_count();
        if page_range.is_empty() || page_count == 0 || total_visible == 0 {
            return 0..0;
        }

        let start_page = page_range.start.min(page_count);
        let end_page = page_range.end.min(page_count);
        if start_page >= end_page {
            return 0..0;
        }

        let start = self.page_layout.pages[start_page]
            .block_start
            .min(total_visible);
        let end = self.page_layout.pages[end_page - 1]
            .block_end()
            .min(total_visible);
        start..end.max(start)
    }

    fn projection_for_ranges(
        &self,
        page_range: Range<usize>,
        block_range: Range<usize>,
    ) -> EditorViewProjection {
        let total_visible_blocks = self.visible_index.total_visible_count();
        let block_start = block_range.start.min(total_visible_blocks);
        let block_end = block_range.end.min(total_visible_blocks).max(block_start);
        let block_range = block_start..block_end;
        if !self.payload_window_covers(&block_range) {
            return self.placeholder_projection_for_ranges(page_range, block_range);
        }
        let block_ids = self.visible_index.visible_block_ids[block_range.clone()].to_vec();
        let local_height_index =
            BlockHeightIndex::new(block_ids.iter().enumerate().map(|(local_index, block_id)| {
                let source_index = self
                    .index
                    .index_of(*block_id)
                    .unwrap_or(block_range.start + local_index);
                HeightEstimate::new(
                    self.index.layout_meta[source_index].effective_height(),
                    HeightConfidence::Historical,
                    4.0,
                )
            }))
            .expect("projection local heights are valid");
        let render_window = RenderWindow::loaded(
            page_range,
            block_range.clone(),
            &block_ids,
            local_height_index,
            1,
        )
        .expect("projection render window is valid");
        let selection_fragments = self
            .document_selection
            .and_then(|selection| selection.normalize(&self.index).ok())
            .and_then(|selection| {
                selection
                    .visible_selection_fragments(
                        block_range.clone(),
                        &self.index,
                        &self.visible_index,
                        |block_id| {
                            self.text_models
                                .get(&block_id)
                                .map(|model| model.len())
                                .unwrap_or(0)
                        },
                    )
                    .ok()
            })
            .unwrap_or_default();
        let selection_ranges = selection_fragments
            .into_iter()
            .map(|fragment| (fragment.block_id, fragment.range))
            .collect::<HashMap<_, _>>();
        let blocks = block_ids
            .iter()
            .enumerate()
            .map(|(local_index, block_id)| {
                let visible_index = block_range.start + local_index;
                let source_index = self.index.index_of(*block_id).unwrap_or(visible_index);
                let marked_range = self
                    .active_composition()
                    .filter(|composition| composition.block_id == *block_id)
                    .and_then(|_| self.active_composition_marked_range());
                let payload = self
                    .payload_window
                    .get(*block_id)
                    .cloned()
                    .map(|payload| self.payload_with_composition_preview(*block_id, payload))
                    .map(BlockPayloadView::Loaded)
                    .unwrap_or(BlockPayloadView::Placeholder {
                        estimated_height: 32.0,
                    });
                let kind = match &payload {
                    BlockPayloadView::Loaded(payload) => payload.kind.clone(),
                    _ => rich_block_kind_from_tag(self.index.kind_tags[source_index]),
                };
                let selection_range = selection_ranges.get(block_id).cloned();
                let mut layout = self.index.layout_meta[source_index];
                if matches!(kind, RichBlockKind::Image)
                    && layout.effective_height() < IMAGE_BLOCK_ESTIMATED_HEIGHT_PX
                {
                    layout.estimated_height = IMAGE_BLOCK_ESTIMATED_HEIGHT_PX;
                    layout.measured_height = None;
                    layout.dirty = true;
                }
                let chrome = self
                    .list_projection_cache
                    .entry(source_index)
                    .map(|entry| {
                        crate::core::block::BlockChromeSnapshot::from_kind(
                            &kind,
                            entry.list_info,
                            entry.chrome.has_children,
                            entry.chrome.collapsed,
                        )
                    })
                    .unwrap_or_else(crate::core::block::BlockChromeSnapshot::plain);
                ViewBlockSnapshot {
                    block_id: *block_id,
                    visible_index,
                    depth: self.index.depths[source_index],
                    chrome,
                    kind,
                    attrs: BlockAttrs::default(),
                    payload,
                    layout,
                    selected: self.selected_block_ids.contains(block_id)
                        || matches!(selection_range, Some(SelectionRange::Full)),
                    selection_range,
                    focused: self.focused_block_id() == Some(*block_id),
                    caret_offset: self
                        .editing
                        .as_ref()
                        .filter(|editing| editing.block_id == *block_id)
                        .map(|editing| editing.caret_anchor.text_offset as usize),
                    marked_range,
                    pinned: self
                        .editing
                        .as_ref()
                        .is_some_and(|editing| editing.is_pinned(*block_id)),
                    placeholder: false,
                }
            })
            .collect::<Vec<_>>();
        let before_window_height = self
            .height_index
            .offset_of_block(render_window.block_range.start)
            .unwrap_or(0.0);
        let window_height = render_window.height();
        let after_window_height =
            (self.page_layout.total_height() - before_window_height - window_height).max(0.0);
        let debug = DebugOverlaySnapshot::from_scroll_state(
            &self.scroll,
            0,
            render_window.page_range.clone(),
        )
        .with_entity_stats(
            blocks.len(),
            blocks.iter().filter(|block| block.pinned).count(),
        );
        EditorViewProjection {
            document_id: self.document_id,
            scroll: self.scroll,
            render_window,
            blocks,
            before_window_height,
            placeholder_window_height: None,
            after_window_height,
            total_visible_blocks,
            debug,
        }
    }

    fn payload_window_covers(&self, block_range: &Range<usize>) -> bool {
        if block_range.is_empty() {
            return true;
        }
        if self.payload_window.block_range.start > block_range.start
            || block_range.end > self.payload_window.block_range.end
        {
            return false;
        }
        block_range.clone().all(|visible_index| {
            self.visible_index
                .id_at_visible_index(visible_index)
                .is_some_and(|block_id| self.payload_window.payloads.contains_key(&block_id))
        })
    }

    fn placeholder_projection_for_ranges(
        &self,
        page_range: Range<usize>,
        block_range: Range<usize>,
    ) -> EditorViewProjection {
        let total_visible_blocks = self.visible_index.total_visible_count();
        let before_window_height = self
            .height_index
            .offset_of_block(block_range.start)
            .unwrap_or(0.0);
        let placeholder_height = self.height_for_page_range(&page_range);
        let render_window = RenderWindow::placeholder(PlaceholderWindow {
            page_range: page_range.clone(),
            block_range,
            height: placeholder_height,
            target_anchor: self
                .target_for_global_offset(self.scroll.global_scroll_top)
                .map(|target| crate::editor::scroll::ScrollAnchor {
                    block_id: target.block_id,
                    offset_in_block: target.offset_in_block,
                    viewport_y: 0.0,
                }),
        });
        let after_window_height =
            (self.page_layout.total_height() - before_window_height - placeholder_height).max(0.0);
        let debug = DebugOverlaySnapshot::from_scroll_state(
            &self.scroll,
            0,
            render_window.page_range.clone(),
        )
        .with_entity_stats(0, 0);
        EditorViewProjection {
            document_id: self.document_id,
            scroll: self.scroll,
            render_window,
            blocks: Vec::new(),
            before_window_height,
            placeholder_window_height: Some(placeholder_height),
            after_window_height,
            total_visible_blocks,
            debug,
        }
    }

    fn height_for_page_range(&self, page_range: &Range<usize>) -> f64 {
        let page_count = self.page_layout.page_count();
        if page_range.is_empty() || page_count == 0 {
            return 0.0;
        }
        let start = page_range.start.min(page_count);
        let end = page_range.end.min(page_count).max(start);
        self.page_layout.pages[start..end]
            .iter()
            .map(|page| page.height)
            .sum()
    }
}

fn push_unique(block_ids: &mut Vec<BlockId>, block_id: BlockId) {
    if !block_ids.contains(&block_id) {
        block_ids.push(block_id);
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
            eprintln!("[cditor][timing] {label} count={count} elapsed_ms={elapsed_ms:.2}");
        } else {
            eprintln!("[cditor][timing] {label} elapsed_ms={elapsed_ms:.2}");
        }
    }
}

fn sync_payload_from_model_after_replace(
    payload_window: &mut PayloadWindow,
    block_id: BlockId,
    content_version: u64,
    model: &PieceTableTextModel,
    replaced_range: Range<usize>,
    inserted_text: &str,
) {
    if let Some(payload) = payload_window.payloads.get_mut(&block_id) {
        payload.content_version = content_version;
        payload.payload = text_payload_for_existing_after_replace(
            &payload.payload,
            model.text(),
            replaced_range,
            inserted_text,
        );
    }
}

fn merge_inline_spans(spans: &mut Vec<InlineSpan>) {
    let mut merged: Vec<InlineSpan> = Vec::new();
    for span in spans.drain(..) {
        if span.text.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut()
            && last.marks == span.marks
        {
            last.text.push_str(&span.text);
            continue;
        }
        merged.push(span);
    }
    if merged.is_empty() {
        merged.push(InlineSpan::plain(String::new()));
    }
    *spans = merged;
}

fn prepend_plain_text_to_payload(prefix: String, payload: BlockPayload) -> BlockPayload {
    if prefix.is_empty() {
        return payload;
    }
    match payload {
        BlockPayload::RichText { mut spans } => {
            spans.insert(0, InlineSpan::plain(prefix));
            merge_inline_spans(&mut spans);
            BlockPayload::RichText { spans }
        }
        BlockPayload::Code { language, text } => BlockPayload::Code {
            language,
            text: format!("{prefix}{text}"),
        },
        BlockPayload::Html { html, sanitized } => BlockPayload::Html {
            html: format!("{prefix}{html}"),
            sanitized,
        },
        other => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(format!("{prefix}{}", other.plain_text()))],
        },
    }
}

fn append_plain_text_to_payload(payload: BlockPayload, suffix: String) -> BlockPayload {
    if suffix.is_empty() {
        return payload;
    }
    match payload {
        BlockPayload::RichText { mut spans } => {
            spans.push(InlineSpan::plain(suffix));
            merge_inline_spans(&mut spans);
            BlockPayload::RichText { spans }
        }
        BlockPayload::Code { language, text } => BlockPayload::Code {
            language,
            text: format!("{text}{suffix}"),
        },
        BlockPayload::Html { html, sanitized } => BlockPayload::Html {
            html: format!("{html}{suffix}"),
            sanitized,
        },
        other => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(format!("{}{suffix}", other.plain_text()))],
        },
    }
}

fn text_payload_for_existing(existing: &BlockPayload, text: &str) -> BlockPayload {
    match existing {
        BlockPayload::Code { language, .. } => BlockPayload::Code {
            language: language.clone(),
            text: text.to_owned(),
        },
        BlockPayload::Html { sanitized, .. } => BlockPayload::Html {
            html: text.to_owned(),
            sanitized: sanitized.clone(),
        },
        _ => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(text)],
        },
    }
}

fn text_payload_for_existing_after_replace(
    existing: &BlockPayload,
    updated_text: &str,
    replaced_range: Range<usize>,
    inserted_text: &str,
) -> BlockPayload {
    match existing {
        BlockPayload::Code { language, .. } => BlockPayload::Code {
            language: language.clone(),
            text: updated_text.to_owned(),
        },
        BlockPayload::Html { sanitized, .. } => BlockPayload::Html {
            html: updated_text.to_owned(),
            sanitized: sanitized.clone(),
        },
        BlockPayload::RichText { spans } => BlockPayload::RichText {
            spans: replace_rich_text_spans_preserving_marks(spans, replaced_range, inserted_text),
        },
        _ => text_payload_for_existing(existing, updated_text),
    }
}

fn replace_rich_text_spans_preserving_marks(
    spans: &[InlineSpan],
    replaced_range: Range<usize>,
    inserted_text: &str,
) -> Vec<InlineSpan> {
    let mut output = Vec::new();
    let mut cursor = 0usize;
    let insertion_marks = marks_for_insertion(spans, replaced_range.start);
    let mut inserted = false;
    for span in spans {
        let span_start = cursor;
        let span_end = span_start + span.text.len();
        if span_end <= replaced_range.start || span_start >= replaced_range.end {
            if !inserted && span_start >= replaced_range.end {
                push_inline_span(&mut output, inserted_text, insertion_marks.clone());
                inserted = true;
            }
            output.push(span.clone());
        } else {
            let keep_prefix_end = replaced_range
                .start
                .saturating_sub(span_start)
                .min(span.text.len());
            let keep_suffix_start = replaced_range
                .end
                .saturating_sub(span_start)
                .min(span.text.len());
            if keep_prefix_end > 0 {
                push_inline_span(
                    &mut output,
                    &span.text[..keep_prefix_end],
                    span.marks.clone(),
                );
            }
            if !inserted {
                push_inline_span(&mut output, inserted_text, insertion_marks.clone());
                inserted = true;
            }
            if keep_suffix_start < span.text.len() {
                push_inline_span(
                    &mut output,
                    &span.text[keep_suffix_start..],
                    span.marks.clone(),
                );
            }
        }
        cursor = span_end;
    }
    if !inserted {
        push_inline_span(&mut output, inserted_text, insertion_marks);
    }
    merge_inline_spans(&mut output);
    if output.is_empty() {
        output.push(InlineSpan::plain(String::new()));
    }
    output
}

fn marks_for_insertion(spans: &[InlineSpan], offset: usize) -> Vec<InlineMark> {
    let mut cursor = 0usize;
    for span in spans {
        let span_start = cursor;
        let span_end = span_start + span.text.len();
        if span_start <= offset && offset < span_end {
            return span.marks.clone();
        }
        if offset == span_end && !span.marks.is_empty() {
            return span.marks.clone();
        }
        cursor = span_end;
    }
    Vec::new()
}

fn push_inline_span(output: &mut Vec<InlineSpan>, text: &str, marks: Vec<InlineMark>) {
    if !text.is_empty() {
        output.push(InlineSpan {
            text: text.to_owned(),
            marks,
        });
    }
}

fn backspace_at_start_resets_kind_to_paragraph(kind: &RichBlockKind) -> bool {
    matches!(
        kind,
        RichBlockKind::Heading { .. }
            | RichBlockKind::Quote
            | RichBlockKind::Callout { .. }
            | RichBlockKind::Todo { .. }
            | RichBlockKind::BulletedList
            | RichBlockKind::NumberedList
            | RichBlockKind::Toggle
            | RichBlockKind::Code { .. }
            | RichBlockKind::Math
            | RichBlockKind::Mermaid
            | RichBlockKind::Html
            | RichBlockKind::FootnoteDefinition
            | RichBlockKind::Comment
            | RichBlockKind::RawMarkdown
            | RichBlockKind::Custom(_)
    )
}

fn uses_soft_tab(kind: &RichBlockKind) -> bool {
    matches!(
        kind,
        RichBlockKind::Code { .. }
            | RichBlockKind::RawMarkdown
            | RichBlockKind::Quote
            | RichBlockKind::Callout { .. }
    )
}

fn newline_sibling_kind_for_v1(kind: &RichBlockKind) -> RichBlockKind {
    match kind {
        RichBlockKind::Todo { .. } => RichBlockKind::Todo { checked: false },
        RichBlockKind::BulletedList => RichBlockKind::BulletedList,
        RichBlockKind::NumberedList => RichBlockKind::NumberedList,
        RichBlockKind::Quote => RichBlockKind::Quote,
        RichBlockKind::Callout { variant } => RichBlockKind::Callout { variant: *variant },
        _ => RichBlockKind::Paragraph,
    }
}

fn split_payload_for_enter(
    payload: &BlockPayload,
    offset: usize,
    new_kind: &RichBlockKind,
) -> (BlockPayload, BlockPayload) {
    match payload {
        BlockPayload::RichText { spans } => {
            let (leading, trailing) = split_inline_spans_at_offset(spans, offset);
            (
                BlockPayload::RichText { spans: leading },
                payload_for_kind_from_plain_or_spans(new_kind, trailing),
            )
        }
        BlockPayload::Code { language, text } => {
            let offset = previous_char_boundary(text, offset.min(text.len()));
            let leading = text[..offset].to_owned();
            let trailing = text[offset..].to_owned();
            (
                BlockPayload::Code {
                    language: language.clone(),
                    text: leading,
                },
                payload_for_kind_from_plain_text(new_kind, trailing),
            )
        }
        BlockPayload::Html { html, sanitized } => {
            let offset = previous_char_boundary(html, offset.min(html.len()));
            let leading = html[..offset].to_owned();
            let trailing = html[offset..].to_owned();
            (
                BlockPayload::Html {
                    html: leading,
                    sanitized: *sanitized,
                },
                payload_for_kind_from_plain_text(new_kind, trailing),
            )
        }
        other => {
            let text = other.plain_text();
            let offset = previous_char_boundary(&text, offset.min(text.len()));
            let leading = text[..offset].to_owned();
            let trailing = text[offset..].to_owned();
            (
                payload_for_kind_from_plain_text(new_kind, leading),
                payload_for_kind_from_plain_text(new_kind, trailing),
            )
        }
    }
}

fn payload_for_kind_from_plain_or_spans(
    kind: &RichBlockKind,
    spans: Vec<InlineSpan>,
) -> BlockPayload {
    match kind {
        RichBlockKind::Code { language } => BlockPayload::Code {
            language: language.clone(),
            text: crate::core::rich_text::plain_text_from_spans(&spans),
        },
        RichBlockKind::Html => BlockPayload::Html {
            html: crate::core::rich_text::plain_text_from_spans(&spans),
            sanitized: true,
        },
        _ => BlockPayload::RichText { spans },
    }
}

fn payload_for_kind_from_plain_text(kind: &RichBlockKind, text: String) -> BlockPayload {
    match kind {
        RichBlockKind::Code { language } => BlockPayload::Code {
            language: language.clone(),
            text,
        },
        RichBlockKind::Html => BlockPayload::Html {
            html: text,
            sanitized: true,
        },
        _ => BlockPayload::RichText {
            spans: vec![InlineSpan::plain(text)],
        },
    }
}

fn split_inline_spans_at_offset(
    spans: &[InlineSpan],
    offset: usize,
) -> (Vec<InlineSpan>, Vec<InlineSpan>) {
    let mut leading = Vec::new();
    let mut trailing = Vec::new();
    let mut cursor = 0usize;
    let split_offset = offset.min(crate::core::rich_text::plain_text_from_spans(spans).len());

    for span in spans {
        let span_start = cursor;
        let span_end = cursor + span.text.len();
        if span_end <= split_offset {
            leading.push(span.clone());
        } else if span_start >= split_offset {
            trailing.push(span.clone());
        } else {
            let local = previous_char_boundary(&span.text, split_offset - span_start);
            let left_text = span.text[..local].to_owned();
            let right_text = span.text[local..].to_owned();
            if !left_text.is_empty() {
                leading.push(InlineSpan {
                    text: left_text,
                    marks: span.marks.clone(),
                });
            }
            if !right_text.is_empty() {
                trailing.push(InlineSpan {
                    text: right_text,
                    marks: span.marks.clone(),
                });
            }
        }
        cursor = span_end;
    }

    if leading.is_empty() {
        leading.push(InlineSpan::plain(String::new()));
    }
    if trailing.is_empty() {
        trailing.push(InlineSpan::plain(String::new()));
    }
    (leading, trailing)
}

fn previous_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn previous_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = previous_char_boundary(text, offset);
    text[..offset]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = next_char_boundary(text, offset);
    text[offset..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| offset + index)
        .unwrap_or(text.len())
}

fn next_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset < text.len() && !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

fn safe_char_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = previous_char_boundary(text, range.start.min(text.len()));
    let end = next_char_boundary(text, range.end.min(text.len())).max(start);
    start..end
}

fn spans_with_mark_for_range(text: &str, range: Range<usize>, mark: InlineMark) -> Vec<InlineSpan> {
    let range = safe_char_range(text, range);
    let mut spans = Vec::new();
    if range.start > 0 {
        spans.push(InlineSpan::plain(&text[..range.start]));
    }
    spans.push(InlineSpan {
        text: text[range.clone()].to_owned(),
        marks: vec![mark],
    });
    if range.end < text.len() {
        spans.push(InlineSpan::plain(&text[range.end..]));
    }
    spans
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
    estimate_block_height(&payload.kind, &payload.payload, DEFAULT_LAYOUT_WIDTH_PX).height
}

#[cfg(test)]
mod tests {
    use crate::core::block::BlockPrefixSnapshot;

    use super::*;

    fn runtime_with_paragraph_blocks(count: usize) -> DocumentRuntime {
        let records = (1..=count as BlockId)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
                .with_layout_meta(crate::core::layout::BlockLayoutMeta::new(block_id, 32.0))
            })
            .collect::<Vec<_>>();
        let payloads = (1..=count as BlockId)
            .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
            .collect::<Vec<_>>();
        DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0)
    }

    fn runtime_with_kind_depths(
        kinds_and_depths: Vec<(RichBlockKind, u16, Option<BlockId>)>,
    ) -> DocumentRuntime {
        runtime_with_kind_depths_and_text(
            kinds_and_depths
                .into_iter()
                .map(|(kind, depth, parent_id)| (kind, depth, parent_id, "item"))
                .collect(),
        )
    }

    fn runtime_with_kind_depths_and_text(
        blocks: Vec<(RichBlockKind, u16, Option<BlockId>, &str)>,
    ) -> DocumentRuntime {
        let records = blocks
            .iter()
            .enumerate()
            .map(|(index, (kind, depth, parent_id, _text))| {
                let block_id = (index + 1) as BlockId;
                BlockIndexRecord::new(
                    block_id,
                    *parent_id,
                    *depth,
                    kind_tag_for_rich_block_kind(kind),
                    0,
                )
                .with_layout_meta(crate::core::layout::BlockLayoutMeta::new(block_id, 32.0))
            })
            .collect::<Vec<_>>();
        let payloads = blocks
            .into_iter()
            .enumerate()
            .map(|(index, (kind, _, _, text))| {
                BlockPayloadRecord::rich_text((index + 1) as BlockId, kind, text)
            })
            .collect::<Vec<_>>();
        DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0)
    }

    #[test]
    fn measured_height_marks_layout_dirty_until_saved() {
        let mut runtime = runtime_with_single_payload(
            RichBlockKind::Image,
            BlockPayload::Image(ImagePayload {
                source: "/tmp/paste.png".to_owned(),
                alt: "paste.png".to_owned(),
                caption: String::new(),
                display_width_ratio_milli: None,
            }),
        );

        assert!(!runtime.has_dirty_layout());
        assert!(runtime.apply_measured_height(1, 1, 512.0).unwrap());
        assert!(runtime.has_dirty_layout());
        runtime.mark_layout_saved();
        assert!(!runtime.has_dirty_layout());
    }

    #[test]
    fn image_projection_clamps_legacy_short_layout_height() {
        let mut runtime = runtime_with_single_payload(
            RichBlockKind::Image,
            BlockPayload::Image(ImagePayload {
                source: "/tmp/paste.png".to_owned(),
                alt: "paste.png".to_owned(),
                caption: String::new(),
                display_width_ratio_milli: None,
            }),
        );
        runtime.index.layout_meta[0].estimated_height = 220.0;

        let projection = runtime.projection_for_window_planned();

        assert_eq!(
            projection.blocks[0].layout.effective_height(),
            IMAGE_BLOCK_ESTIMATED_HEIGHT_PX
        );
    }

    #[test]
    fn image_asset_insert_creates_image_block_and_trailing_paragraph() {
        let mut runtime =
            runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "hello")]);
        runtime.focus_block_at_offset(1, 5).unwrap();

        let (image_block_id, trailing_block_id) = runtime
            .insert_image_asset_after_focused(ImagePayload {
                source: "/tmp/paste.png".to_owned(),
                alt: "paste.png".to_owned(),
                caption: String::new(),
                display_width_ratio_milli: None,
            })
            .unwrap();

        assert_eq!(runtime.index.total_count(), 3);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::Image);
        assert_eq!(runtime.kind_at_index(2), RichBlockKind::Paragraph);
        assert_eq!(runtime.focused_block_id(), Some(trailing_block_id));
        let image_payload = runtime.block_payload_record(image_block_id).unwrap();
        assert!(matches!(image_payload.payload, BlockPayload::Image(_)));
    }

    #[test]
    fn markdown_paste_heading_replaces_current_block_and_preserves_prefix_suffix() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::Paragraph,
            0,
            None,
            "hello world",
        )]);
        runtime.focus_block_at_offset(1, 5).unwrap();

        assert!(runtime.insert_markdown_paste("# Title").unwrap());

        assert_eq!(runtime.index.total_count(), 1);
        assert_eq!(
            runtime.kind_at_index(0),
            RichBlockKind::Heading { level: 1 }
        );
        assert_eq!(
            runtime.payload_window.get(1).unwrap().plain_text(),
            "helloTitle world"
        );
        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.caret_offset_for_block(1), Some("helloTitle".len()));
    }

    #[test]
    fn markdown_paste_multiline_list_inserts_structured_siblings() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::Paragraph,
            0,
            None,
            "prefix suffix",
        )]);
        runtime.focus_block_at_offset(1, 7).unwrap();

        assert!(runtime.insert_markdown_paste("- one\n- two").unwrap());

        assert_eq!(runtime.index.total_count(), 2);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
        assert_eq!(
            runtime.payload_window.get(1).unwrap().plain_text(),
            "prefix one"
        );
        assert_eq!(
            runtime.payload_window.get(3).unwrap().plain_text(),
            "twosuffix"
        );
        assert_eq!(runtime.focused_block_id(), Some(3));
        assert_eq!(runtime.caret_offset_for_block(3), Some("two".len()));
    }

    #[test]
    fn markdown_paste_detection_scans_all_lines_like_v1() {
        assert!(crate::core::rich_text::looks_like_markdown_paste(
            "plain intro\n- item"
        ));
    }

    #[test]
    fn markdown_paste_deletes_cross_block_selection_and_undo_restores_it() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Paragraph, 0, None, "abc"),
            (RichBlockKind::Paragraph, 0, None, "def"),
            (RichBlockKind::Paragraph, 0, None, "ghi"),
        ]);
        runtime.set_document_text_selection(1, 1, 3, 1).unwrap();

        assert!(runtime.insert_markdown_paste("- x\n- y").unwrap());
        assert_eq!(runtime.index.total_count(), 2);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ax");
        assert_eq!(runtime.payload_window.get(5).unwrap().plain_text(), "yhi");

        assert!(runtime.undo_focused_block().unwrap());
        assert_eq!(runtime.index.total_count(), 3);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::Paragraph);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::Paragraph);
        assert_eq!(runtime.kind_at_index(2), RichBlockKind::Paragraph);
        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "abc");
        assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), "def");
        assert_eq!(runtime.payload_window.get(3).unwrap().plain_text(), "ghi");
        assert_eq!(runtime.focused_block_id(), Some(3));
        assert_eq!(runtime.caret_offset_for_block(3), Some(1));

        assert!(runtime.redo_focused_block().unwrap());
        assert_eq!(runtime.index.total_count(), 2);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ax");
        assert_eq!(runtime.payload_window.get(5).unwrap().plain_text(), "yhi");
    }

    #[test]
    fn markdown_paste_undo_redo_restores_structure_and_payloads() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::Paragraph,
            0,
            None,
            "prefix suffix",
        )]);
        runtime.focus_block_at_offset(1, 7).unwrap();

        assert!(runtime.insert_markdown_paste("- one\n- two").unwrap());
        assert_eq!(runtime.index.total_count(), 2);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);

        assert!(runtime.undo_focused_block().unwrap());
        assert_eq!(runtime.index.total_count(), 1);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::Paragraph);
        assert_eq!(
            runtime.payload_window.get(1).unwrap().plain_text(),
            "prefix suffix"
        );
        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.caret_offset_for_block(1), Some("prefix ".len()));

        assert!(runtime.redo_focused_block().unwrap());
        assert_eq!(runtime.index.total_count(), 2);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
        assert_eq!(
            runtime.payload_window.get(1).unwrap().plain_text(),
            "prefix one"
        );
        assert_eq!(
            runtime.payload_window.get(3).unwrap().plain_text(),
            "twosuffix"
        );
        assert_eq!(runtime.focused_block_id(), Some(3));
        assert_eq!(runtime.caret_offset_for_block(3), Some("two".len()));
    }

    #[test]
    fn markdown_paste_table_with_suffix_adds_trailing_paragraph() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::Paragraph,
            0,
            None,
            "before after",
        )]);
        runtime.focus_block_at_offset(1, 7).unwrap();

        assert!(
            runtime
                .insert_markdown_paste("| a | b |\n| - | - |")
                .unwrap()
        );

        assert_eq!(runtime.index.total_count(), 3);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::Paragraph);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::Table);
        assert_eq!(runtime.kind_at_index(2), RichBlockKind::Paragraph);
        assert_eq!(
            runtime.payload_window.get(1).unwrap().plain_text(),
            "before "
        );
        assert_eq!(runtime.payload_window.get(3).unwrap().plain_text(), "after");
        assert_eq!(runtime.focused_block_id(), Some(3));
        assert_eq!(runtime.caret_offset_for_block(3), Some("after".len()));
    }

    #[test]
    fn enter_on_bulleted_list_splits_and_inherits_kind() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::BulletedList,
            0,
            None,
            "hello world",
        )]);
        runtime.focus_block_at_offset(1, 5).unwrap();

        runtime.handle_enter().unwrap();

        assert_eq!(runtime.index.total_count(), 2);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "hello");
        assert_eq!(
            runtime.payload_window.get(2).unwrap().plain_text(),
            " world"
        );
        assert_eq!(runtime.focused_block_id(), Some(2));
        assert_eq!(runtime.caret_offset_for_block(2), Some(0));
    }

    #[test]
    fn enter_on_numbered_list_splits_and_inherits_kind() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::NumberedList,
            0,
            None,
            "one two",
        )]);
        runtime.focus_block_at_offset(1, 3).unwrap();

        runtime.handle_enter().unwrap();

        assert_eq!(runtime.kind_at_index(0), RichBlockKind::NumberedList);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::NumberedList);
        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "one");
        assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), " two");
        let projection = runtime.projection_for_window_planned();
        assert_eq!(
            projection.blocks[0].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 1 }
        );
        assert_eq!(
            projection.blocks[1].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 2 }
        );
    }

    #[test]
    fn enter_on_todo_splits_and_new_item_is_unchecked() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::Todo { checked: true },
            0,
            None,
            "done later",
        )]);
        runtime.focus_block_at_offset(1, 4).unwrap();

        runtime.handle_enter().unwrap();

        assert_eq!(
            runtime.kind_at_index(0),
            RichBlockKind::Todo { checked: true }
        );
        assert_eq!(
            runtime.kind_at_index(1),
            RichBlockKind::Todo { checked: false }
        );
        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "done");
        assert_eq!(
            runtime.payload_window.get(2).unwrap().plain_text(),
            " later"
        );
    }

    #[test]
    fn enter_splits_trailing_rich_spans_and_preserves_marks() {
        let record = BlockIndexRecord::new(
            1,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::BulletedList),
            0,
        )
        .with_layout_meta(crate::core::layout::BlockLayoutMeta::new(1, 32.0));
        let payload = BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::BulletedList,
            payload: BlockPayload::RichText {
                spans: vec![
                    InlineSpan::plain("ab"),
                    InlineSpan {
                        text: "cd".to_owned(),
                        marks: vec![InlineMark::Bold],
                    },
                ],
            },
        };
        let mut runtime =
            DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0);
        runtime.focus_block_at_offset(1, 3).unwrap();

        runtime.handle_enter().unwrap();

        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "abc");
        assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), "d");
        match &runtime.payload_window.get(2).unwrap().payload {
            BlockPayload::RichText { spans } => {
                assert_eq!(spans.len(), 1);
                assert_eq!(spans[0].text, "d");
                assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
            }
            other => panic!("expected rich text payload, got {other:?}"),
        }
    }

    #[test]
    fn command_enter_splits_but_forces_paragraph() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::NumberedList,
            0,
            None,
            "abcde",
        )]);
        runtime.focus_block_at_offset(1, 2).unwrap();

        let new_block_id = runtime.insert_paragraph_after_focused().unwrap();

        assert_eq!(new_block_id, 2);
        assert_eq!(runtime.kind_at_index(0), RichBlockKind::NumberedList);
        assert_eq!(runtime.kind_at_index(1), RichBlockKind::Paragraph);
        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");
        assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), "cde");
        assert_eq!(runtime.focused_block_id(), Some(2));
    }

    #[test]
    fn indent_focused_block_requires_previous_block_that_supports_children() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 0, None),
        ]);
        runtime.focus_block(2);
        let before_version = runtime.index.structure_version;

        assert!(runtime.indent_focused_block().unwrap());

        assert_eq!(runtime.index.structure_version, before_version + 1);
        assert_eq!(runtime.index.parent_ids[1], Some(1));
        assert_eq!(runtime.index.depths[1], 1);
        let projection = runtime.projection();
        assert!(projection.blocks[0].chrome.has_children);
        assert_eq!(projection.blocks[1].chrome.list_info.depth, 1);

        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::Paragraph, 0, None),
            (RichBlockKind::BulletedList, 0, None),
        ]);
        runtime.focus_block(2);
        assert!(!runtime.indent_focused_block().unwrap());
        assert_eq!(runtime.index.parent_ids[1], None);
        assert_eq!(runtime.index.depths[1], 0);
    }

    fn runtime_with_single_payload(kind: RichBlockKind, payload: BlockPayload) -> DocumentRuntime {
        let record = BlockIndexRecord::new(1, None, 0, kind_tag_for_rich_block_kind(&kind), 0)
            .with_layout_meta(crate::core::layout::BlockLayoutMeta::new(1, 32.0));
        let payload = BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind,
            payload,
        };
        DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0)
    }

    #[test]
    fn code_tab_inserts_four_spaces_without_structure_change() {
        let mut runtime = runtime_with_single_payload(
            RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main()".to_owned(),
            },
        );
        runtime.focus_block_at_offset(1, 2).unwrap();
        let before_structure_version = runtime.index.structure_version;

        assert!(runtime.indent_focused_block().unwrap());

        assert_eq!(runtime.focused_text().unwrap(), "fn     main()");
        assert_eq!(runtime.caret_offset_for_block(1), Some(6));
        assert_eq!(runtime.index.structure_version, before_structure_version);
        assert_eq!(runtime.pending_structure_transaction_count(), 0);
        match &runtime.payload_window.get(1).unwrap().payload {
            BlockPayload::Code { text, .. } => assert_eq!(text, "fn     main()"),
            other => panic!("expected code payload, got {other:?}"),
        }
    }

    #[test]
    fn code_shift_tab_removes_line_indent_without_structure_change() {
        let mut runtime = runtime_with_single_payload(
            RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {\n    value\n}".to_owned(),
            },
        );
        runtime
            .focus_block_at_offset(1, "fn main() {\n    va".len())
            .unwrap();
        let before_structure_version = runtime.index.structure_version;

        assert!(runtime.outdent_focused_block().unwrap());

        assert_eq!(runtime.focused_text().unwrap(), "fn main() {\nvalue\n}");
        assert_eq!(
            runtime.caret_offset_for_block(1),
            Some("fn main() {\nva".len())
        );
        assert_eq!(runtime.index.structure_version, before_structure_version);
        assert_eq!(runtime.pending_structure_transaction_count(), 0);
    }

    #[test]
    fn raw_markdown_tab_and_shift_tab_are_payload_only() {
        let mut runtime = runtime_with_single_payload(
            RichBlockKind::RawMarkdown,
            BlockPayload::RichText {
                spans: vec![InlineSpan::plain("alpha")],
            },
        );
        runtime.focus_block_at_offset(1, 0).unwrap();
        let before_structure_version = runtime.index.structure_version;

        assert!(runtime.indent_focused_block().unwrap());
        assert_eq!(runtime.focused_text().unwrap(), "    alpha");
        assert!(runtime.outdent_focused_block().unwrap());
        assert_eq!(runtime.focused_text().unwrap(), "alpha");
        assert_eq!(runtime.index.structure_version, before_structure_version);
        assert_eq!(runtime.pending_structure_transaction_count(), 0);
    }

    #[test]
    fn tab_indents_block_under_previous_sibling_children_tail() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 1, Some(1)),
            (RichBlockKind::BulletedList, 0, None),
        ]);
        runtime.focus_block(3);

        assert!(runtime.indent_focused_block().unwrap());

        assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
        assert_eq!(runtime.index.parent_ids[2], Some(1));
        assert_eq!(runtime.index.depths[2], 1);
        assert_eq!(runtime.direct_child_position(Some(1), 3), Some(1));
        assert_eq!(runtime.focused_block_id(), Some(3));
        assert_eq!(runtime.pending_structure_transaction_count(), 1);
    }

    #[test]
    fn tab_first_sibling_does_nothing() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 0, None),
        ]);
        runtime.focus_block(1);
        let before_version = runtime.index.structure_version;

        assert!(!runtime.indent_focused_block().unwrap());

        assert_eq!(runtime.index.structure_version, before_version);
        assert_eq!(runtime.index.parent_ids, vec![None, None]);
        assert_eq!(runtime.pending_structure_transaction_count(), 0);
    }

    #[test]
    fn tab_previous_non_container_does_nothing() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::Paragraph, 0, None),
            (RichBlockKind::BulletedList, 0, None),
        ]);
        runtime.focus_block(2);
        let before_version = runtime.index.structure_version;

        assert!(!runtime.indent_focused_block().unwrap());

        assert_eq!(runtime.index.structure_version, before_version);
        assert_eq!(runtime.index.parent_ids[1], None);
        assert_eq!(runtime.pending_structure_transaction_count(), 0);
    }

    #[test]
    fn outdent_focused_block_moves_subtree_up_one_level() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 1, Some(1)),
            (RichBlockKind::Todo { checked: false }, 2, Some(2)),
        ]);
        runtime.focus_block(2);
        let before_version = runtime.index.structure_version;

        assert!(runtime.outdent_focused_block().unwrap());

        assert_eq!(runtime.index.structure_version, before_version + 1);
        assert_eq!(runtime.index.parent_ids[1], None);
        assert_eq!(runtime.index.depths[1], 0);
        assert_eq!(runtime.index.parent_ids[2], Some(2));
        assert_eq!(runtime.index.depths[2], 1);
        let projection = runtime.projection();
        assert_eq!(projection.blocks[1].chrome.list_info.depth, 0);
        assert_eq!(projection.blocks[2].chrome.list_info.depth, 1);
    }

    #[test]
    fn shift_tab_outdents_block_after_parent_subtree() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 1, Some(1)),
            (RichBlockKind::Todo { checked: false }, 2, Some(2)),
            (RichBlockKind::BulletedList, 1, Some(1)),
        ]);
        runtime.focus_block(2);

        assert!(runtime.outdent_focused_block().unwrap());

        assert_eq!(runtime.index.block_ids, vec![1, 4, 2, 3]);
        assert_eq!(runtime.index.parent_ids[2], None);
        assert_eq!(runtime.index.depths[2], 0);
        assert_eq!(runtime.index.parent_ids[3], Some(2));
        assert_eq!(runtime.index.depths[3], 1);
        assert_eq!(runtime.focused_block_id(), Some(2));
        assert_eq!(runtime.pending_structure_transaction_count(), 1);
    }

    #[test]
    fn shift_tab_root_block_does_nothing() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 0, None),
        ]);
        runtime.focus_block(2);
        let before_version = runtime.index.structure_version;

        assert!(!runtime.outdent_focused_block().unwrap());

        assert_eq!(runtime.index.structure_version, before_version);
        assert_eq!(runtime.index.parent_ids, vec![None, None]);
        assert_eq!(runtime.pending_structure_transaction_count(), 0);
    }

    #[test]
    fn indent_outdent_preserve_subtree_children_and_queue_transactions() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::Todo { checked: false }, 1, Some(2)),
        ]);
        runtime.focus_block(2);

        assert!(runtime.indent_focused_block().unwrap());
        assert_eq!(runtime.index.parent_ids[1], Some(1));
        assert_eq!(runtime.index.depths[1], 1);
        assert_eq!(runtime.index.parent_ids[2], Some(2));
        assert_eq!(runtime.index.depths[2], 2);
        assert_eq!(runtime.pending_structure_transaction_count(), 1);

        assert!(runtime.outdent_focused_block().unwrap());
        assert_eq!(runtime.index.parent_ids[1], None);
        assert_eq!(runtime.index.depths[1], 0);
        assert_eq!(runtime.index.parent_ids[2], Some(2));
        assert_eq!(runtime.index.depths[2], 1);
        assert_eq!(runtime.pending_structure_transaction_count(), 2);
    }

    #[test]
    fn numbered_ordinal_recomputes_after_enter_indent_outdent() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::NumberedList, 0, None, "one"),
            (RichBlockKind::NumberedList, 0, None, "two"),
        ]);
        runtime.focus_block_at_offset(1, 3).unwrap();
        runtime.handle_enter().unwrap();
        assert_eq!(runtime.index.block_ids, vec![1, 3, 2]);

        let projection = runtime.projection_for_window_planned();
        assert_eq!(
            projection.blocks[0].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 1 }
        );
        assert_eq!(
            projection.blocks[1].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 2 }
        );
        assert_eq!(
            projection.blocks[2].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 3 }
        );

        runtime.focus_block(2);
        assert!(runtime.indent_focused_block().unwrap());
        let projection = runtime.projection_for_window_planned();
        assert_eq!(
            projection.blocks[0].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 1 }
        );
        assert_eq!(
            projection.blocks[1].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 2 }
        );
        assert_eq!(
            projection.blocks[2].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 1 }
        );

        assert!(runtime.outdent_focused_block().unwrap());
        let projection = runtime.projection_for_window_planned();
        assert_eq!(
            projection.blocks[0].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 1 }
        );
        assert_eq!(
            projection.blocks[1].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 2 }
        );
        assert_eq!(
            projection.blocks[2].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 3 }
        );
    }

    #[test]
    fn indent_outdent_undo_redo_restore_tree() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::Todo { checked: false }, 1, Some(2)),
        ]);
        runtime.focus_block(2);

        assert!(runtime.indent_focused_block().unwrap());
        assert_eq!(runtime.index.parent_ids[1], Some(1));
        assert_eq!(runtime.index.depths[2], 2);

        assert!(runtime.undo_focused_block().unwrap());
        assert_eq!(runtime.index.parent_ids[1], None);
        assert_eq!(runtime.index.depths[1], 0);
        assert_eq!(runtime.index.parent_ids[2], Some(2));
        assert_eq!(runtime.index.depths[2], 1);

        assert!(runtime.redo_focused_block().unwrap());
        assert_eq!(runtime.index.parent_ids[1], Some(1));
        assert_eq!(runtime.index.depths[1], 1);
        assert_eq!(runtime.index.parent_ids[2], Some(2));
        assert_eq!(runtime.index.depths[2], 2);
    }

    #[test]
    fn move_block_subtree_before_moves_children_and_preserves_total_height() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::Todo { checked: false }, 1, Some(1)),
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::NumberedList, 0, None),
        ]);
        let total_height = runtime.height_index.total_height();
        let before_version = runtime.index.structure_version;

        assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());

        assert_eq!(runtime.index.structure_version, before_version + 1);
        assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);
        assert_eq!(runtime.index.parent_ids[1], None);
        assert_eq!(runtime.index.parent_ids[2], Some(1));
        assert_eq!(runtime.index.depths[1], 0);
        assert_eq!(runtime.index.depths[2], 1);
        assert_eq!(runtime.height_index.total_height(), total_height);
        let projection = runtime.projection();
        assert_eq!(
            projection.blocks[0].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 1 }
        );
        assert_eq!(
            projection.blocks[1].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 2 }
        );
        assert_eq!(
            projection.blocks[3].chrome.prefix,
            BlockPrefixSnapshot::Number { ordinal: 3 }
        );
    }

    #[test]
    fn move_block_subtree_commit_preserves_scroll_top_and_total_height() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::Todo { checked: false }, 1, Some(1)),
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::BulletedList, 0, None),
        ]);
        runtime
            .scroll
            .scroll_to_global_offset(96.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        let before_scroll_top = runtime.scroll.global_scroll_top;
        let before_total_height = runtime.height_index.total_height();

        assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());

        assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
        assert_eq!(runtime.height_index.total_height(), before_total_height);
        assert_eq!(runtime.scroll.model_total_height, before_total_height);
        assert_eq!(runtime.scroll.displayed_total_height, before_total_height);
    }

    #[test]
    fn move_block_subtree_to_parent_reparents_and_updates_depth_delta() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::Paragraph, 0, None),
            (RichBlockKind::Todo { checked: false }, 1, Some(2)),
        ]);
        let total_height = runtime.height_index.total_height();

        assert!(runtime.move_block_subtree_to_parent(2, Some(1), 0).unwrap());

        assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
        assert_eq!(runtime.index.parent_ids[1], Some(1));
        assert_eq!(runtime.index.depths[1], 1);
        assert_eq!(runtime.index.parent_ids[2], Some(2));
        assert_eq!(runtime.index.depths[2], 2);
        assert_eq!(runtime.height_index.total_height(), total_height);
        let projection = runtime.projection();
        assert!(projection.blocks[0].chrome.has_children);
        assert_eq!(projection.blocks[1].chrome.list_info.depth, 1);
        assert_eq!(projection.blocks[2].chrome.list_info.depth, 2);
    }

    #[test]
    fn undo_and_redo_restore_structure_move_without_full_snapshot() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::Todo { checked: false }, 1, Some(1)),
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::NumberedList, 0, None),
        ]);

        assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
        assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);

        assert!(runtime.undo_focused_block().unwrap());
        assert_eq!(runtime.index.block_ids, vec![1, 2, 3, 4]);
        assert_eq!(runtime.index.parent_ids[1], Some(1));
        assert_eq!(runtime.index.depths[1], 1);

        assert!(runtime.redo_focused_block().unwrap());
        assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);
        assert_eq!(runtime.index.parent_ids[2], Some(1));
        assert_eq!(runtime.index.depths[2], 1);
    }

    #[test]
    fn structure_move_queues_persistable_transactions_for_move_undo_and_redo() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::Todo { checked: false }, 1, Some(1)),
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::NumberedList, 0, None),
        ]);

        assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
        assert_eq!(runtime.pending_structure_transaction_count(), 1);
        let txs = runtime.drain_pending_structure_transactions();
        assert_eq!(txs.len(), 1);
        assert_eq!(txs[0].kind, EditTransactionKind::BlockStructureChange);
        assert_eq!(
            txs[0].ops,
            vec![EditOperation::MoveBlockToParent {
                block_id: 1,
                parent_id: None,
                sibling_index: 1,
            }]
        );
        assert_eq!(
            txs[0].inverse_ops,
            vec![EditOperation::MoveBlockToParent {
                block_id: 1,
                parent_id: None,
                sibling_index: 0,
            }]
        );

        assert!(runtime.undo_focused_block().unwrap());
        let undo_txs = runtime.drain_pending_structure_transactions();
        assert_eq!(
            undo_txs[0].ops,
            vec![EditOperation::MoveBlockToParent {
                block_id: 1,
                parent_id: None,
                sibling_index: 0,
            }]
        );

        assert!(runtime.redo_focused_block().unwrap());
        let redo_txs = runtime.drain_pending_structure_transactions();
        assert_eq!(
            redo_txs[0].ops,
            vec![EditOperation::MoveBlockToParent {
                block_id: 1,
                parent_id: None,
                sibling_index: 1,
            }]
        );
    }

    #[test]
    fn undo_order_prefers_newer_text_edit_over_older_structure_move() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::NumberedList, 0, None),
            (RichBlockKind::Todo { checked: false }, 1, Some(1)),
            (RichBlockKind::Paragraph, 0, None),
            (RichBlockKind::NumberedList, 0, None),
        ]);

        assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
        runtime.focus_block_at_offset(3, 0).unwrap();
        runtime.insert_char('x').unwrap();
        assert_eq!(runtime.focused_text(), Some("xitem"));

        assert!(runtime.undo_focused_block().unwrap());
        assert_eq!(runtime.focused_text(), Some("item"));
        assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);

        assert!(runtime.undo_focused_block().unwrap());
        assert_eq!(runtime.index.block_ids, vec![1, 2, 3, 4]);
    }

    #[test]
    fn move_block_subtree_to_parent_rejects_invalid_parent() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::Paragraph, 0, None),
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 1, Some(2)),
        ]);

        assert!(!runtime.move_block_subtree_to_parent(2, Some(1), 0).unwrap());
        assert!(!runtime.move_block_subtree_to_parent(2, Some(3), 0).unwrap());
        assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
    }

    #[test]
    fn move_block_subtree_before_rejects_target_inside_source_subtree() {
        let mut runtime = runtime_with_kind_depths(vec![
            (RichBlockKind::BulletedList, 0, None),
            (RichBlockKind::BulletedList, 1, Some(1)),
            (RichBlockKind::BulletedList, 0, None),
        ]);

        assert!(!runtime.move_block_subtree_before(1, Some(2)).unwrap());
        assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
    }

    #[test]
    fn enter_on_empty_root_list_turns_it_into_paragraph() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::Todo { checked: false },
            0,
            None,
            "",
        )]);
        runtime.focus_block(1);

        runtime.handle_enter().unwrap();

        assert!(matches!(
            runtime.payload_window.get(1).map(|record| &record.kind),
            Some(RichBlockKind::Paragraph)
        ));
        let projection = runtime.projection();
        assert!(matches!(
            projection.blocks[0].kind,
            RichBlockKind::Paragraph
        ));
        assert_eq!(
            projection.blocks[0].chrome.prefix,
            BlockPrefixSnapshot::None
        );
    }

    #[test]
    fn enter_on_empty_nested_list_outdents_it() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::BulletedList, 0, None, "parent"),
            (RichBlockKind::BulletedList, 1, Some(1), ""),
        ]);
        runtime.focus_block(2);

        runtime.handle_enter().unwrap();

        assert!(matches!(
            runtime.payload_window.get(2).map(|record| &record.kind),
            Some(RichBlockKind::BulletedList)
        ));
        assert_eq!(runtime.index.parent_ids[1], None);
        assert_eq!(runtime.index.depths[1], 0);
        let projection = runtime.projection();
        assert_eq!(projection.blocks[1].chrome.list_info.depth, 0);
    }

    #[test]
    fn enter_on_empty_root_todo_turns_paragraph_and_clears_checkbox() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::Todo { checked: true },
            0,
            None,
            "",
        )]);
        runtime.focus_block(1);

        runtime.handle_enter().unwrap();

        assert!(matches!(
            runtime.payload_window.get(1).map(|record| &record.kind),
            Some(RichBlockKind::Paragraph)
        ));
        let projection = runtime.projection();
        assert_eq!(projection.blocks.len(), 1);
        assert_eq!(
            projection.blocks[0].chrome.prefix,
            BlockPrefixSnapshot::None
        );
    }

    #[test]
    fn enter_on_empty_nested_todo_outdents_and_preserves_todo_kind() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Todo { checked: false }, 0, None, "parent"),
            (RichBlockKind::Todo { checked: true }, 1, Some(1), ""),
        ]);
        runtime.focus_block(2);

        runtime.handle_enter().unwrap();

        assert!(matches!(
            runtime.payload_window.get(2).map(|record| &record.kind),
            Some(RichBlockKind::Todo { checked: true })
        ));
        assert_eq!(runtime.index.parent_ids[1], None);
        assert_eq!(runtime.index.depths[1], 0);
        let projection = runtime.projection();
        assert_eq!(projection.blocks[1].chrome.list_info.depth, 0);
        assert_eq!(
            projection.blocks[1].chrome.prefix,
            BlockPrefixSnapshot::Todo { checked: true }
        );
    }

    #[test]
    fn enter_on_whitespace_only_list_item_uses_trim_empty_check() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![(
            RichBlockKind::NumberedList,
            0,
            None,
            "  \n\t  ",
        )]);
        runtime.focus_block(1);

        runtime.handle_enter().unwrap();

        assert_eq!(runtime.index.total_count(), 1);
        assert!(matches!(
            runtime.payload_window.get(1).map(|record| &record.kind),
            Some(RichBlockKind::Paragraph)
        ));
    }

    #[test]
    fn enter_on_empty_list_does_not_create_block_or_move_scroll_top() {
        let mut blocks = Vec::new();
        for index in 0..50 {
            let kind = if index == 20 {
                RichBlockKind::BulletedList
            } else {
                RichBlockKind::Paragraph
            };
            let text = if index == 20 { "" } else { "item" };
            blocks.push((kind, 0, None, text));
        }
        let mut runtime = runtime_with_kind_depths_and_text(blocks);
        runtime
            .scroll
            .scroll_to_global_offset(320.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        runtime.focus_block(21);
        let before_scroll_top = runtime.scroll.global_scroll_top;
        let before_count = runtime.index.total_count();

        runtime.handle_enter().unwrap();

        assert_eq!(runtime.index.total_count(), before_count);
        assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
        assert!(matches!(
            runtime.payload_window.get(21).map(|record| &record.kind),
            Some(RichBlockKind::Paragraph)
        ));
    }

    #[test]
    fn toggle_todo_checked_updates_payload_kind_and_projection_prefix() {
        let mut document = RichTextDocument::empty(1);
        document.push_root_block(RichBlockRecord::todo(1, false, "ship it"));
        let mut runtime = DocumentRuntime::from_rich_text_document(document, 720.0);

        assert!(runtime.toggle_todo_checked(1).unwrap());

        assert!(matches!(
            runtime.payload_window.get(1).map(|record| &record.kind),
            Some(RichBlockKind::Todo { checked: true })
        ));
        let projection = runtime.projection_for_window();
        assert!(matches!(
            projection.blocks[0].kind,
            RichBlockKind::Todo { checked: true }
        ));
        assert_eq!(
            projection.blocks[0].chrome.prefix,
            BlockPrefixSnapshot::Todo { checked: true }
        );
    }

    #[test]
    fn runtime_with_100k_blocks_fixture_builds_without_large_strings() {
        let runtime = runtime_with_paragraph_blocks(100_000);

        assert_eq!(runtime.index.total_count(), 100_000);
        assert_eq!(runtime.visible_index.total_visible_count(), 100_000);
        assert_eq!(runtime.payload_window.payloads.len(), 100_000);
        assert_eq!(runtime.height_index.total_height(), 3_200_000.0);
        assert!(runtime.page_layout.page_count() >= 100);
    }

    #[test]
    fn large_mixed_demo_keeps_payloads_windowed() {
        let mut runtime = DocumentRuntime::large_mixed_demo();

        assert_eq!(
            runtime.index.total_count(),
            crate::runtime::LARGE_MIXED_DEMO_BLOCKS
        );
        assert!(runtime.payload_window.payloads.len() < 2_000);
        assert!(runtime.payload_window.block_range.start == 0);

        runtime
            .scroll
            .scroll_to_global_offset(1_000_000.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        let projection = runtime.projection_for_window_planned();

        assert!(!projection.blocks.is_empty());
        assert!(projection.blocks.len() <= 320);
        assert!(runtime.payload_window.payloads.len() < 5_000);
        assert!(runtime.payload_window.block_range.start > 0);
    }

    #[test]
    fn target_for_global_offset_maps_100k_document_precisely() {
        let runtime = runtime_with_paragraph_blocks(100_000);
        let samples = [0.0, 1.0, 31.9, 32.0, 50_000.0, 3_199_999.0];

        for global_y in samples {
            let target = runtime.target_for_global_offset(global_y).unwrap();
            assert_eq!(
                target.block_index,
                (target.global_scroll_top / 32.0).floor().min(99_999.0) as usize
            );
            assert_eq!(target.block_id, target.block_index as BlockId + 1);
            assert!(target.block_top <= target.global_scroll_top + f64::EPSILON);
            assert!(target.offset_in_block >= 0.0);
            assert!(target.offset_in_block <= 32.0);
            assert_eq!(
                runtime.page_layout.page_for_block_index(target.block_index),
                Some(target.page_index)
            );
        }
    }

    #[test]
    fn planned_window_hysteresis_keeps_boundary_window_stable() {
        let mut runtime = runtime_with_paragraph_blocks(3_000);
        runtime.window_planner = WindowPlanner::new(
            0,
            0,
            WindowPlannerPolicy {
                enter_threshold_viewports: 0.5,
                min_stable_frames_before_trim: 0,
                min_ms_between_window_commits: 0,
                ..WindowPlannerPolicy::default()
            },
        );
        let first_page_height = runtime.page_layout.pages[0].height;
        runtime
            .scroll
            .scroll_to_global_offset(
                first_page_height - 10.0,
                crate::editor::scroll::ScrollOrigin::UserWheel,
            )
            .unwrap();
        let initial = runtime.current_page_window_planned();
        runtime
            .scroll
            .scroll_to_global_offset(
                first_page_height + 10.0,
                crate::editor::scroll::ScrollOrigin::UserWheel,
            )
            .unwrap();
        let near_boundary = runtime.current_page_window_planned();

        assert_eq!(near_boundary, initial);
    }

    #[test]
    fn planned_window_keeps_focused_page_pinned() {
        let mut runtime = runtime_with_paragraph_blocks(10_000);
        runtime.window_planner = WindowPlanner::new(0, 0, WindowPlannerPolicy::default());
        runtime.focus_block(1);
        let target_page = runtime.page_layout.page_count() - 1;
        let offset = runtime.page_layout.offset_of_page(target_page).unwrap();
        runtime
            .scroll
            .scroll_to_global_offset(offset, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();

        let planned = runtime.current_page_window_planned();
        let focused_page = runtime.page_layout.page_for_block_index(0).unwrap();
        assert!(planned.contains(&focused_page));
        assert!(planned.contains(&target_page));
    }

    #[test]
    fn document_runtime_projects_v2_blocks_without_ui_truth() {
        let runtime = DocumentRuntime::demo();
        let projection = runtime.projection();
        assert_eq!(projection.total_visible_blocks, 4);
        assert_eq!(projection.blocks.len(), 4);
        assert_eq!(projection.blocks[0].block_id, 1);
        assert!(matches!(
            projection.blocks[0].kind,
            RichBlockKind::Heading { level: 1 }
        ));
    }

    #[test]
    fn projection_for_window_exposes_total_visible_count_and_spacers() {
        let runtime = DocumentRuntime::demo();

        let projection = runtime.projection_for_window();

        assert_eq!(
            projection.total_visible_blocks,
            runtime.visible_index.total_visible_count()
        );
        assert_eq!(projection.before_window_height, 0.0);
        assert_eq!(projection.placeholder_window_height, None);
        assert_eq!(projection.after_window_height, 0.0);
    }

    #[test]
    fn scrollbar_drag_uses_runtime_frozen_projection_instead_of_placeholder() {
        let records = (1..=1_000 as BlockId)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
                .with_layout_meta(crate::core::layout::BlockLayoutMeta::new(block_id, 32.0))
            })
            .collect::<Vec<_>>();
        let payloads = (1..=1_000 as BlockId)
            .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
            .collect::<Vec<_>>();
        let mut runtime = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
        let loaded = runtime.projection_for_window_planned();
        assert!(!loaded.render_window.is_placeholder());
        runtime.payload_window.block_range = 0..64;
        runtime
            .payload_window
            .payloads
            .retain(|block_id, _| *block_id <= 64);

        runtime
            .scroll
            .scroll_to_global_offset(20_000.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        let policy = ScrollbarPolicy::default();
        runtime.begin_scrollbar_drag(policy);

        let frozen = runtime.projection_for_window_planned();

        assert!(!frozen.render_window.is_placeholder());
        assert_eq!(frozen.placeholder_window_height, None);
        assert!(!frozen.blocks.is_empty());
        assert_eq!(frozen.blocks[0].block_id, loaded.blocks[0].block_id);
        assert_eq!(
            frozen.render_window.block_range,
            loaded.render_window.block_range
        );
    }

    #[test]
    fn projection_uses_placeholder_window_when_payload_window_is_not_loaded() {
        let records = (1..=1_000 as BlockId)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
                .with_layout_meta(crate::core::layout::BlockLayoutMeta::new(block_id, 32.0))
            })
            .collect::<Vec<_>>();
        let runtime =
            DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);

        let projection = runtime.projection_for_window();

        assert!(projection.render_window.is_placeholder());
        assert!(projection.blocks.is_empty());
        assert_eq!(
            projection.placeholder_window_height,
            Some(projection.render_window.height())
        );
        assert_eq!(
            projection.before_window_height
                + projection.placeholder_window_height.unwrap_or_default()
                + projection.after_window_height,
            runtime.page_layout.total_height()
        );
    }

    #[test]
    fn focus_block_at_offset_sets_caret_without_ui_truth() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );

        runtime.focus_block_at_offset(1, 2).unwrap();

        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.caret_offset_for_block(1), Some(2));
        let projection = runtime.projection_for_window();
        assert_eq!(projection.blocks[0].caret_offset, Some(2));
    }

    #[test]
    fn insert_char_uses_caret_offset() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        runtime.set_caret_offset(1, 2).unwrap();

        runtime.insert_char('X').unwrap();

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "abXcd");
        assert_eq!(projection.blocks[0].caret_offset, Some(3));
    }

    #[test]
    fn composition_preview_does_not_commit_until_commit() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "ab",
            )],
            720.0,
        );
        runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

        assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");
        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "a中b");
        assert_eq!(projection.blocks[0].marked_range, Some(1.."a中".len()));
        assert_eq!(projection.blocks[0].caret_offset, Some("a中".len()));
        assert_eq!(runtime.composition_preview_text().as_deref(), Some("a中b"));
        assert_eq!(runtime.focused_text_for_platform_input().unwrap().1, "a中b");
        assert_eq!(
            runtime.active_composition_marked_range(),
            Some(1.."a中".len())
        );
        assert_eq!(
            runtime
                .editing
                .as_ref()
                .unwrap()
                .composition
                .as_ref()
                .unwrap()
                .preview_text,
            "中"
        );

        assert!(runtime.commit_composition().unwrap());
        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "a中b");
        assert!(runtime.editing.as_ref().unwrap().composition.is_none());
    }

    #[test]
    fn replace_text_in_focused_range_commits_text_and_clears_composition() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();
        runtime.begin_or_update_composition(1, 1..3, "中").unwrap();

        assert!(runtime.replace_text_in_focused_range(None, "字").unwrap());

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "a字d");
        assert!(runtime.active_composition().is_none());
    }

    #[test]
    fn replace_text_space_path_applies_block_markdown_shortcut() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "#",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 1).unwrap();

        assert!(runtime.replace_text_in_focused_range(None, " ").unwrap());

        let projection = runtime.projection_for_window();
        assert!(matches!(
            projection.blocks[0].kind,
            RichBlockKind::Heading { level: 1 }
        ));
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "");
    }

    #[test]
    fn bold_markdown_shortcut_creates_bold_not_italic() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "**abc**",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, "**abc**".len()).unwrap();

        assert!(runtime.apply_inline_markdown_shortcut(1).unwrap());

        let payload = runtime.payload_window.get(1).unwrap();
        let BlockPayload::RichText { spans } = &payload.payload else {
            panic!("expected rich text payload");
        };
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "abc");
        assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
    }

    #[test]
    fn inserting_inside_bold_span_preserves_bold_mark() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Paragraph,
                payload: BlockPayload::RichText {
                    spans: vec![InlineSpan {
                        text: "ab".to_owned(),
                        marks: vec![InlineMark::Bold],
                    }],
                },
            }],
            720.0,
        );
        runtime.focus_block_at_offset(1, 1).unwrap();

        runtime.insert_char('X').unwrap();

        let payload = runtime.payload_window.get(1).unwrap();
        let BlockPayload::RichText { spans } = &payload.payload else {
            panic!("expected rich text payload");
        };
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "aXb");
        assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
    }

    #[test]
    fn deleting_inside_bold_span_preserves_remaining_bold_mark() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Paragraph,
                payload: BlockPayload::RichText {
                    spans: vec![InlineSpan {
                        text: "abc".to_owned(),
                        marks: vec![InlineMark::Bold],
                    }],
                },
            }],
            720.0,
        );
        runtime.focus_block_at_offset(1, "abc".len()).unwrap();

        assert!(runtime.delete_backward().unwrap());

        let payload = runtime.payload_window.get(1).unwrap();
        let BlockPayload::RichText { spans } = &payload.payload else {
            panic!("expected rich text payload");
        };
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "ab");
        assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
    }

    #[test]
    fn replace_text_path_applies_inline_markdown_shortcut() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "**bold**",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, "**bold**".len()).unwrap();

        assert!(runtime.replace_text_in_focused_range(None, "!").unwrap());

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        let BlockPayload::RichText { spans } = &payload.payload else {
            panic!("payload should be rich text");
        };
        assert_eq!(payload.plain_text(), "bold!");
        assert!(spans.iter().any(|span| {
            span.text == "bold"
                && span
                    .marks
                    .contains(&crate::core::rich_text::InlineMark::Bold)
        }));
    }

    #[test]
    fn move_focused_caret_to_offset_updates_caret_without_selection() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcdef",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();

        assert!(runtime.move_focused_caret_to_offset(1, 5, false).unwrap());

        let projection = runtime.projection_for_window();
        assert_eq!(projection.blocks[0].caret_offset, Some(5));
        assert_eq!(runtime.focused_text_selection_range(), None);
    }

    #[test]
    fn move_focused_caret_to_offset_extends_same_block_selection() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcdef",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();

        assert!(runtime.move_focused_caret_to_offset(1, 5, true).unwrap());

        let projection = runtime.projection_for_window();
        assert_eq!(projection.blocks[0].caret_offset, Some(5));
        assert_eq!(runtime.focused_text_selection_range(), Some(2..5));
        assert_eq!(runtime.selected_focused_text().as_deref(), Some("cde"));
    }

    #[test]
    fn insert_char_uses_middle_caret_offset() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();

        runtime.insert_char('X').unwrap();

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "abXcd");
        assert_eq!(projection.blocks[0].caret_offset, Some(3));
    }

    #[test]
    fn replace_text_in_focused_range_can_insert_in_middle_without_selection() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();

        assert!(runtime.replace_text_in_focused_range(None, "中").unwrap());

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "ab中cd");
        assert_eq!(projection.blocks[0].caret_offset, Some("ab中".len()));
    }

    #[test]
    fn replace_text_in_focused_range_inserts_string_at_middle_caret() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcdef",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 3).unwrap();

        assert!(runtime.replace_text_in_focused_range(None, "XYZ").unwrap());

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "abcXYZdef");
        assert_eq!(projection.blocks[0].caret_offset, Some("abcXYZ".len()));
    }

    #[test]
    fn replace_text_in_focused_range_replaces_selection_and_caret_after_inserted_text() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcdef",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();
        runtime.set_document_text_selection(1, 2, 1, 4).unwrap();

        assert!(runtime.replace_text_in_focused_range(None, "XYZ").unwrap());

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "abXYZef");
        assert_eq!(projection.blocks[0].caret_offset, Some("abXYZ".len()));
        assert_eq!(runtime.focused_text_selection_range(), None);
    }

    #[test]
    fn ime_preview_and_commit_can_start_in_middle_of_text() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();

        runtime.begin_or_update_composition(1, 2..2, "你").unwrap();
        assert_eq!(
            runtime.composition_preview_text().as_deref(),
            Some("ab你cd")
        );
        assert_eq!(
            runtime.active_composition_marked_range(),
            Some(2.."ab你".len())
        );
        assert!(runtime.commit_composition().unwrap());

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "ab你cd");
        assert_eq!(projection.blocks[0].caret_offset, Some("ab你".len()));
    }

    #[test]
    fn replace_text_prioritizes_active_composition_over_stale_selection() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcdef",
            )],
            720.0,
        );
        runtime.set_document_text_selection(1, 1, 1, 5).unwrap();
        runtime
            .begin_or_update_composition_with_selection(1, 3..3, "中", None)
            .unwrap();

        assert!(runtime.replace_text_in_focused_range(None, "文").unwrap());

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "abc文def");
        assert_eq!(projection.blocks[0].caret_offset, Some("abc文".len()));
    }

    #[test]
    fn ime_preview_tracks_selected_subrange_inside_marked_text() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "abcd",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 2).unwrap();

        runtime
            .begin_or_update_composition_with_selection(
                1,
                2..2,
                "你好",
                Some("你".len().."你好".len()),
            )
            .unwrap();

        assert_eq!(
            runtime.composition_preview_text().as_deref(),
            Some("ab你好cd")
        );
        assert_eq!(
            runtime.active_composition_marked_range(),
            Some(2.."ab你好".len())
        );
        assert_eq!(
            runtime.active_composition_selected_range(),
            Some("ab你".len().."ab你好".len())
        );
        assert_eq!(runtime.caret_offset_for_block(1), Some("ab你好".len()));
    }

    #[test]
    fn backspace_at_start_merges_non_empty_paragraph_into_previous() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Paragraph, 0, None, "hello "),
            (RichBlockKind::Paragraph, 0, None, "world"),
        ]);
        runtime.focus_block_at_offset(2, 0).unwrap();
        let before_scroll_top = runtime.scroll.global_scroll_top;

        assert!(runtime.delete_backward().unwrap());

        assert_eq!(runtime.index.total_count(), 1);
        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.focused_text(), Some("hello world"));
        assert_eq!(runtime.selected_focused_text(), Some("world".to_owned()));
        assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    }

    #[test]
    fn list_item_backspace_first_resets_then_second_merges() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Paragraph, 0, None, "a"),
            (RichBlockKind::BulletedList, 0, None, "b"),
        ]);
        runtime.focus_block_at_offset(2, 0).unwrap();

        assert!(runtime.delete_backward().unwrap());
        assert!(matches!(
            runtime.kind_for_block(2),
            RichBlockKind::Paragraph
        ));
        assert_eq!(runtime.index.total_count(), 2);

        assert!(runtime.delete_backward().unwrap());
        assert_eq!(runtime.index.total_count(), 1);
        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.focused_text(), Some("ab"));
    }

    #[test]
    fn empty_block_backspace_and_delete_remove_block_and_focus_adjacent() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Paragraph, 0, None, "a"),
            (RichBlockKind::Paragraph, 0, None, ""),
            (RichBlockKind::Paragraph, 0, None, "c"),
        ]);
        runtime.focus_block_at_offset(2, 0).unwrap();

        assert!(runtime.delete_backward().unwrap());
        assert_eq!(runtime.index.total_count(), 2);
        assert_eq!(runtime.focused_block_id(), Some(1));

        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Paragraph, 0, None, "a"),
            (RichBlockKind::Paragraph, 0, None, ""),
            (RichBlockKind::Paragraph, 0, None, "c"),
        ]);
        runtime.focus_block_at_offset(2, 0).unwrap();

        assert!(runtime.delete_forward().unwrap());
        assert_eq!(runtime.index.total_count(), 2);
        assert_eq!(runtime.focused_block_id(), Some(3));
    }

    #[test]
    fn last_empty_block_is_not_deleted() {
        let mut runtime =
            runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "")]);
        runtime.focus_block_at_offset(1, 0).unwrap();

        assert!(!runtime.delete_backward().unwrap());
        assert_eq!(runtime.index.total_count(), 1);
        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.focused_text(), Some(""));
    }

    #[test]
    fn delete_at_end_merges_next_block_into_current() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Paragraph, 0, None, "a"),
            (RichBlockKind::Paragraph, 0, None, "b"),
        ]);
        runtime.focus_block_at_offset(1, 1).unwrap();

        assert!(runtime.delete_forward().unwrap());
        assert_eq!(runtime.index.total_count(), 1);
        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.focused_text(), Some("ab"));
    }

    #[test]
    fn arrow_keys_cross_block_boundaries_and_shift_extends_selection() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Paragraph, 0, None, "ab"),
            (RichBlockKind::Paragraph, 0, None, "cd"),
        ]);
        runtime.focus_block_at_offset(2, 0).unwrap();

        assert!(runtime.move_caret_left(false).unwrap());
        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.caret_offset_for_block(1), Some(2));

        assert!(runtime.move_caret_right(false).unwrap());
        assert_eq!(runtime.focused_block_id(), Some(2));
        assert_eq!(runtime.caret_offset_for_block(2), Some(0));

        runtime.focus_block_at_offset(1, 2).unwrap();
        assert!(runtime.move_caret_right(true).unwrap());
        assert!(runtime.has_cross_block_text_selection());
    }

    #[test]
    fn delete_document_selection_collapses_cross_block_range() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Paragraph, 0, None, "ab"),
            (RichBlockKind::Paragraph, 0, None, "middle"),
            (RichBlockKind::Paragraph, 0, None, "cd"),
        ]);
        runtime
            .set_document_text_selection(1, 1, 3, 1)
            .expect("selection spans loaded blocks");

        assert!(runtime.delete_document_selection().unwrap());

        assert_eq!(runtime.index.total_count(), 1);
        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.focused_text(), Some("ad"));
        assert_eq!(runtime.caret_offset_for_block(1), Some(1));
    }

    #[test]
    fn up_down_fallback_focuses_adjacent_visible_blocks() {
        let mut runtime = runtime_with_kind_depths_and_text(vec![
            (RichBlockKind::Paragraph, 0, None, "a"),
            (RichBlockKind::Paragraph, 0, None, "b"),
            (RichBlockKind::Paragraph, 0, None, "c"),
        ]);
        runtime.focus_block_at_offset(2, 1).unwrap();

        assert!(runtime.move_caret_up(false).unwrap());
        assert_eq!(runtime.focused_block_id(), Some(1));
        assert_eq!(runtime.caret_offset_for_block(1), Some(1));

        assert!(runtime.move_caret_down(false).unwrap());
        assert_eq!(runtime.focused_block_id(), Some(2));
        assert_eq!(runtime.caret_offset_for_block(2), Some(0));
    }

    #[test]
    fn delete_backward_uses_caret_offset() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "a👨‍👩‍👧‍👦b",
            )],
            720.0,
        );
        let caret_after_emoji = "a👨‍👩‍👧‍👦".len();
        runtime.set_caret_offset(1, caret_after_emoji).unwrap();

        assert!(runtime.delete_backward().unwrap());

        let projection = runtime.projection_for_window();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "ab");
        assert_eq!(projection.blocks[0].caret_offset, Some(1));
    }

    #[test]
    fn backspace_at_start_resets_textual_block_styles_to_paragraph() {
        let kinds = [
            RichBlockKind::Heading { level: 1 },
            RichBlockKind::Quote,
            RichBlockKind::Callout {
                variant: crate::core::rich_text::CalloutVariant::Note,
            },
            RichBlockKind::Todo { checked: true },
            RichBlockKind::BulletedList,
            RichBlockKind::NumberedList,
            RichBlockKind::Toggle,
            RichBlockKind::Math,
            RichBlockKind::Mermaid,
            RichBlockKind::FootnoteDefinition,
            RichBlockKind::Comment,
            RichBlockKind::RawMarkdown,
            RichBlockKind::Custom("legacy-text".to_owned()),
        ];

        for kind in kinds {
            let mut runtime = DocumentRuntime::from_payloads(
                1,
                vec![BlockPayloadRecord::rich_text(1, kind.clone(), "keep text")],
                720.0,
            );
            runtime.focus_block_at_offset(1, 0).unwrap();

            assert!(runtime.delete_backward().unwrap(), "{kind:?} should reset");

            let projection = runtime.projection_for_window();
            assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
            let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
                panic!("payload should be loaded");
            };
            assert_eq!(payload.plain_text(), "keep text");
            assert_eq!(projection.blocks[0].caret_offset, Some(0));
        }
    }

    #[test]
    fn backspace_at_start_resets_code_and_html_payloads_to_paragraph_without_losing_text() {
        let cases = [
            BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                payload: BlockPayload::Code {
                    language: Some("rust".to_owned()),
                    text: "fn main() {}".to_owned(),
                },
            },
            BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Html,
                payload: BlockPayload::Html {
                    html: "<b>hello</b>".to_owned(),
                    sanitized: true,
                },
            },
        ];

        for record in cases {
            let expected_text = record.plain_text();
            let mut runtime = DocumentRuntime::from_payloads(1, vec![record], 720.0);
            runtime.focus_block_at_offset(1, 0).unwrap();

            assert!(runtime.delete_backward().unwrap());

            let projection = runtime.projection_for_window();
            assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
            let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
                panic!("payload should be loaded");
            };
            assert_eq!(payload.plain_text(), expected_text);
            assert_eq!(projection.blocks[0].caret_offset, Some(0));
        }
    }

    #[test]
    fn backspace_at_start_keeps_plain_paragraph_unchanged() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "plain",
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, 0).unwrap();

        assert!(!runtime.delete_backward().unwrap());

        let projection = runtime.projection_for_window();
        assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "plain");
        assert_eq!(projection.blocks[0].caret_offset, Some(0));
    }

    #[test]
    fn measured_height_above_viewport_restores_viewport_top_anchor() {
        let mut runtime = runtime_with_paragraph_blocks(1_000);
        runtime
            .scroll
            .scroll_to_global_offset(3_200.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        let before = runtime.scroll.global_scroll_top;

        assert!(runtime.apply_measured_height(1, 1, 64.0).unwrap());

        assert_eq!(before, 3_200.0);
        assert_eq!(runtime.scroll.global_scroll_top, before + 32.0);
    }

    #[test]
    fn measured_height_below_viewport_does_not_move_scroll_top() {
        let mut runtime = runtime_with_paragraph_blocks(1_000);
        runtime
            .scroll
            .scroll_to_global_offset(3_200.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        let before = runtime.scroll.global_scroll_top;

        assert!(runtime.apply_measured_height(900, 1, 64.0).unwrap());

        assert_eq!(runtime.scroll.global_scroll_top, before);
    }

    #[test]
    fn measured_height_rejects_stale_content_version() {
        let mut runtime = DocumentRuntime::demo();
        runtime.focus_block(3);
        runtime.insert_char('!').unwrap();

        let applied = runtime.apply_measured_height(3, 1, 96.0).unwrap();

        assert!(!applied);
        let block_index = runtime.index.index_of(3).unwrap();
        assert_ne!(
            runtime.index.layout_meta[block_index].measured_height,
            Some(96.0)
        );
    }

    #[test]
    fn editing_code_block_preserves_code_payload() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Code {
                    language: Some("rust".to_owned()),
                },
                payload: BlockPayload::Code {
                    language: Some("rust".to_owned()),
                    text: "fn main()".to_owned(),
                },
            }],
            720.0,
        );

        runtime.focus_block_at_offset(1, 2).unwrap();
        runtime.insert_char('x').unwrap();

        let payload = runtime.payload_window.get(1).unwrap();
        match &payload.payload {
            BlockPayload::Code { language, text } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert_eq!(text, "fnx main()");
            }
            _ => panic!("expected code payload after editing code block"),
        }
        assert_eq!(runtime.caret_offset_for_block(1), Some(3));
    }

    #[test]
    fn code_block_estimate_includes_language_label_and_chrome() {
        let payload = BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {\n    let value = 1;\n    value + 1\n}".to_owned(),
            },
        };

        assert!(estimate_payload_height(&payload, 0) >= RichBlockRecord::DEFAULT_CODE_HEIGHT);
        assert!(estimate_payload_height(&payload, 0) >= 130.0);
    }

    #[test]
    fn enter_in_quote_soft_wraps_and_grows_block_height() {
        let records = vec![
            BlockIndexRecord::new(
                1,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Quote),
                0,
            )
            .with_layout_meta(crate::core::layout::BlockLayoutMeta::new(1, 36.0)),
        ];
        let payloads = vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Quote,
            "> 引用块: UI 只是投影，runtime 才是真相。",
        )];
        let mut runtime = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
        runtime.focus_block(1);
        let before = runtime
            .index
            .index_of(1)
            .map(|index| runtime.index.layout_meta[index].effective_height())
            .unwrap();

        runtime.handle_enter().unwrap();

        assert!(runtime.focused_text().unwrap().contains('\n'));
        let after = runtime
            .index
            .index_of(1)
            .map(|index| runtime.index.layout_meta[index].effective_height())
            .unwrap();
        assert!(
            after > before,
            "quote height should grow: {before} -> {after}"
        );
    }

    #[test]
    fn document_text_selection_projects_partial_and_full_ranges() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "abcd"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "efgh"),
                BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "ijkl"),
            ],
            720.0,
        );

        runtime.set_document_text_selection(1, 2, 3, 1).unwrap();
        let projection = runtime.projection_for_window();

        assert_eq!(
            projection.blocks[0].selection_range,
            Some(SelectionRange::Partial(2..4))
        );
        assert_eq!(
            projection.blocks[1].selection_range,
            Some(SelectionRange::Full)
        );
        assert_eq!(
            projection.blocks[2].selection_range,
            Some(SelectionRange::Partial(0..1))
        );
        assert_eq!(
            runtime.selected_document_text().as_deref(),
            Some("cd\nefgh\ni")
        );
    }

    #[test]
    fn focused_text_selection_replaces_and_moves_with_shift_arrows() {
        let mut runtime = DocumentRuntime::demo();
        runtime.focus_block_at_offset(3, 0).unwrap();

        assert!(runtime.move_caret_right(true).unwrap());
        let selected = runtime.selected_focused_text().unwrap();
        assert_eq!(selected.chars().count(), 1);

        runtime.insert_char('你').unwrap();
        assert!(runtime.focused_text_selection_range().is_none());
        assert!(runtime.focused_text().unwrap().starts_with('你'));
    }

    #[test]
    fn select_all_copy_cut_paste_and_inline_mark_work_on_focused_text() {
        let mut runtime = DocumentRuntime::demo();
        runtime.focus_block_at_offset(3, 0).unwrap();
        assert!(runtime.select_focused_text_all());
        let selected = runtime.selected_focused_text().unwrap();
        assert!(!selected.is_empty());

        assert!(
            runtime
                .toggle_inline_mark_on_selection(InlineMark::Bold)
                .unwrap()
        );
        let payload = runtime.payload_window.get(3).unwrap();
        match &payload.payload {
            BlockPayload::RichText { spans } => {
                assert!(
                    spans
                        .iter()
                        .any(|span| span.marks.contains(&InlineMark::Bold))
                );
            }
            _ => panic!("expected rich text payload"),
        }

        assert!(
            runtime
                .replace_text_in_focused_range(None, "粘贴文本")
                .unwrap()
        );
        assert_eq!(runtime.focused_text(), Some("粘贴文本"));
    }

    #[test]
    fn queued_measured_heights_do_not_apply_until_flush() {
        let mut runtime = runtime_with_paragraph_blocks(1_000);
        runtime
            .scroll
            .scroll_to_global_offset(3_200.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        let before_scroll_top = runtime.scroll.global_scroll_top;
        let before_total_height = runtime.height_index.total_height();

        assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());

        assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
        assert_eq!(runtime.height_index.total_height(), before_total_height);

        assert!(runtime.flush_pending_height_corrections().unwrap());
        assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top + 32.0);
        assert_eq!(
            runtime.height_index.total_height(),
            before_total_height + 32.0
        );
    }

    #[test]
    fn flush_measured_heights_restores_anchor_once_for_batched_changes() {
        let mut runtime = runtime_with_paragraph_blocks(1_000);
        runtime
            .scroll
            .scroll_to_global_offset(3_200.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        let before = runtime.scroll.global_scroll_top;

        assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
        assert!(runtime.queue_measured_height(2, 1, 72.0).unwrap());
        assert!(runtime.queue_measured_height(3, 1, 80.0).unwrap());
        assert!(runtime.flush_pending_height_corrections().unwrap());

        assert_eq!(
            runtime.scroll.global_scroll_top,
            before + 32.0 + 40.0 + 48.0
        );
    }

    #[test]
    fn flush_discards_stale_measured_height_versions() {
        let mut runtime = DocumentRuntime::demo();
        runtime.focus_block(3);

        assert!(runtime.queue_measured_height(3, 1, 96.0).unwrap());
        runtime.insert_char('!').unwrap();

        assert!(!runtime.flush_pending_height_corrections().unwrap());
        let block_index = runtime.index.index_of(3).unwrap();
        assert_ne!(
            runtime.index.layout_meta[block_index].measured_height,
            Some(96.0)
        );
    }

    #[test]
    fn flush_below_viewport_heights_does_not_move_scroll_top() {
        let mut runtime = runtime_with_paragraph_blocks(1_000);
        runtime
            .scroll
            .scroll_to_global_offset(3_200.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        let before = runtime.scroll.global_scroll_top;

        assert!(runtime.queue_measured_height(900, 1, 64.0).unwrap());
        assert!(runtime.queue_measured_height(901, 1, 72.0).unwrap());
        assert!(runtime.flush_pending_height_corrections().unwrap());

        assert_eq!(runtime.scroll.global_scroll_top, before);
    }

    #[test]
    fn wheel_scroll_height_flush_preserves_user_scroll_top_without_bounce() {
        let mut runtime = runtime_with_paragraph_blocks(1_000);
        runtime
            .scroll
            .scroll_to_global_offset(3_200.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        runtime.scroll_by_delta(-64.0).unwrap();
        let before_scroll_top = runtime.scroll.global_scroll_top;
        let before_total_height = runtime.height_index.total_height();

        assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
        assert!(
            runtime
                .flush_pending_height_corrections_with_priority(
                    HeightCorrectionPriority::DeferRemote
                )
                .unwrap()
        );

        assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
        assert_eq!(
            runtime.height_index.total_height(),
            before_total_height + 32.0
        );
        assert_eq!(
            runtime.scroll.model_total_height,
            before_total_height + 32.0
        );
        assert!(runtime.pending_measured_heights.is_empty());
    }

    #[test]
    fn scrollbar_drag_freezes_displayed_total_and_defers_anchor_restore() {
        let mut runtime = runtime_with_paragraph_blocks(1_000);
        runtime
            .scroll
            .scroll_to_global_offset(3_200.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();
        let policy = ScrollbarPolicy {
            track_height: 720.0,
            ..ScrollbarPolicy::default()
        };
        let before_scroll_top = runtime.scroll.global_scroll_top;
        let before_total_height = runtime.scroll.displayed_total_height;

        let visual = runtime.begin_scrollbar_drag(policy);
        assert!(visual.enabled);
        assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
        assert!(runtime.flush_pending_height_corrections().unwrap());

        assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
        assert_eq!(
            runtime.scroll.model_total_height,
            before_total_height + 32.0
        );
        assert_eq!(runtime.scroll.displayed_total_height, before_total_height);

        let end = runtime.finish_scrollbar_drag().unwrap().unwrap();
        assert_eq!(end.pending_layout_corrections, 1);
        assert_eq!(
            runtime.scroll.displayed_total_height,
            runtime.scroll.model_total_height
        );
    }

    #[test]
    fn scrollbar_drag_uses_frozen_total_height_for_thumb_mapping() {
        let mut runtime = runtime_with_paragraph_blocks(1_000);
        let policy = ScrollbarPolicy {
            track_height: 720.0,
            ..ScrollbarPolicy::default()
        };
        let visual = runtime.begin_scrollbar_drag(policy);
        let max_thumb_top = policy.track_height - visual.thumb_height;

        let update = runtime
            .drag_scrollbar_to_thumb_top(policy, max_thumb_top)
            .unwrap()
            .unwrap();

        assert_eq!(update.drag_ratio, 1.0);
        assert_eq!(
            runtime.scroll.global_scroll_top,
            runtime.scroll.max_scroll_top()
        );
        assert!(runtime.finish_scrollbar_drag().unwrap().is_some());
    }

    #[test]
    fn rich_text_height_updates_after_wrap() {
        let long_text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                long_text,
            )],
            720.0,
        );
        let snapshot = runtime.projection_for_window().blocks[0].clone();
        let input =
            crate::gui::text::RichTextLayoutInput::from_snapshot(&snapshot, 80.0, 1, 1).unwrap();
        let layout = crate::gui::text::wrap_rich_text(&input);
        assert!(layout.height > RichBlockRecord::DEFAULT_TEXT_HEIGHT);

        let applied = runtime
            .apply_measured_height(snapshot.block_id, input.content_version, layout.height)
            .unwrap();

        assert!(applied);
        let updated = runtime.projection_for_window();
        assert_eq!(
            updated.blocks[0].layout.measured_height,
            Some(layout.height)
        );
        assert_eq!(runtime.height_index.total_height(), layout.height);
        assert_eq!(runtime.page_layout.total_height(), layout.height);
    }

    #[test]
    fn document_runtime_scroll_by_delta_clamps_and_updates_page_window() {
        let mut runtime = runtime_with_paragraph_blocks(100_000);
        let initial_window = runtime.current_page_window();

        runtime.scroll_by_delta(50_000.0).unwrap();

        assert_eq!(runtime.scroll.global_scroll_top, 50_000.0);
        assert_ne!(runtime.current_page_window(), initial_window);

        runtime.scroll_by_delta(-100_000.0).unwrap();
        assert_eq!(runtime.scroll.global_scroll_top, 0.0);

        runtime.scroll_by_delta(10_000_000.0).unwrap();
        assert_eq!(
            runtime.scroll.global_scroll_top,
            runtime.scroll.max_scroll_top()
        );
    }

    #[test]
    fn projection_window_spacer_heights_sum_to_total() {
        let mut runtime = runtime_with_paragraph_blocks(100_000);
        let middle_page = runtime.page_layout.page_count() / 2;
        let middle_offset = runtime.page_layout.offset_of_page(middle_page).unwrap();
        runtime
            .scroll
            .scroll_to_global_offset(
                middle_offset,
                crate::editor::scroll::ScrollOrigin::ProgrammaticVirtualScroll,
            )
            .unwrap();

        let projection = runtime.projection_for_window();
        let projected_total = projection.before_window_height
            + projection.render_window.height()
            + projection.after_window_height;

        assert!(projection.before_window_height > 0.0);
        assert!((projected_total - runtime.page_layout.total_height()).abs() < 0.001);
    }

    #[test]
    fn projection_for_window_limits_blocks_for_100k_document() {
        let runtime = runtime_with_paragraph_blocks(100_000);

        let projection = runtime.projection_for_window();

        assert_eq!(projection.total_visible_blocks, 100_000);
        assert!(projection.blocks.len() < 10_000);
        assert_eq!(
            projection.render_window.block_range.len(),
            projection.blocks.len()
        );
        assert_eq!(
            projection.render_window.page_range,
            runtime.current_page_window()
        );
        assert_eq!(projection.blocks.first().unwrap().visible_index, 0);
        assert_eq!(
            projection.blocks.last().unwrap().visible_index + 1,
            projection.render_window.block_range.end
        );
    }

    #[test]
    fn current_page_window_clamps_first_middle_and_last_pages() {
        let mut runtime = runtime_with_paragraph_blocks(3_000);
        let page_count = runtime.page_layout.page_count();
        assert!(page_count >= 4);

        assert_eq!(runtime.current_page_window().start, 0);
        assert!(runtime.current_page_window().contains(&0));

        let middle_page = page_count / 2;
        let middle_offset = runtime.page_layout.offset_of_page(middle_page).unwrap();
        runtime
            .scroll
            .scroll_to_global_offset(
                middle_offset,
                crate::editor::scroll::ScrollOrigin::ProgrammaticVirtualScroll,
            )
            .unwrap();
        let middle_window = runtime.current_page_window();
        assert!(middle_window.contains(&middle_page));
        assert_eq!(middle_window.start, middle_page.saturating_sub(1));
        assert!(middle_window.end <= page_count);

        runtime
            .scroll
            .scroll_to_global_offset(
                runtime.scroll.model_total_height,
                crate::editor::scroll::ScrollOrigin::ProgrammaticVirtualScroll,
            )
            .unwrap();
        let last_window = runtime.current_page_window();
        assert!(last_window.contains(&(page_count - 1)));
        assert_eq!(last_window.end, page_count);
    }

    #[test]
    fn document_runtime_insert_char_updates_payload_and_pins_editing_block() {
        let mut runtime = DocumentRuntime::demo();
        runtime.focus_block(3);
        runtime.insert_char('!').unwrap();
        let projection = runtime.projection();
        let block = projection
            .blocks
            .iter()
            .find(|block| block.block_id == 3)
            .unwrap();
        assert!(block.focused);
        assert!(block.pinned);
        let BlockPayloadView::Loaded(payload) = &block.payload else {
            panic!("payload should be loaded");
        };
        assert!(payload.plain_text().ends_with('!'));
    }

    #[test]
    fn document_runtime_delete_backward_removes_one_grapheme() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "a👨‍👩‍👧‍👦",
            )],
            720.0,
        );
        runtime.focus_block(1);

        assert!(runtime.delete_backward().unwrap());

        let projection = runtime.projection();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "a");
    }

    #[test]
    fn select_all_marks_visible_projection_without_ui_truth() {
        let mut runtime = DocumentRuntime::demo();
        assert!(runtime.select_all_visible_blocks());

        let projection = runtime.projection();
        assert!(projection.blocks.iter().all(|block| block.selected));
        assert_eq!(projection.blocks.len(), 4);
    }

    #[test]
    fn undo_and_redo_restore_focused_block_text_snapshot() {
        let mut runtime = DocumentRuntime::demo();
        runtime.focus_block(3);
        runtime.insert_char('!').unwrap();

        assert!(runtime.undo_focused_block().unwrap());
        let projection = runtime.projection();
        let block = projection
            .blocks
            .iter()
            .find(|block| block.block_id == 3)
            .unwrap();
        let BlockPayloadView::Loaded(payload) = &block.payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "点击窗口后直接输入文本。");

        assert!(runtime.redo_focused_block().unwrap());
        let projection = runtime.projection();
        let block = projection
            .blocks
            .iter()
            .find(|block| block.block_id == 3)
            .unwrap();
        let BlockPayloadView::Loaded(payload) = &block.payload else {
            panic!("payload should be loaded");
        };
        assert!(payload.plain_text().ends_with('!'));
    }

    #[test]
    fn ctrl_enter_inserts_new_paragraph_after_focused_block() {
        let mut runtime = DocumentRuntime::demo();
        runtime.focus_block(3);

        let new_block_id = runtime.insert_paragraph_after_focused().unwrap();

        assert_eq!(new_block_id, 5);
        assert_eq!(runtime.focused_block_id(), Some(5));
        let projection = runtime.projection();
        assert_eq!(projection.blocks.len(), 5);
        assert_eq!(projection.blocks[3].block_id, 5);
        assert!(matches!(
            projection.blocks[3].kind,
            RichBlockKind::Paragraph
        ));
    }

    #[test]
    fn shift_enter_inserts_soft_line_break_in_focused_block() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "first line",
            )],
            720.0,
        );
        runtime.focus_block(1);
        let before_height = runtime.projection().blocks[0].layout.effective_height();
        let before_total_height = runtime.height_index.total_height();

        runtime.insert_soft_line_break().unwrap();

        let projection = runtime.projection();
        let block = &projection.blocks[0];
        let BlockPayloadView::Loaded(payload) = &block.payload else {
            panic!("payload should be loaded");
        };
        assert!(payload.plain_text().ends_with('\n'));
        assert!(
            block.layout.effective_height() > before_height,
            "soft line break should grow block height: {} <= {before_height}",
            block.layout.effective_height()
        );
        assert!(runtime.height_index.total_height() > before_total_height);
        assert_eq!(
            runtime.page_layout.total_height(),
            runtime.height_index.total_height()
        );
        assert_eq!(
            runtime.scroll.model_total_height,
            runtime.height_index.total_height()
        );
    }

    #[test]
    fn space_shortcut_turns_marker_into_heading_block() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "#",
            )],
            720.0,
        );
        runtime.focus_block(1);

        runtime.insert_space_or_markdown_shortcut().unwrap();

        let projection = runtime.projection();
        assert!(matches!(
            projection.blocks[0].kind,
            RichBlockKind::Heading { level: 1 }
        ));
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "");
    }

    #[test]
    fn enter_shortcut_turns_code_fence_into_code_block() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "```rust",
            )],
            720.0,
        );
        runtime.focus_block(1);

        runtime.handle_enter().unwrap();

        let projection = runtime.projection();
        assert!(matches!(
            projection.blocks[0].kind,
            RichBlockKind::Code { ref language } if language.as_deref() == Some("rust")
        ));
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "");
    }

    #[test]
    fn inline_markdown_shortcut_updates_payload_spans() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "hello **bold**",
            )],
            720.0,
        );
        runtime.focus_block(1);
        runtime.insert_char('!').unwrap();

        let projection = runtime.projection();
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        let BlockPayload::RichText { spans } = &payload.payload else {
            panic!("payload should be rich text");
        };
        assert_eq!(payload.plain_text(), "hello bold!");
        assert!(spans.iter().any(|span| {
            span.text == "bold"
                && span
                    .marks
                    .contains(&crate::core::rich_text::InlineMark::Bold)
        }));
    }

    #[test]
    fn planned_payload_window_without_records_does_not_render_per_block_placeholders() {
        let records = (1..=1_000 as BlockId)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
                .with_layout_meta(crate::core::layout::BlockLayoutMeta::new(block_id, 32.0))
            })
            .collect::<Vec<_>>();
        let payloads = (1..=64 as BlockId)
            .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
            .collect::<Vec<_>>();
        let mut runtime =
            DocumentRuntime::from_index_records_with_window(1, records, payloads, 1, 720.0, 0..64);
        runtime.plan_payload_window_load(400..430);
        runtime
            .scroll
            .scroll_to_global_offset(400.0 * 32.0, crate::editor::scroll::ScrollOrigin::UserWheel)
            .unwrap();

        let projection = runtime.projection_for_window();

        assert!(projection.render_window.is_placeholder());
        assert!(projection.blocks.is_empty());
        assert!(projection.placeholder_window_height.is_some());
    }

    #[test]
    fn payload_window_store_request_prioritizes_focus_and_selection_endpoints() {
        let mut runtime = runtime_with_paragraph_blocks(10);
        runtime.focus_block(5);
        runtime.select_all_visible_blocks();

        let request = runtime.plan_payload_window_load(3..6);

        assert_eq!(request.generation, 1);
        assert_eq!(request.block_range, 3..6);
        assert_eq!(&request.block_ids[..3], &[5, 1, 10]);
        assert!(request.block_ids.contains(&4));
        assert!(request.block_ids.contains(&6));
    }

    #[test]
    fn payload_window_store_discards_stale_generation_result() {
        let mut runtime = runtime_with_paragraph_blocks(4);
        let stale = runtime.plan_payload_window_load(0..2);
        let current = runtime.plan_payload_window_load(2..4);
        assert_eq!(current.generation, 2);

        let decision = runtime.apply_payload_window_result(PayloadWindowLoadResult {
            request: stale,
            records: Vec::new(),
            missing_block_ids: Vec::new(),
        });

        assert_eq!(
            decision,
            PayloadWindowApplyDecision::DiscardedStaleGeneration {
                expected: 2,
                actual: 1,
            }
        );
        assert_eq!(runtime.payload_window.block_range, 2..4);
    }

    #[test]
    fn payload_window_store_marks_loading_and_missing_payload_errors() {
        let records = (1..=3)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
            })
            .collect::<Vec<_>>();
        let mut runtime =
            DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);

        let request = runtime.plan_payload_window_load(0..2);
        assert!(runtime.payload_window.loading.contains(&1));
        assert!(runtime.payload_window.loading.contains(&2));

        let decision = runtime.apply_payload_window_result(PayloadWindowLoadResult {
            request,
            records: Vec::new(),
            missing_block_ids: vec![1, 2],
        });

        assert_eq!(decision, PayloadWindowApplyDecision::Applied);
        assert!(runtime.payload_window.loading.is_empty());
        assert!(runtime.payload_window.failed.contains_key(&1));
        assert!(runtime.payload_window.failed.contains_key(&2));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn payload_window_store_loads_requested_window_from_postgres() {
        let (document_store, payload_store, _layout_store, document, base_block_id) =
            postgres_runtime_fixture(81_001).await;
        let records = sample_index_records(base_block_id, 4);
        let payloads = sample_payloads(base_block_id, 4);
        document_store
            .save_block_index_records(document.id, &records, 1)
            .await
            .unwrap();
        payload_store
            .save_block_payloads(document.id, &payloads)
            .await
            .unwrap();
        let mut runtime = DocumentRuntime::from_index_records_with_window(
            81_001,
            records,
            Vec::new(),
            1,
            720.0,
            0..0,
        );

        let decision = runtime
            .load_payload_window_from_store(&payload_store, 1..3)
            .await
            .unwrap();

        assert_eq!(decision, PayloadWindowApplyDecision::Applied);
        assert_eq!(runtime.payload_window.block_range, 1..3);
        assert_eq!(runtime.payload_window.payloads.len(), 2);
        assert!(
            runtime
                .payload_window
                .payloads
                .contains_key(&(base_block_id + 1))
        );
        assert!(
            runtime
                .payload_window
                .payloads
                .contains_key(&(base_block_id + 2))
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn runtime_from_store_loads_metadata_snapshot_layout_and_initial_payload_window() {
        let (document_store, payload_store, layout_store, document, base_block_id) =
            postgres_runtime_fixture(80_001).await;
        let records = sample_index_records(base_block_id, 4);
        let payloads = sample_payloads(base_block_id, 4);
        document_store
            .save_block_index_records(document.id, &records, 1)
            .await
            .unwrap();
        payload_store
            .save_block_payloads(document.id, &payloads)
            .await
            .unwrap();
        document_store
            .save_document_index_snapshot(document.id, 0, 1, &records)
            .await
            .unwrap();
        let layout_key = runtime_store_layout_key();
        layout_store
            .save_block_layout(
                document.id,
                &crate::storage::layout_cache::BlockLayoutRow::new(
                    base_block_id,
                    layout_key,
                    HeightEstimate::new(123.0, HeightConfidence::Exact, 0.0),
                ),
            )
            .await
            .unwrap();

        let (runtime, report) = DocumentRuntime::from_store(
            document.id,
            &document_store,
            &payload_store,
            &layout_store,
            DocumentRuntimeFromStoreOptions {
                initial_payload_window_blocks: 2,
                layout_key,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(report.document_title, document.title);
        assert_eq!(report.index_source, DocumentRuntimeIndexSource::Snapshot);
        assert_eq!(report.total_blocks, 4);
        assert_eq!(report.payloads_loaded, 2);
        assert_eq!(report.payloads_missing, 0);
        assert_eq!(report.layout_cache_hits, 1);
        assert_eq!(runtime.index.total_count(), 4);
        assert_eq!(runtime.payload_window.block_range, 0..2);
        assert_eq!(runtime.payload_window.payloads.len(), 2);
        assert_eq!(runtime.index.layout_meta[0].measured_height, Some(123.0));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn runtime_from_store_rebuilds_from_blocks_when_snapshot_is_stale() {
        let (document_store, payload_store, layout_store, document, base_block_id) =
            postgres_runtime_fixture(80_002).await;
        let stale_records = sample_index_records(base_block_id, 2);
        document_store
            .save_block_index_records(document.id, &stale_records, 1)
            .await
            .unwrap();
        document_store
            .save_document_index_snapshot(document.id, 0, 1, &stale_records)
            .await
            .unwrap();

        let current_records = sample_index_records(base_block_id, 3);
        let current_payloads = sample_payloads(base_block_id, 1);
        document_store
            .save_block_index_records(document.id, &current_records, 2)
            .await
            .unwrap();
        payload_store
            .save_block_payloads(document.id, &current_payloads)
            .await
            .unwrap();

        let (runtime, report) = DocumentRuntime::from_store(
            document.id,
            &document_store,
            &payload_store,
            &layout_store,
            DocumentRuntimeFromStoreOptions {
                initial_payload_window_blocks: 2,
                layout_key: runtime_store_layout_key(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(report.index_source, DocumentRuntimeIndexSource::Blocks);
        assert_eq!(runtime.index.total_count(), 3);
        assert_eq!(runtime.index.structure_version, 2);
        assert_eq!(report.payloads_loaded, 1);
        assert_eq!(report.payloads_missing, 1);
    }

    async fn postgres_runtime_fixture(
        document_id: u64,
    ) -> (
        crate::storage::postgres::PostgresDocumentStore,
        crate::storage::postgres::PostgresPayloadStore,
        crate::storage::postgres::PostgresLayoutCacheStore,
        crate::storage::postgres::DocumentRow,
        BlockId,
    ) {
        use crate::storage::postgres::{
            DocumentRow, PostgresDocumentStore, PostgresLayoutCacheStore, PostgresPayloadStore,
            PostgresPoolConfig, create_pg_pool, pg_document_id_from_runtime, run_migrations,
        };
        use sqlx::types::Uuid;

        let database_url = std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned());
        let pool = create_pg_pool(&PostgresPoolConfig::for_tests(database_url))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let document_store = PostgresDocumentStore::new(pool.clone());
        let payload_store = PostgresPayloadStore::new(pool.clone());
        let layout_store = PostgresLayoutCacheStore::new(pool);
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let runtime_document_id = document_id + suffix;
        let document = DocumentRow {
            id: pg_document_id_from_runtime(runtime_document_id),
            workspace_id: Uuid::from_u128(
                0x9500_0000_0000_0000_0000_0000_0000_0000 | runtime_document_id as u128,
            ),
            title: format!("Runtime Store {runtime_document_id}"),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        };
        document_store
            .save_document_metadata(&document)
            .await
            .unwrap();
        let base_block_id = runtime_document_id * 10;
        (
            document_store,
            payload_store,
            layout_store,
            document,
            base_block_id,
        )
    }

    fn sample_index_records(base_block_id: BlockId, count: usize) -> Vec<BlockIndexRecord> {
        (0..count)
            .map(|index| {
                BlockIndexRecord::new(
                    base_block_id + index as u64,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
                .with_layout_meta(BlockLayoutMeta::new(base_block_id + index as u64, 32.0))
            })
            .collect()
    }

    fn sample_payloads(base_block_id: BlockId, count: usize) -> Vec<BlockPayloadRecord> {
        (0..count)
            .map(|index| {
                BlockPayloadRecord::rich_text(
                    base_block_id + index as u64,
                    RichBlockKind::Paragraph,
                    format!("payload {index}"),
                )
            })
            .collect()
    }

    fn runtime_store_layout_key() -> LayoutCacheKey {
        LayoutCacheKey {
            width_bucket: 10,
            exact_width_px: 800,
            content_version: 1,
            attrs_version: 0,
            style_version: 0,
            font_version: 0,
            theme_version: 0,
            scale_factor_milli: 1000,
        }
    }
}
