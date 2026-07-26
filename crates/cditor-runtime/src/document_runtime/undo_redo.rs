use super::*;
use cditor_core::edit::{ExternalUndoBlobRef, UndoExternalizationJob};

const TYPING_MERGE_WINDOW: Duration = Duration::from_millis(1_000);
const TEXT_UNDO_MAX_STEPS: usize = 1_000;
const TEXT_UNDO_MAX_STEPS_PER_BLOCK: usize = 100;
pub(crate) const TEXT_UNDO_MAX_ESTIMATED_BYTES: usize = 32 * 1024 * 1024;

impl DocumentRuntime {
    /// Ends the current realtime typing batch before a non-text command or an
    /// external driver yields control. This does not mutate document content.
    pub fn end_input_batch(&mut self) {
        self.break_typing_coalescing();
    }

    /// Moves one large undo transaction out of the Runtime so an async
    /// persistence worker can spill it without holding a Runtime borrow.
    pub fn begin_external_undo_spill(&mut self) -> Option<UndoExternalizationJob> {
        self.history.external_undo_stack.begin_externalization()
    }

    pub fn complete_external_undo_spill(
        &mut self,
        job: UndoExternalizationJob,
        reference: ExternalUndoBlobRef,
    ) -> Result<(), UndoExternalizationJob> {
        self.history
            .external_undo_stack
            .complete_externalization(job, reference)
    }

    pub fn abort_external_undo_spill(&mut self, job: UndoExternalizationJob) -> bool {
        self.history.external_undo_stack.abort_externalization(job)
    }

    pub fn hydrate_external_undo(
        &mut self,
        snapshot_id: u64,
        transaction: EditTransaction,
    ) -> bool {
        self.history
            .external_undo_stack
            .hydrate_externalized(snapshot_id, transaction)
    }

    pub fn drain_orphaned_external_undo_blobs(&mut self) -> Vec<ExternalUndoBlobRef> {
        self.history
            .external_undo_stack
            .drain_orphaned_external_blobs()
    }

    pub fn restore_orphaned_external_undo_blobs(
        &mut self,
        references: impl IntoIterator<Item = ExternalUndoBlobRef>,
    ) {
        self.history
            .external_undo_stack
            .restore_orphaned_external_blobs(references);
    }

    pub fn pending_undo_hydration(&self) -> Option<ExternalUndoBlobRef> {
        self.history
            .external_undo_stack
            .next_undo_external_blob()
            .cloned()
    }

    pub fn pending_redo_hydration(&self) -> Option<ExternalUndoBlobRef> {
        self.history
            .external_undo_stack
            .next_redo_external_blob()
            .cloned()
    }

    pub(crate) fn undo_focused_block(&mut self) -> Result<bool, String> {
        self.break_typing_coalescing();
        let Some(event) = self.history.undo_events.pop() else {
            return Ok(false);
        };
        match event {
            RuntimeUndoEvent::Text(block_id) => {
                let Some(previous) = self
                    .history
                    .undo_stacks
                    .get_mut(&block_id)
                    .and_then(Vec::pop)
                else {
                    return Ok(false);
                };
                let current = self.snapshot(block_id)?;
                self.history
                    .redo_stacks
                    .entry(block_id)
                    .or_default()
                    .push(current);
                self.restore_snapshot(block_id, previous)?;
                self.history.redo_events.push(event);
                self.trim_runtime_undo_history();
                Ok(true)
            }
            RuntimeUndoEvent::ExternalTransaction => {
                let Some(mut step) = self.history.external_undo_stack.take_undo_step() else {
                    return Ok(false);
                };
                let Some(transaction) = step.payload.transaction_mut() else {
                    self.history.external_undo_stack.rollback_undo_step(step);
                    self.history.undo_events.push(event);
                    return Err("external undo transaction requires hydration".to_owned());
                };
                std::mem::swap(&mut transaction.ops, &mut transaction.inverse_ops);
                let apply_result = self
                    .apply_external_transaction(transaction, cditor_core::edit::ChangeOrigin::Undo)
                    .map_err(|error| error.to_string());
                std::mem::swap(&mut transaction.ops, &mut transaction.inverse_ops);
                if let Err(error) = apply_result {
                    self.history.external_undo_stack.rollback_undo_step(step);
                    self.history.undo_events.push(event);
                    return Err(error);
                }
                let ux_result = self.restore_transaction_ux(transaction, false);
                self.history.external_undo_stack.commit_undo_step(step);
                self.history.redo_events.push(event);
                ux_result?;
                Ok(true)
            }
        }
    }

    pub(crate) fn redo_focused_block(&mut self) -> Result<bool, String> {
        self.break_typing_coalescing();
        let Some(event) = self.history.redo_events.pop() else {
            return Ok(false);
        };
        match event {
            RuntimeUndoEvent::Text(block_id) => {
                let Some(next) = self
                    .history
                    .redo_stacks
                    .get_mut(&block_id)
                    .and_then(Vec::pop)
                else {
                    return Ok(false);
                };
                let current = self.snapshot(block_id)?;
                self.history
                    .undo_stacks
                    .entry(block_id)
                    .or_default()
                    .push(current);
                self.restore_snapshot(block_id, next)?;
                self.history.undo_events.push(event);
                self.trim_runtime_undo_history();
                Ok(true)
            }
            RuntimeUndoEvent::ExternalTransaction => {
                let Some(mut step) = self.history.external_undo_stack.take_redo_step() else {
                    return Ok(false);
                };
                let Some(transaction) = step.payload.transaction_mut() else {
                    self.history.external_undo_stack.rollback_redo_step(step);
                    self.history.redo_events.push(event);
                    return Err("external redo transaction requires hydration".to_owned());
                };
                if let Err(error) = self
                    .apply_external_transaction(transaction, cditor_core::edit::ChangeOrigin::Redo)
                    .map_err(|error| error.to_string())
                {
                    self.history.external_undo_stack.rollback_redo_step(step);
                    self.history.redo_events.push(event);
                    return Err(error);
                }
                let ux_result = self.restore_transaction_ux(transaction, true);
                self.history.external_undo_stack.commit_redo_step(step);
                self.history.undo_events.push(event);
                ux_result?;
                Ok(true)
            }
        }
    }

    pub(super) fn snapshot(&self, block_id: BlockId) -> Result<TextSnapshot, String> {
        let payload = self
            .document
            .payload_window
            .get(block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {block_id}"))?;
        Ok(TextSnapshot {
            kind: payload.kind,
            payload: payload.payload,
            content_version: payload.content_version,
            focused_table_cell: self
                .selection
                .focused_table_cell
                .filter(|focused| focused.block_id == block_id),
            input_target: self
                .editing
                .session
                .as_ref()
                .filter(|editing| editing.block_id == block_id)
                .map(|editing| editing.input_target),
            selected_range: self
                .editing
                .session
                .as_ref()
                .filter(|editing| editing.block_id == block_id)
                .map(|editing| editing.selected_range.clone()),
            selection_reversed: self
                .editing
                .session
                .as_ref()
                .filter(|editing| editing.block_id == block_id)
                .is_some_and(|editing| editing.selection_reversed),
            scroll: self.capture_undo_scroll_snapshot(),
        })
    }

    pub(super) fn push_undo_snapshot(&mut self, block_id: BlockId) -> Result<(), String> {
        if let Some(request) = self.history.pending_typing_undo {
            if self.history.typing_undo_group.is_some_and(|group| {
                group.surface_id == request.surface_id
                    && group.next_offset == request.offset
                    && request.started_at.duration_since(group.last_input_at) <= TYPING_MERGE_WINDOW
            }) {
                return Ok(());
            }
            self.push_undo_snapshot_uncoalesced(block_id)?;
            self.history.typing_undo_group = Some(TypingUndoGroup {
                surface_id: request.surface_id,
                next_offset: request.offset,
                last_input_at: request.started_at,
            });
            return Ok(());
        }
        self.break_typing_coalescing();
        self.push_undo_snapshot_uncoalesced(block_id)
    }

    fn push_undo_snapshot_uncoalesced(&mut self, block_id: BlockId) -> Result<(), String> {
        let snapshot = self.snapshot(block_id)?;
        let stack = self.history.undo_stacks.entry(block_id).or_default();
        if stack.last() != Some(&snapshot) {
            stack.push(snapshot);
            self.history
                .undo_events
                .push(RuntimeUndoEvent::Text(block_id));
            self.history.redo_stacks.clear();
            self.history.redo_events.clear();
            self.history.external_undo_stack.clear_redo();
            while self.history.undo_stacks.get(&block_id).map_or(0, Vec::len)
                > TEXT_UNDO_MAX_STEPS_PER_BLOCK
            {
                remove_oldest_text_history(
                    &mut self.history.undo_events,
                    &mut self.history.undo_stacks,
                    Some(block_id),
                );
            }
            self.trim_runtime_undo_history();
        }
        Ok(())
    }

    pub(super) fn estimated_text_history_memory_bytes(&self) -> usize {
        estimated_text_history_bytes(&self.history.undo_stacks)
            .saturating_add(estimated_text_history_bytes(&self.history.redo_stacks))
    }

    pub(super) fn clear_runtime_redo_history(&mut self) {
        self.history.redo_stacks.clear();
        self.history.redo_events.clear();
        self.history.external_undo_stack.clear_redo();
    }

    pub(super) fn trim_runtime_undo_history(&mut self) {
        while self
            .history
            .undo_events
            .iter()
            .filter(|event| matches!(event, RuntimeUndoEvent::Text(_)))
            .count()
            > TEXT_UNDO_MAX_STEPS
        {
            if !remove_oldest_text_history(
                &mut self.history.undo_events,
                &mut self.history.undo_stacks,
                None,
            ) {
                break;
            }
        }

        while self
            .history
            .undo_events
            .iter()
            .filter(|event| matches!(event, RuntimeUndoEvent::ExternalTransaction))
            .count()
            > self.history.external_undo_stack.undo_len()
        {
            let Some(position) = self
                .history
                .undo_events
                .iter()
                .position(|event| matches!(event, RuntimeUndoEvent::ExternalTransaction))
            else {
                break;
            };
            self.history.undo_events.remove(position);
        }

        while self.estimated_text_history_memory_bytes() > TEXT_UNDO_MAX_ESTIMATED_BYTES
            && text_history_snapshot_count(&self.history.undo_stacks, &self.history.redo_stacks) > 1
        {
            if remove_oldest_text_history(
                &mut self.history.undo_events,
                &mut self.history.undo_stacks,
                None,
            ) {
                continue;
            }
            if !remove_oldest_text_history(
                &mut self.history.redo_events,
                &mut self.history.redo_stacks,
                None,
            ) {
                break;
            }
        }
    }

    pub(super) fn prepare_typing_undo(&mut self, surface_id: SurfaceId, offset: usize) {
        self.history.pending_typing_undo = Some(TypingUndoRequest {
            surface_id,
            offset,
            started_at: Instant::now(),
        });
    }

    pub(super) fn finish_typing_undo(
        &mut self,
        surface_id: SurfaceId,
        next_offset: Option<usize>,
        changed: bool,
    ) {
        let request = self.history.pending_typing_undo.take();
        if !changed {
            self.history.typing_undo_group = None;
            return;
        }
        if let (Some(group), Some(next_offset)) = (&mut self.history.typing_undo_group, next_offset)
            && group.surface_id == surface_id
        {
            group.next_offset = next_offset;
            if let Some(request) = request {
                group.last_input_at = request.started_at;
            }
        }
    }

    pub(crate) fn break_typing_coalescing(&mut self) {
        self.history.pending_typing_undo = None;
        self.history.typing_undo_group = None;
        self.editing.typing_mark_override = None;
    }

    pub(super) fn restore_snapshot(
        &mut self,
        block_id: BlockId,
        snapshot: TextSnapshot,
    ) -> Result<(), String> {
        let scroll = snapshot.scroll;
        let text = snapshot.payload.plain_text();
        let input_target = snapshot.input_target;
        let selected_range = snapshot.selected_range.clone();
        let selection_reversed = snapshot.selection_reversed;
        let focused_table_cell = snapshot.focused_table_cell;
        self.replace_block_kind_and_payload(block_id, snapshot.kind, snapshot.payload)?;
        let _ = self.refresh_table_block_height(block_id)?;
        if let Some(payload) = self.document.payload_window.get_mut(block_id) {
            payload.content_version = snapshot.content_version;
        }
        match input_target {
            Some(
                target @ (InputTarget::ImageCaption { .. } | InputTarget::CollectionTitle { .. }),
            ) => {
                let offset = selected_range
                    .as_ref()
                    .map(|range| {
                        if selection_reversed {
                            range.start
                        } else {
                            range.end
                        }
                    })
                    .unwrap_or(text.len());
                if let Some(surface_id) = target.surface_id() {
                    self.focus_text_surface_at_offset(surface_id, offset)?;
                }
            }
            Some(InputTarget::TableCell { block_id, row, col }) => {
                let offset = focused_table_cell
                    .map(|cell| cell.offset)
                    .unwrap_or_default();
                self.focus_table_cell_at_offset(block_id, row, col, offset)?;
            }
            Some(InputTarget::BlockText { block_id }) => {
                let offset = selected_range
                    .as_ref()
                    .map(|range| {
                        if selection_reversed {
                            range.start
                        } else {
                            range.end
                        }
                    })
                    .unwrap_or(text.len());
                if self.document.text_models.contains_key(&block_id) {
                    self.focus_block_at_offset(block_id, offset)?;
                } else {
                    self.focus_block(block_id);
                }
            }
            Some(InputTarget::ComplexBlock { block_id })
            | Some(InputTarget::BlockChrome { block_id }) => self.focus_block(block_id),
            None => {
                if self.focused_block_id() != Some(block_id) {
                    self.focus_block(block_id);
                }
            }
        }
        if let Some(editing) = self.editing.session.as_mut() {
            editing.content_version = snapshot.content_version;
            if let Some(range) = selected_range {
                editing.set_selected_range(range, selection_reversed);
            } else {
                editing.set_collapsed_selection(text.len());
            }
        }
        self.restore_snapshot_table_focus(block_id, focused_table_cell);
        self.selection.selected_block_ids.clear();
        self.restore_undo_scroll_snapshot(scroll)?;
        Ok(())
    }

    fn restore_transaction_ux(
        &mut self,
        transaction: &EditTransaction,
        forward: bool,
    ) -> Result<(), String> {
        let selection = if forward {
            transaction.after_selection
        } else {
            transaction.before_selection
        };
        self.restore_document_selection_ux(selection)?;
        let selected_blocks = if forward {
            &transaction.after_selected_blocks
        } else {
            &transaction.before_selected_blocks
        };
        self.restore_selected_blocks_ux(selected_blocks);
        let anchor = if forward {
            transaction.after_anchor
        } else {
            transaction.before_anchor
        };
        if anchor.is_some() {
            self.restore_scroll_anchor(anchor, self.layout.scroll.global_scroll_top)?;
        }
        Ok(())
    }

    fn restore_document_selection_ux(
        &mut self,
        selection: Option<DocumentSelection>,
    ) -> Result<(), String> {
        let Some(selection) = selection else {
            return Ok(());
        };
        self.hydrate_payload_runtime_state(selection.anchor.block_id);
        self.hydrate_payload_runtime_state(selection.focus.block_id);
        self.set_document_selection(selection)?;
        Ok(())
    }

    fn restore_selected_blocks_ux(&mut self, block_ids: &[BlockId]) {
        self.selection.selected_block_ids = block_ids
            .iter()
            .copied()
            .filter(|block_id| self.document.index.index_of(*block_id).is_some())
            .collect();
        if !self.selection.selected_block_ids.is_empty() {
            self.selection.document_selection = None;
            self.selection.focused_text_selection = None;
            self.selection.focused_table_cell = None;
            self.editing.session = None;
        }
    }

    fn restore_snapshot_table_focus(
        &mut self,
        block_id: BlockId,
        focused: Option<FocusedTableCell>,
    ) {
        let Some(mut focused) = focused.filter(|focused| focused.block_id == block_id) else {
            if self
                .selection
                .focused_table_cell
                .is_some_and(|focused| focused.block_id == block_id)
            {
                self.selection.focused_table_cell = None;
            }
            return;
        };
        let Some(text) = self.table_cell_plain_text(block_id, focused.row, focused.col) else {
            self.selection.focused_table_cell = None;
            return;
        };
        focused.offset = normalized_grapheme_offset(&text, focused.offset);
        let selected = normalized_grapheme_range(
            &text,
            focused.selected_range_start..focused.selected_range_end,
        );
        focused.selected_range_start = selected.start;
        focused.selected_range_end = selected.end;
        if let (Some(start), Some(end)) = (focused.marked_range_start, focused.marked_range_end) {
            let marked = normalized_grapheme_range(&text, start..end);
            focused.marked_range_start = Some(marked.start);
            focused.marked_range_end = Some(marked.end);
        } else {
            focused.marked_range_start = None;
            focused.marked_range_end = None;
        }
        self.selection.focused_table_cell = Some(focused);
        if let Some(editing) = self.editing.session.as_mut() {
            editing.set_input_target(InputTarget::TableCell {
                block_id,
                row: focused.row,
                col: focused.col,
            });
            editing.set_selected_range(focused.selected_range(), focused.selection_reversed);
            if let Some(marked_range) = focused.marked_range() {
                editing.set_marked_range(marked_range);
            } else {
                editing.clear_composition();
            }
        }
    }
}

fn estimated_text_history_bytes(stacks: &HashMap<BlockId, Vec<TextSnapshot>>) -> usize {
    stacks
        .values()
        .flatten()
        .map(|snapshot| {
            std::mem::size_of::<TextSnapshot>().saturating_add(
                crate::content::payload_cache::estimated_owned_block_payload_bytes(
                    &snapshot.kind,
                    &snapshot.payload,
                ),
            )
        })
        .sum()
}

fn text_history_snapshot_count(
    undo_stacks: &HashMap<BlockId, Vec<TextSnapshot>>,
    redo_stacks: &HashMap<BlockId, Vec<TextSnapshot>>,
) -> usize {
    undo_stacks
        .values()
        .chain(redo_stacks.values())
        .map(Vec::len)
        .sum()
}

fn remove_oldest_text_history(
    events: &mut Vec<RuntimeUndoEvent>,
    stacks: &mut HashMap<BlockId, Vec<TextSnapshot>>,
    required_block_id: Option<BlockId>,
) -> bool {
    let Some(position) = events.iter().position(|event| {
        matches!(
            event,
            RuntimeUndoEvent::Text(block_id)
                if required_block_id.is_none_or(|required| required == *block_id)
        )
    }) else {
        return false;
    };
    let RuntimeUndoEvent::Text(block_id) = events.remove(position) else {
        unreachable!("matched text undo event");
    };
    let remove_stack = if let Some(stack) = stacks.get_mut(&block_id) {
        if !stack.is_empty() {
            stack.remove(0);
        }
        stack.is_empty()
    } else {
        false
    };
    if remove_stack {
        stacks.remove(&block_id);
    }
    true
}
