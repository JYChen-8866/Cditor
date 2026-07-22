use std::ops::Range;

use cditor_core::edit::TextAffinity;
use cditor_core::ids::{BlockId, SurfaceId};
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan, RichBlockKind, TableCellPayload,
};
use cditor_viewport::scroll::CaretAnchor;

use super::DocumentRuntime;

mod handle;
mod protocol;
pub use handle::RuntimeTextSurface;
pub use protocol::{RichTextDelta, TextSurface, TextSurfaceEditResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSurfaceRole {
    BlockBody,
    TableCell,
    ImageCaption,
    CollectionTitle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSurfaceCapabilities {
    pub rich_text: bool,
    pub multiline: bool,
    pub inline_objects: bool,
    pub markdown_shortcuts: bool,
    pub block_structure_commands: bool,
}

impl TextSurfaceCapabilities {
    const fn block_body(kind: &RichBlockKind) -> Self {
        let plain = matches!(kind, RichBlockKind::Code { .. } | RichBlockKind::Html);
        let markdown = matches!(kind, RichBlockKind::RawMarkdown | RichBlockKind::Mermaid);
        Self {
            rich_text: !plain && !markdown,
            multiline: true,
            inline_objects: !plain && !markdown,
            markdown_shortcuts: !plain,
            block_structure_commands: !plain && !markdown,
        }
    }

    const fn table_cell() -> Self {
        Self {
            rich_text: true,
            multiline: false,
            inline_objects: true,
            markdown_shortcuts: false,
            block_structure_commands: false,
        }
    }

    const fn image_caption() -> Self {
        Self {
            rich_text: true,
            multiline: true,
            inline_objects: true,
            markdown_shortcuts: false,
            block_structure_commands: false,
        }
    }

    const fn collection_title() -> Self {
        Self {
            rich_text: true,
            multiline: false,
            inline_objects: true,
            markdown_shortcuts: false,
            block_structure_commands: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextSurfaceSnapshotIdentity {
    pub surface_id: SurfaceId,
    pub content_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSurfaceSnapshot {
    pub identity: TextSurfaceSnapshotIdentity,
    pub owner_block_id: BlockId,
    pub role: TextSurfaceRole,
    pub kind: RichBlockKind,
    pub capabilities: TextSurfaceCapabilities,
    pub spans: Vec<InlineSpan>,
}

impl TextSurfaceSnapshot {
    pub fn plain_text(&self) -> String {
        cditor_core::rich_text::plain_text_from_spans(&self.spans)
    }

    pub fn len(&self) -> usize {
        self.spans.iter().map(|span| span.text.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|span| span.text.is_empty())
    }

    pub fn marks_at(&self, offset: usize) -> &[InlineMark] {
        let mut cursor = 0usize;
        for span in &self.spans {
            let end = cursor.saturating_add(span.text.len());
            if offset < end || (offset == end && end == self.len()) {
                return &span.marks;
            }
            cursor = end;
        }
        &[]
    }
}

/// Stateless resolver over the authoritative payload/table stores. Keeping the
/// registry free of text copies prevents a second mutable truth while still
/// centralizing surface discovery and snapshot validation.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextSurfaceRegistry;

impl TextSurfaceRegistry {
    pub fn snapshot(
        &self,
        record: &BlockPayloadRecord,
        surface_id: SurfaceId,
    ) -> Option<TextSurfaceSnapshot> {
        if surface_id.block_id()? != record.block_id {
            return None;
        }
        let (role, kind, capabilities, spans) = match surface_id {
            SurfaceId::Block(_) => {
                let spans = block_body_spans(&record.payload)?;
                (
                    TextSurfaceRole::BlockBody,
                    record.kind.clone(),
                    TextSurfaceCapabilities::block_body(&record.kind),
                    spans,
                )
            }
            SurfaceId::TableCell { row, column, .. } => (
                TextSurfaceRole::TableCell,
                RichBlockKind::Paragraph,
                TextSurfaceCapabilities::table_cell(),
                table_cell(&record.payload, row, column)?.spans.clone(),
            ),
            SurfaceId::ImageCaption { .. } => {
                let BlockPayload::Image(image) = &record.payload else {
                    return None;
                };
                (
                    TextSurfaceRole::ImageCaption,
                    RichBlockKind::Paragraph,
                    TextSurfaceCapabilities::image_caption(),
                    image.caption.spans.clone(),
                )
            }
            SurfaceId::CollectionTitle { .. } => {
                let BlockPayload::Collection(collection) = &record.payload else {
                    return None;
                };
                (
                    TextSurfaceRole::CollectionTitle,
                    RichBlockKind::Heading { level: 3 },
                    TextSurfaceCapabilities::collection_title(),
                    collection.title.spans.clone(),
                )
            }
            SurfaceId::Ephemeral { .. } => return None,
        };
        Some(TextSurfaceSnapshot {
            identity: TextSurfaceSnapshotIdentity {
                surface_id,
                content_version: record.content_version,
            },
            owner_block_id: record.block_id,
            role,
            kind,
            capabilities,
            spans,
        })
    }

    pub fn surface_ids(&self, record: &BlockPayloadRecord) -> Vec<SurfaceId> {
        match &record.payload {
            BlockPayload::RichText { .. }
            | BlockPayload::Code { .. }
            | BlockPayload::Html { .. } => vec![SurfaceId::Block(record.block_id)],
            BlockPayload::Table(table) => table
                .rows
                .iter()
                .enumerate()
                .flat_map(|(row, payload)| {
                    payload
                        .cells
                        .iter()
                        .enumerate()
                        .filter_map(move |(column, cell)| {
                            (!matches!(
                                cell.merge,
                                cditor_core::rich_text::TableCellMerge::Covered { .. }
                            ))
                            .then_some(SurfaceId::TableCell {
                                block_id: record.block_id,
                                row,
                                column,
                            })
                        })
                })
                .collect(),
            BlockPayload::Image(_) => vec![SurfaceId::ImageCaption {
                block_id: record.block_id,
            }],
            BlockPayload::Collection(_) => vec![SurfaceId::CollectionTitle {
                block_id: record.block_id,
            }],
            _ => Vec::new(),
        }
    }

    pub fn validate_identity(
        &self,
        record: &BlockPayloadRecord,
        identity: TextSurfaceSnapshotIdentity,
    ) -> bool {
        record.content_version == identity.content_version
            && self.snapshot(record, identity.surface_id).is_some()
    }
}

impl DocumentRuntime {
    pub(super) fn handle_auxiliary_text_surface_enter(
        &mut self,
        block_id: BlockId,
    ) -> Result<bool, String> {
        match self.focused_text_surface_id() {
            Some(SurfaceId::ImageCaption { .. }) => {
                self.replace_text_in_focused_range(None, "\n")?;
                Ok(true)
            }
            Some(SurfaceId::CollectionTitle { .. }) => {
                self.insert_paragraph_after_block(block_id)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn focus_text_surface_at_offset(
        &mut self,
        surface_id: SurfaceId,
        offset: usize,
    ) -> Result<(), String> {
        self.break_typing_coalescing();
        match surface_id {
            SurfaceId::Block(block_id) => return self.focus_block_at_offset(block_id, offset),
            SurfaceId::TableCell {
                block_id,
                row,
                column,
            } => return self.focus_table_cell_at_offset(block_id, row, column, offset),
            SurfaceId::Ephemeral { .. } => {
                return Err("ephemeral surfaces are not owned by DocumentRuntime".to_owned());
            }
            SurfaceId::ImageCaption { .. } | SurfaceId::CollectionTitle { .. } => {}
        }

        let snapshot = self
            .text_surface_base_snapshot(surface_id)
            .ok_or_else(|| format!("missing text surface {surface_id:?}"))?;
        let target = match surface_id {
            SurfaceId::ImageCaption { block_id } => crate::InputTarget::ImageCaption { block_id },
            SurfaceId::CollectionTitle { block_id } => {
                crate::InputTarget::CollectionTitle { block_id }
            }
            _ => unreachable!("block and table surfaces returned above"),
        };
        if self.prepare_input_focus_transition(target)?
            == super::CompositionFocusTransition::PreservedSameSurface
        {
            return Ok(());
        }

        let text = snapshot.plain_text();
        let offset = super::normalized_grapheme_offset(&text, offset.min(text.len()));
        self.selected_block_ids.clear();
        self.document_selection = None;
        self.visual_caret_position = None;
        self.focused_text_selection = None;
        self.focused_table_cell = None;

        let block_id = snapshot.owner_block_id;
        let session_id = self.allocate_input_session_id();
        let mut editing = crate::EditingSession::start_with_session_id(
            block_id,
            snapshot.identity.content_version,
            offset,
            CaretAnchor {
                block_id,
                caret_rect_y_in_block: 0.0,
                viewport_y: 120.0,
            },
            session_id,
        );
        editing.set_input_target(target);
        editing.set_collapsed_selection(offset);
        self.editing = Some(editing);
        Ok(())
    }

    pub fn focused_text_surface_id(&self) -> Option<SurfaceId> {
        self.input_session_target()?.surface_id()
    }

    pub fn text_surface_caret_offset(&self, surface_id: SurfaceId) -> Option<usize> {
        (self.focused_text_surface_id() == Some(surface_id))
            .then(|| {
                self.editing
                    .as_ref()
                    .map(crate::EditingSession::focus_offset)
            })
            .flatten()
    }

    pub fn text_surface_selection_range(&self, surface_id: SurfaceId) -> Option<Range<usize>> {
        (self.focused_text_surface_id() == Some(surface_id))
            .then(|| {
                self.editing
                    .as_ref()
                    .map(|editing| editing.selected_range.clone())
            })
            .flatten()
    }

    pub fn text_surface_marked_range(&self, surface_id: SurfaceId) -> Option<Range<usize>> {
        (self.focused_text_surface_id() == Some(surface_id))
            .then(|| {
                self.editing
                    .as_ref()
                    .and_then(|editing| editing.marked_range.clone())
            })
            .flatten()
    }

    pub fn move_focused_text_surface_to_offset(
        &mut self,
        surface_id: SurfaceId,
        offset: usize,
        affinity: TextAffinity,
        extend_selection: bool,
    ) -> Result<bool, String> {
        self.break_typing_coalescing();
        match surface_id {
            SurfaceId::Block(block_id) => {
                return self.move_focused_caret_to_text_position(
                    cditor_core::edit::TextPosition {
                        block_id,
                        offset,
                        affinity,
                    },
                    extend_selection,
                );
            }
            SurfaceId::TableCell {
                block_id,
                row,
                column,
            } => {
                if self.focused_table_cell_for_block(block_id)
                    != Some(crate::TableCellPosition { row, col: column })
                {
                    return Ok(false);
                }
                return self.move_focused_table_cell_to_text_position(
                    offset,
                    affinity,
                    extend_selection,
                );
            }
            SurfaceId::ImageCaption { .. } | SurfaceId::CollectionTitle { .. } => {}
            SurfaceId::Ephemeral { .. } => return Ok(false),
        }
        if self.focused_text_surface_id() != Some(surface_id) {
            return Ok(false);
        }
        let snapshot = self
            .text_surface_base_snapshot(surface_id)
            .ok_or_else(|| format!("missing text surface {surface_id:?}"))?;
        let text = snapshot.plain_text();
        let offset = super::normalized_grapheme_offset(&text, offset.min(text.len()));
        let editing = self
            .editing
            .as_mut()
            .ok_or_else(|| "missing editing session".to_owned())?;
        let previous = editing.focus_offset();
        let anchor = editing.anchor_offset();
        let target = editing.input_target;
        editing.set_input_target(target);
        editing.clear_composition();
        if extend_selection {
            let range = anchor.min(offset)..anchor.max(offset);
            editing.set_selected_range(range, offset < anchor);
        } else {
            editing.set_collapsed_selection(offset);
        }
        self.document_selection = None;
        self.focused_text_selection = None;
        self.visual_caret_position = None;
        Ok(previous != offset || extend_selection || affinity != TextAffinity::Downstream)
    }

    pub(super) fn delete_in_auxiliary_text_surface(
        &mut self,
        backwards: bool,
    ) -> Result<Option<bool>, String> {
        let Some(surface_id @ (SurfaceId::ImageCaption { .. } | SurfaceId::CollectionTitle { .. })) =
            self.focused_text_surface_id()
        else {
            return Ok(None);
        };
        let snapshot = self
            .text_surface_base_snapshot(surface_id)
            .ok_or_else(|| format!("missing text surface {surface_id:?}"))?;
        let text = snapshot.plain_text();
        let selection = self
            .editing
            .as_ref()
            .map(|editing| editing.selected_range.clone())
            .unwrap_or_default();
        if !selection.is_empty() {
            return self
                .replace_text_in_focused_range(Some(selection), "")
                .map(Some);
        }
        let caret = self
            .editing
            .as_ref()
            .map(crate::EditingSession::focus_offset)
            .unwrap_or(text.len());
        let caret = super::normalized_grapheme_offset(&text, caret);
        let range = if backwards {
            let previous = super::previous_grapheme_boundary(&text, caret);
            previous..caret
        } else {
            let next = super::next_grapheme_boundary(&text, caret);
            caret..next
        };
        if range.is_empty() {
            return Ok(Some(false));
        }
        self.replace_text_in_focused_range(Some(range), "")
            .map(Some)
    }

    pub(super) fn text_surface_base_snapshot(
        &self,
        surface_id: SurfaceId,
    ) -> Option<TextSurfaceSnapshot> {
        let block_id = surface_id.block_id()?;
        let record = self.payload_window.get(block_id)?.clone();
        let record = self.table_runtime_payload_record(block_id, record);
        TextSurfaceRegistry.snapshot(&record, surface_id)
    }

    pub fn text_surface_snapshot(&self, surface_id: SurfaceId) -> Option<TextSurfaceSnapshot> {
        let mut snapshot = self.text_surface_base_snapshot(surface_id)?;
        let target_is_surface = self
            .input_session_target()
            .and_then(|target| target.surface_id())
            == Some(surface_id);
        if target_is_surface && let Some(composition) = self.active_composition() {
            snapshot.spans = super::text_payload::replace_rich_text_spans_preserving_marks(
                &snapshot.spans,
                composition.range_start as usize..composition.range_end as usize,
                &composition.preview_text,
            );
        }
        Some(snapshot)
    }

    pub fn text_surface_ids_for_block(&self, block_id: BlockId) -> Vec<SurfaceId> {
        let Some(record) = self.payload_window.get(block_id).cloned() else {
            return Vec::new();
        };
        let record = self.table_runtime_payload_record(block_id, record);
        TextSurfaceRegistry.surface_ids(&record)
    }

    pub fn text_surface(&mut self, surface_id: SurfaceId) -> Option<RuntimeTextSurface<'_>> {
        self.text_surface_snapshot(surface_id)?;
        Some(RuntimeTextSurface::new(self, surface_id))
    }

    pub fn validate_text_surface_identity(&self, identity: TextSurfaceSnapshotIdentity) -> bool {
        let Some(block_id) = identity.surface_id.block_id() else {
            return false;
        };
        let Some(record) = self.payload_window.get(block_id).cloned() else {
            return false;
        };
        let record = self.table_runtime_payload_record(block_id, record);
        TextSurfaceRegistry.validate_identity(&record, identity)
    }

    pub fn replace_text_surface_range(
        &mut self,
        surface_id: SurfaceId,
        range: Range<usize>,
        delta: RichTextDelta,
    ) -> Result<TextSurfaceEditResult, String> {
        let before = self
            .text_surface_snapshot(surface_id)
            .ok_or_else(|| format!("missing text surface {surface_id:?}"))?;
        if self
            .input_session_target()
            .and_then(|target| target.surface_id())
            != Some(surface_id)
        {
            return Err(format!(
                "text surface {surface_id:?} is not the active input target"
            ));
        }
        let inserted_text = delta.plain_text();
        let normalized = super::normalized_grapheme_range(&before.plain_text(), range);
        let inserted_range = normalized.start..normalized.start + inserted_text.len();
        let rich_delta =
            delta.spans.len() > 1 || delta.spans.iter().any(|span| !span.marks.is_empty());
        let changed = if rich_delta {
            if let Some(editing) = self.editing.as_mut() {
                editing.set_selected_range(normalized.clone(), false);
            }
            self.paste_clipboard_selection(
                &cditor_import_export::clipboard::ClipboardSelection::Inline { spans: delta.spans },
            )?
        } else {
            self.replace_text_in_focused_range(Some(normalized.clone()), &inserted_text)?
        };
        if !changed {
            return Err(format!("text surface {surface_id:?} rejected replacement"));
        }
        let after = self
            .text_surface_snapshot(surface_id)
            .ok_or_else(|| format!("text surface {surface_id:?} disappeared after edit"))?;
        Ok(TextSurfaceEditResult {
            identity_before: before.identity,
            identity_after: after.identity,
            replaced_range: normalized,
            inserted_range,
        })
    }

    pub(super) fn apply_auxiliary_text_surface_edit(
        &mut self,
        edit: super::FocusedTextEdit,
        text: &str,
    ) -> Result<bool, String> {
        let target = edit.target;
        let Some(surface_id) = target.surface_id() else {
            return Ok(false);
        };
        if !matches!(
            target,
            crate::InputTarget::ImageCaption { .. } | crate::InputTarget::CollectionTitle { .. }
        ) {
            return Ok(false);
        }
        let block_id = target.block_id();
        let snapshot = self
            .text_surface_base_snapshot(surface_id)
            .ok_or_else(|| format!("missing auxiliary text surface {surface_id:?}"))?;
        if edit.range.is_empty() && text.is_empty() {
            return Ok(false);
        }
        let range = super::normalized_grapheme_range(&snapshot.plain_text(), edit.range);
        let next_spans = super::text_payload::replace_rich_text_spans_preserving_marks(
            &snapshot.spans,
            range.clone(),
            text,
        );
        let next_offset = range.start.saturating_add(text.len());

        self.push_undo_snapshot(block_id)?;
        self.cancel_composition();
        self.document_selection = None;
        self.focused_text_selection = None;
        self.focused_table_cell = None;
        self.selected_block_ids.clear();

        let next_content_version = {
            let record = self
                .payload_window
                .payloads
                .get_mut(&block_id)
                .ok_or_else(|| format!("missing payload for block {block_id}"))?;
            match (&mut record.payload, target) {
                (
                    BlockPayload::Image(image),
                    crate::InputTarget::ImageCaption {
                        block_id: target_block_id,
                    },
                ) if target_block_id == block_id => image.caption.spans = next_spans,
                (
                    BlockPayload::Collection(collection),
                    crate::InputTarget::CollectionTitle {
                        block_id: target_block_id,
                    },
                ) if target_block_id == block_id => collection.title.spans = next_spans,
                _ => {
                    return Err(format!(
                        "payload for block {block_id} does not own surface {surface_id:?}"
                    ));
                }
            }
            record.content_version = record.content_version.saturating_add(1);
            record.content_version
        };
        if let Some(editing) = self.editing.as_mut() {
            editing.content_version = next_content_version;
            editing.set_input_target(target);
            editing.set_collapsed_selection(next_offset);
        }
        Ok(true)
    }

    pub(super) fn replace_auxiliary_text_surface_spans(
        &mut self,
        surface_id: SurfaceId,
        spans: Vec<InlineSpan>,
    ) -> Result<bool, String> {
        let target = self
            .input_session_target()
            .filter(|target| target.surface_id() == Some(surface_id))
            .ok_or_else(|| format!("text surface {surface_id:?} is not focused"))?;
        let snapshot = self
            .text_surface_base_snapshot(surface_id)
            .ok_or_else(|| format!("missing text surface {surface_id:?}"))?;
        if snapshot.spans == spans {
            return Ok(false);
        }
        let selection = self
            .editing
            .as_ref()
            .map(|editing| (editing.selected_range.clone(), editing.selection_reversed));
        self.apply_text_surface_format_transaction(surface_id, snapshot.spans, spans)?;
        if let Some(editing) = self.editing.as_mut() {
            editing.set_input_target(target);
            if let Some((range, reversed)) = selection {
                editing.set_selected_range(range, reversed);
            }
        }
        Ok(true)
    }
}

fn block_body_spans(payload: &BlockPayload) -> Option<Vec<InlineSpan>> {
    match payload {
        BlockPayload::RichText { spans } => Some(spans.clone()),
        BlockPayload::Code { text, .. } | BlockPayload::Html { html: text, .. } => {
            Some(vec![InlineSpan::plain(text)])
        }
        _ => None,
    }
}

fn table_cell(payload: &BlockPayload, row: usize, column: usize) -> Option<&TableCellPayload> {
    let BlockPayload::Table(table) = payload else {
        return None;
    };
    table.rows.get(row)?.cells.get(column)
}
