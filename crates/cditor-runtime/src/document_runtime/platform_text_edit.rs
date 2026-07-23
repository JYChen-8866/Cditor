//! Platform and command text replacement routed through replayable local
//! transactions. The PieceTable remains the allocation-sensitive mutation
//! path; the transaction is derived from authoritative before/after surfaces.

use super::*;

impl DocumentRuntime {
    pub(crate) fn replace_text_from_platform(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
    ) -> Result<bool, String> {
        let composition_active = self.active_composition().is_some();
        let typing = self.focused_text_surface_id().and_then(|surface_id| {
            let replacement = range
                .clone()
                .or_else(|| self.input_session_selected_range())?;
            (replacement.is_empty() && !text.is_empty() && !text.contains(['\r', '\n']))
                .then_some((surface_id, replacement.start))
        });
        if let Some((surface_id, offset)) = typing {
            self.prepare_typing_undo(surface_id, offset);
        } else {
            self.break_typing_coalescing();
        }
        let (kind, origin) = if composition_active {
            (
                EditTransactionKind::CompositionCommit,
                cditor_core::edit::ChangeOrigin::Ime,
            )
        } else if typing.is_some() {
            (
                EditTransactionKind::Typing,
                cditor_core::edit::ChangeOrigin::User,
            )
        } else {
            (
                EditTransactionKind::ExplicitCommand,
                cditor_core::edit::ChangeOrigin::User,
            )
        };
        let result = self.replace_text_with_preapplied_transaction(range, text, kind, origin);
        if let Some((surface_id, _)) = typing {
            let changed = matches!(result, Ok(true));
            let next_offset = self.text_surface_caret_offset(surface_id);
            self.finish_typing_undo(surface_id, next_offset, changed);
        }
        result
    }

    pub(crate) fn replace_text_in_focused_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
    ) -> Result<bool, String> {
        self.replace_text_with_preapplied_transaction(
            range,
            text,
            EditTransactionKind::ExplicitCommand,
            cditor_core::edit::ChangeOrigin::User,
        )
    }

    pub(crate) fn replace_text_from_paste(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
    ) -> Result<bool, String> {
        self.break_typing_coalescing();
        self.replace_text_with_preapplied_transaction(
            range,
            text,
            EditTransactionKind::Paste,
            cditor_core::edit::ChangeOrigin::Import,
        )
    }

    pub(super) fn replace_text_from_ai(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
    ) -> Result<bool, String> {
        self.break_typing_coalescing();
        self.replace_text_with_preapplied_transaction(
            range,
            text,
            EditTransactionKind::AiApply,
            cditor_core::edit::ChangeOrigin::Ai,
        )
    }

    pub(super) fn replace_text_from_composition(
        &mut self,
        range: Range<usize>,
        text: &str,
    ) -> Result<bool, String> {
        self.break_typing_coalescing();
        self.replace_text_with_preapplied_transaction(
            Some(range),
            text,
            EditTransactionKind::CompositionCommit,
            cditor_core::edit::ChangeOrigin::Ime,
        )
    }

    fn replace_text_with_preapplied_transaction(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        kind: EditTransactionKind,
        origin: cditor_core::edit::ChangeOrigin,
    ) -> Result<bool, String> {
        let explicit_range = range.clone();
        let Some(edit) = self.resolve_focused_text_edit(range) else {
            return Ok(false);
        };
        if edit.range.is_empty() && text.is_empty() {
            return Ok(false);
        }
        let Some(surface_id) = edit.target.surface_id() else {
            return Ok(false);
        };
        let block_id = surface_id
            .block_id()
            .ok_or_else(|| "ephemeral surface cannot be edited by DocumentRuntime".to_owned())?;
        let before_record = self
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        let before_surface = self
            .text_surface_base_snapshot(surface_id)
            .ok_or_else(|| format!("missing text surface {surface_id:?}"))?;
        let before_selection = matches!(surface_id, SurfaceId::Block(_))
            .then(|| self.document_selection_snapshot())
            .flatten();
        let before_anchor = self.capture_undo_scroll_snapshot().anchor;
        let before_layout_version = self
            .index
            .index_of(block_id)
            .map(|index| self.index.layout_meta[index].layout_version)
            .ok_or_else(|| format!("missing layout metadata for block {block_id}"))?;
        let replaced_range = edit.range.clone();
        super::local_transaction::validate_preapplied_text_preflight(
            surface_id,
            &before_record,
            &before_surface,
            replaced_range.clone(),
        )?;
        self.validate_preapplied_surface_truth(&before_surface)?;

        if !self.apply_resolved_focused_text_edit(edit, text, explicit_range)? {
            return Ok(false);
        }
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.saturating_add(1);

        let after_record = self
            .payload_window
            .get(block_id)
            .cloned()
            .expect("pre-applied text edit must preserve its owner payload");
        let after_surface = self.text_surface_base_snapshot(surface_id);
        let after_selection = matches!(surface_id, SurfaceId::Block(_))
            .then(|| self.document_selection_snapshot())
            .flatten();
        let after_anchor = self.capture_undo_scroll_snapshot().anchor;
        self.record_preapplied_text_replacement(super::local_transaction::PreappliedTextEdit {
            transaction_id,
            kind,
            origin,
            surface_id,
            replaced_range,
            inserted_text: text.to_owned(),
            before_record,
            after_record,
            before_surface,
            after_surface,
            before_selection,
            after_selection,
            before_anchor,
            after_anchor,
            before_layout_version,
        });
        Ok(true)
    }

    fn validate_preapplied_surface_truth(
        &self,
        before_surface: &TextSurfaceSnapshot,
    ) -> Result<(), String> {
        let authoritative_text = before_surface.plain_text();
        let live_text = match before_surface.identity.surface_id {
            SurfaceId::Block(block_id) => self
                .text_models
                .get(&block_id)
                .map(|model| model.text().to_owned()),
            SurfaceId::TableCell {
                block_id,
                row,
                column,
            } => self.table_cell_plain_text(block_id, row, column),
            SurfaceId::ImageCaption { .. } | SurfaceId::CollectionTitle { .. } => {
                Some(authoritative_text.clone())
            }
            SurfaceId::Ephemeral { .. } => None,
        };
        match live_text {
            Some(live_text) if live_text == authoritative_text => Ok(()),
            Some(live_text) => Err(format!(
                "text surface {:?} live text ({} bytes) diverges from authoritative payload ({} bytes)",
                before_surface.identity.surface_id,
                live_text.len(),
                authoritative_text.len()
            )),
            None => Err(format!(
                "text surface {:?} has no live edit model",
                before_surface.identity.surface_id
            )),
        }
    }

    fn apply_resolved_focused_text_edit(
        &mut self,
        edit: FocusedTextEdit,
        text: &str,
        explicit_range: Option<Range<usize>>,
    ) -> Result<bool, String> {
        if matches!(edit.target, InputTarget::TableCell { .. }) {
            return self.apply_focused_table_cell_text_edit(edit, text);
        }
        if matches!(
            edit.target,
            InputTarget::ImageCaption { .. } | InputTarget::CollectionTitle { .. }
        ) {
            return self.apply_auxiliary_text_surface_edit(edit, text);
        }
        let InputTarget::BlockText { block_id } = edit.target else {
            return Ok(false);
        };
        if self.focused_block_is_table() {
            self.cancel_composition();
            return Ok(false);
        }
        let range = edit.range;
        trace_input(
            "replace_text_in_focused_range.range",
            format_args!(
                "block={block_id} explicit_range={explicit_range:?} resolved_range={range:?} insert_len={} caret_before={:?} focused_selection={:?} active_composition={:?}",
                text.len(),
                self.caret_offset_for_block(block_id),
                self.focused_text_selection_range(),
                self.active_composition()
                    .map(|composition| composition.range_start as usize
                        ..composition.range_end as usize)
            ),
        );
        if text == " "
            && range.is_empty()
            && self.try_apply_space_block_markdown_shortcut(block_id, range.start)?
        {
            trace_input(
                "replace_text_in_focused_range.space_shortcut",
                format_args!("block={block_id} shortcut_offset={}", range.start),
            );
            return Ok(true);
        }

        self.cancel_composition();
        self.document_selection = None;
        self.focused_text_selection = None;
        self.push_undo_snapshot(block_id)?;
        let replaced_range = range.clone();
        let replacement_start = range.start;
        let surface_id = SurfaceId::Block(block_id);
        let typing_marks = self.typing_marks_for(surface_id, replacement_start);
        let (content_version, text_len_after, next_offset) = {
            let model = self
                .text_models
                .get_mut(&block_id)
                .ok_or_else(|| format!("missing text model for block {block_id}"))?;
            let inserted = model
                .replace_range(range, text)
                .map_err(|error| format!("{error:?}"))?;
            let editing = self.editing.as_mut().expect("editing session exists");
            editing.content_version += 1;
            editing.set_input_target(InputTarget::BlockText { block_id });
            editing.set_collapsed_selection(inserted.end);
            super::typing_marks::sync_payload_after_replace_with_typing_marks(
                &mut self.payload_window,
                block_id,
                editing.content_version,
                model,
                replaced_range,
                text,
                typing_marks,
            );
            (editing.content_version, model.len(), inserted.end)
        };
        let applied_shortcut = self.apply_inline_markdown_shortcut(block_id)?;
        if !applied_shortcut {
            self.advance_typing_mark_override(surface_id, replacement_start, next_offset);
        }
        trace_input(
            "replace_text_in_focused_range.end",
            format_args!(
                "block={block_id} caret_after={:?} content_version={content_version} text_len={text_len_after}",
                self.caret_offset_for_block(block_id)
            ),
        );
        Ok(true)
    }
}
